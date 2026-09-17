//! Native helper calls retain QEMU memory and arithmetic behavior.
use super::{Error, Ir, Result, abi};
use llvm_sys::{
    core::{
        LLVMBuildAlloca, LLVMBuildCall2, LLVMBuildIntToPtr, LLVMBuildLShr, LLVMBuildOr,
        LLVMBuildShl, LLVMConstIntToPtr, LLVMFunctionType, LLVMPointerTypeInContext,
        LLVMSetAlignment, LLVMTypeOf, LLVMVoidTypeInContext,
    },
    prelude::{LLVMTypeRef, LLVMValueRef},
};
impl Ir<'_> {
    pub(super) fn native_call(
        &self,
        address: usize,
        result: LLVMTypeRef,
        args: &mut [LLVMValueRef],
    ) -> Result<LLVMValueRef> {
        let count = u32::try_from(args.len()).map_err(|_| Error::Native)?;
        // SAFETY: the SDK supplies function addresses; each call site uses its declared C signature.
        unsafe {
            let mut types: Vec<_> = args.iter().map(|&value| LLVMTypeOf(value)).collect();
            let function_type = LLVMFunctionType(result, types.as_mut_ptr(), count, 0);
            let address = u64::try_from(address).map_err(|_| Error::Native)?;
            let function = LLVMConstIntToPtr(
                self.constant(64, address),
                LLVMPointerTypeInContext(self.context, 0),
            );
            Ok(LLVMBuildCall2(
                self.builder,
                function_type,
                function,
                args.as_mut_ptr(),
                count,
                c"".as_ptr(),
            ))
        }
    }
    pub(super) fn memory(
        &self,
        name: &str,
        op: &abi::OplabOp,
        v: &[LLVMValueRef],
        memop: u64,
    ) -> Result<()> {
        // SAFETY: guest memory access is delegated to the native MMU, including pair
        // accesses. Splitting a 128-bit access into two independent accesses would change faults.
        unsafe {
            let pointer = self.pointer(self.env, 0);
            let memop = self.constant(32, memop);
            match name {
                "qemu_ld" => {
                    let value = self.native_call(
                        self.helpers[0],
                        self.int(64),
                        &mut [pointer, self.cast(v[0], 64, false), memop],
                    )?;
                    self.put(op.args[0], value)
                }
                "qemu_st" => {
                    self.native_call(
                        self.helpers[1],
                        LLVMVoidTypeInContext(self.context),
                        &mut [
                            pointer,
                            self.cast(v[1], 64, false),
                            self.cast(v[0], 64, false),
                            memop,
                        ],
                    )?;
                    Ok(())
                }
                "qemu_ld2" => {
                    let buffer = LLVMBuildAlloca(self.builder, self.int(128), c"pair".as_ptr());
                    LLVMSetAlignment(buffer, 16);
                    self.native_call(
                        self.helpers[2],
                        LLVMVoidTypeInContext(self.context),
                        &mut [pointer, self.cast(v[0], 64, false), memop, buffer],
                    )?;
                    let pair = self.load(buffer, 128);
                    self.put(op.args[0], pair)?;
                    self.put(
                        op.args[1],
                        LLVMBuildLShr(self.builder, pair, self.constant(128, 64), c"".as_ptr()),
                    )
                }
                "qemu_st2" => {
                    self.native_call(
                        self.helpers[3],
                        LLVMVoidTypeInContext(self.context),
                        &mut [
                            pointer,
                            self.cast(v[2], 64, false),
                            self.cast(v[0], 64, false),
                            self.cast(v[1], 64, false),
                            memop,
                        ],
                    )?;
                    Ok(())
                }
                _ => Err(Error::Native),
            }
        }
    }
    pub(super) fn call(&self, op: &abi::OplabOp, values: &[LLVMValueRef]) -> Result<()> {
        // SAFETY: the signature is derived from the same QEMU helper metadata as
        // its address. I128 temporaries use two consecutive 64-bit host pieces.
        unsafe {
            let mut values = values.iter();
            let mut arguments = Vec::new();
            for parameter in &op.signature[1..] {
                if parameter.bits == 0 {
                    break;
                }
                let value = *values.next().ok_or(Error::Native)?;
                let value = if parameter.bits == 128 {
                    let high = *values.next().ok_or(Error::Native)?;
                    LLVMBuildOr(
                        self.builder,
                        self.cast(value, 128, false),
                        LLVMBuildShl(
                            self.builder,
                            self.cast(high, 128, false),
                            self.constant(128, 64),
                            c"".as_ptr(),
                        ),
                        c"".as_ptr(),
                    )
                } else {
                    self.cast(value, parameter.bits, parameter.sign_extend)
                };
                arguments.push(if parameter.pointer {
                    LLVMBuildIntToPtr(
                        self.builder,
                        value,
                        LLVMPointerTypeInContext(self.context, 0),
                        c"".as_ptr(),
                    )
                } else {
                    value
                });
            }
            if values.next().is_some() {
                return Err(Error::Native);
            }
            let result = op.signature[0];
            let ty = if result.pointer {
                LLVMPointerTypeInContext(self.context, 0)
            } else if result.bits == 0 {
                LLVMVoidTypeInContext(self.context)
            } else {
                self.int(result.bits)
            };
            let value = self.native_call(op.helper, ty, &mut arguments)?;
            if op.outputs > 0 {
                self.put(op.args[0], value)?;
            }
            if op.outputs == 2 {
                self.put(
                    op.args[1],
                    LLVMBuildLShr(self.builder, value, self.constant(128, 64), c"".as_ptr()),
                )?;
            }
            Ok(())
        }
    }
}
