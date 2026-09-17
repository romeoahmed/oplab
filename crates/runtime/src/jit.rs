//! LLVM handles stay on one thread; ORC trackers bound the lifetime of compiled blocks.
#![expect(
    unsafe_code,
    reason = "LLVM's C API uses opaque handles owned by this module"
)]
mod helpers;
mod integer;
mod vector;

use crate::{
    Error,
    qemu::{Block, abi},
};
use llvm_sys::{
    LLVMIntPredicate, LLVMTypeKind,
    analysis::{LLVMVerifierFailureAction, LLVMVerifyModule},
    core::{
        LLVMAddFunction, LLVMAppendBasicBlockInContext, LLVMBuildAlloca, LLVMBuildAnd, LLVMBuildBr,
        LLVMBuildCall2, LLVMBuildCondBr, LLVMBuildFence, LLVMBuildGEP2, LLVMBuildICmp,
        LLVMBuildIntCast2, LLVMBuildIntToPtr, LLVMBuildLoad2, LLVMBuildPtrToInt, LLVMBuildRetVoid,
        LLVMBuildStore, LLVMConstInt, LLVMConstIntOfArbitraryPrecision, LLVMConstNull,
        LLVMContextCreate, LLVMContextDispose, LLVMCountParams, LLVMCreateBuilderInContext,
        LLVMDisposeBuilder, LLVMDisposeMessage, LLVMDisposeModule, LLVMFunctionType,
        LLVMGetIntrinsicDeclaration, LLVMGetParam, LLVMGetTypeKind, LLVMGlobalGetValueType,
        LLVMInt1TypeInContext, LLVMInt64TypeInContext, LLVMIntTypeInContext, LLVMLookupIntrinsicID,
        LLVMModuleCreateWithNameInContext, LLVMPointerTypeInContext, LLVMPositionBuilderAtEnd,
        LLVMSetAlignment, LLVMSetDataLayout, LLVMSetTarget, LLVMTypeOf, LLVMVoidTypeInContext,
    },
    error::{LLVMConsumeError, LLVMDisposeErrorMessage, LLVMErrorRef, LLVMGetErrorMessage},
    orc2::{
        LLVMOrcCreateNewThreadSafeContextFromLLVMContext, LLVMOrcCreateNewThreadSafeModule,
        LLVMOrcDisposeThreadSafeContext, LLVMOrcJITDylibCreateResourceTracker,
        LLVMOrcReleaseResourceTracker, LLVMOrcResourceTrackerRef, LLVMOrcResourceTrackerRemove,
        lljit::{
            LLVMOrcCreateLLJIT, LLVMOrcDisposeLLJIT, LLVMOrcLLJITAddLLVMIRModuleWithRT,
            LLVMOrcLLJITGetDataLayoutStr, LLVMOrcLLJITGetMainJITDylib, LLVMOrcLLJITGetTripleString,
            LLVMOrcLLJITLookup, LLVMOrcLLJITRef,
        },
    },
    prelude::*,
    target::{LLVM_InitializeNativeAsmPrinter, LLVM_InitializeNativeTarget},
    transforms::pass_builder::{
        LLVMCreatePassBuilderOptions, LLVMDisposePassBuilderOptions, LLVMRunPasses,
    },
};
use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    ptr::null_mut,
    rc::Rc,
    sync::OnceLock,
};

type Result<T> = std::result::Result<T, Error>;

fn check(error: LLVMErrorRef) -> Result<()> {
    if error.is_null() {
        return Ok(());
    }
    // SAFETY: LLVM transfers this error; extracting its message consumes it.
    unsafe {
        let message = LLVMGetErrorMessage(error);
        let text = CStr::from_ptr(message).to_string_lossy().into_owned();
        LLVMDisposeErrorMessage(message);
        Err(Error::Llvm(text))
    }
}

struct Jit(LLVMOrcLLJITRef);
impl Drop for Jit {
    fn drop(&mut self) {
        // SAFETY: compiled resources retain this owner until their trackers are removed.
        unsafe {
            let error = LLVMOrcDisposeLLJIT(self.0);
            if !error.is_null() {
                LLVMConsumeError(error);
            }
        }
    }
}

pub(crate) struct Compiler {
    jit: Rc<Jit>,
    sequence: u64,
}
pub(crate) struct Compiled {
    _jit: Rc<Jit>,
    tracker: LLVMOrcResourceTrackerRef,
    pub(crate) entry: usize,
}
impl Drop for Compiled {
    fn drop(&mut self) {
        // SAFETY: the caller has stopped executing this code and ORC still owns the tracker.
        unsafe {
            let error = LLVMOrcResourceTrackerRemove(self.tracker);
            if !error.is_null() {
                LLVMConsumeError(error);
            }
            LLVMOrcReleaseResourceTracker(self.tracker);
        }
    }
}

impl Compiler {
    pub(crate) fn new() -> Result<Self> {
        static INITIALIZED: OnceLock<bool> = OnceLock::new();
        // SAFETY: native target registration runs once before any JIT is created.
        if !INITIALIZED.get_or_init(|| unsafe {
            LLVM_InitializeNativeTarget() == 0 && LLVM_InitializeNativeAsmPrinter() == 0
        }) {
            return Err(Error::Llvm("native target is unavailable".into()));
        }
        let mut jit = null_mut();
        // SAFETY: LLVM initializes the output handle, using its default native builder.
        check(unsafe { LLVMOrcCreateLLJIT(&raw mut jit, null_mut()) })?;
        Ok(Self {
            jit: Rc::new(Jit(jit)),
            sequence: 0,
        })
    }

    pub(crate) fn compile(&mut self, block: &Block, helpers: [usize; 4]) -> Result<Compiled> {
        let name =
            CString::new(format!("oplab_block_{}", self.sequence)).map_err(|_| Error::Native)?;
        self.sequence = self.sequence.checked_add(1).ok_or(Error::Native)?;
        let mut module = Ir::new(self.jit.0, &name, block, helpers)?;
        for op in block.ops() {
            module.emit(op)?;
        }
        module.finish()?;
        // SAFETY: the module has been verified and owns its context. Transfer both
        // into a ThreadSafeModule before transferring that wrapper to ORC.
        unsafe {
            let dylib = LLVMOrcLLJITGetMainJITDylib(self.jit.0);
            let tracker = LLVMOrcJITDylibCreateResourceTracker(dylib);
            LLVMDisposeBuilder(module.builder);
            module.builder = null_mut();
            let context = LLVMOrcCreateNewThreadSafeContextFromLLVMContext(module.context);
            let owned = LLVMOrcCreateNewThreadSafeModule(module.module, context);
            LLVMOrcDisposeThreadSafeContext(context);
            module.context = null_mut();
            module.module = null_mut();
            let mut compiled = Compiled {
                _jit: Rc::clone(&self.jit),
                tracker,
                entry: 0,
            };
            check(LLVMOrcLLJITAddLLVMIRModuleWithRT(
                self.jit.0, tracker, owned,
            ))?;
            let mut address = 0;
            check(LLVMOrcLLJITLookup(
                self.jit.0,
                &raw mut address,
                name.as_ptr(),
            ))?;
            compiled.entry = usize::try_from(address).map_err(|_| Error::Native)?;
            Ok(compiled)
        }
    }
}

struct Ir<'a> {
    context: LLVMContextRef,
    module: LLVMModuleRef,
    builder: LLVMBuilderRef,
    function: LLVMValueRef,
    env: LLVMValueRef,
    temps: &'a [abi::OplabTemp],
    slots: Vec<LLVMValueRef>,
    labels: HashMap<u64, LLVMBasicBlockRef>,
    helpers: [usize; 4],
    carry: LLVMValueRef,
}
impl Drop for Ir<'_> {
    fn drop(&mut self) {
        // SAFETY: this builder is exclusively owned; null module/context indicate ORC ownership.
        unsafe {
            if !self.builder.is_null() {
                LLVMDisposeBuilder(self.builder);
            }
            if !self.module.is_null() {
                LLVMDisposeModule(self.module);
            }
            if !self.context.is_null() {
                LLVMContextDispose(self.context);
            }
        }
    }
}
impl<'a> Ir<'a> {
    fn new(
        jit: LLVMOrcLLJITRef,
        name: &CStr,
        block: &'a Block,
        helpers: [usize; 4],
    ) -> Result<Self> {
        if block
            .temps()
            .iter()
            .any(|temp| !matches!(temp.bits, 32 | 64 | 128 | 256))
        {
            return Err(Error::Native);
        }
        // SAFETY: every handle belongs to the newly created context. The function
        // accepts only its QEMU CPU-state pointer; guest addresses are never host pointers.
        unsafe {
            let context = LLVMContextCreate();
            let module = LLVMModuleCreateWithNameInContext(name.as_ptr(), context);
            LLVMSetTarget(module, LLVMOrcLLJITGetTripleString(jit));
            LLVMSetDataLayout(module, LLVMOrcLLJITGetDataLayoutStr(jit));
            let builder = LLVMCreateBuilderInContext(context);
            let mut arguments = [LLVMPointerTypeInContext(context, 0)];
            let ty = LLVMFunctionType(LLVMVoidTypeInContext(context), arguments.as_mut_ptr(), 1, 0);
            let function = LLVMAddFunction(module, name.as_ptr(), ty);
            let entry = LLVMAppendBasicBlockInContext(context, function, c"entry".as_ptr());
            LLVMPositionBuilderAtEnd(builder, entry);
            let env = LLVMBuildPtrToInt(
                builder,
                LLVMGetParam(function, 0),
                LLVMInt64TypeInContext(context),
                c"env".as_ptr(),
            );
            let slots = block
                .temps()
                .iter()
                .map(|temp| {
                    if temp.storage == abi::OPLAB_LOCAL {
                        LLVMBuildAlloca(
                            builder,
                            LLVMIntTypeInContext(context, temp.bits),
                            c"".as_ptr(),
                        )
                    } else {
                        null_mut()
                    }
                })
                .collect();
            let carry = LLVMBuildAlloca(builder, LLVMInt1TypeInContext(context), c"carry".as_ptr());
            Ok(Self {
                context,
                module,
                builder,
                function,
                env,
                slots,
                temps: block.temps(),
                labels: HashMap::new(),
                helpers,
                carry,
            })
        }
    }
    fn finish(&mut self) -> Result<()> {
        // SAFETY: all blocks are terminated before verification. Passes run only on verified IR.
        unsafe {
            LLVMBuildRetVoid(self.builder);
            let mut message = null_mut();
            if LLVMVerifyModule(
                self.module,
                LLVMVerifierFailureAction::LLVMReturnStatusAction,
                &raw mut message,
            ) != 0
            {
                let text = CStr::from_ptr(message).to_string_lossy().into_owned();
                LLVMDisposeMessage(message);
                return Err(Error::Llvm(text));
            }
            LLVMDisposeMessage(message);
            let options = LLVMCreatePassBuilderOptions();
            let result = LLVMRunPasses(self.module, c"default<O2>".as_ptr(), null_mut(), options);
            LLVMDisposePassBuilderOptions(options);
            check(result)
        }
    }
    fn int(&self, bits: u32) -> LLVMTypeRef {
        // SAFETY: widths come from validated TCG types or bounded bit-field operations.
        unsafe { LLVMIntTypeInContext(self.context, bits) }
    }
    fn constant(&self, bits: u32, value: u64) -> LLVMValueRef {
        // SAFETY: integer constants are zero-extended to the requested bit width.
        unsafe { LLVMConstInt(self.int(bits), value, 0) }
    }
    fn cast(&self, value: LLVMValueRef, bits: u32, signed: bool) -> LLVMValueRef {
        // SAFETY: values here are integer bit patterns or native pointers, never floating-point scalars.
        unsafe {
            if LLVMGetTypeKind(LLVMTypeOf(value)) == LLVMTypeKind::LLVMPointerTypeKind {
                LLVMBuildPtrToInt(self.builder, value, self.int(bits), c"".as_ptr())
            } else {
                LLVMBuildIntCast2(
                    self.builder,
                    value,
                    self.int(bits),
                    i32::from(signed),
                    c"".as_ptr(),
                )
            }
        }
    }
    fn pointer(&self, base: LLVMValueRef, offset: u64) -> LLVMValueRef {
        // SAFETY: this is a QEMU-owned CPU-state address. Non-inbounds GEP preserves
        // negative offsets into the enclosing CPUState without adding LLVM assumptions.
        unsafe {
            let pointer = LLVMBuildIntToPtr(
                self.builder,
                base,
                LLVMPointerTypeInContext(self.context, 0),
                c"".as_ptr(),
            );
            LLVMBuildGEP2(
                self.builder,
                self.int(8),
                pointer,
                [self.constant(64, offset)].as_mut_ptr(),
                1,
                c"".as_ptr(),
            )
        }
    }
    fn get(&self, index: u64) -> Result<LLVMValueRef> {
        let index = usize::try_from(index).map_err(|_| Error::Native)?;
        let temp = self.temps.get(index).ok_or(Error::Native)?;
        match temp.storage {
            abi::OPLAB_ENV => Ok(self.env),
            abi::OPLAB_CONSTANT => {
                // SAFETY: wide TCG vector constants repeat their 64-bit pattern.
                Ok(if temp.bits > 64 {
                    // SAFETY: the slice has all words for the validated vector width.
                    unsafe {
                        LLVMConstIntOfArbitraryPrecision(
                            self.int(temp.bits),
                            temp.bits / 64,
                            [temp.value; 4].as_ptr(),
                        )
                    }
                } else {
                    self.constant(temp.bits, temp.value)
                })
            }
            abi::OPLAB_GLOBAL => {
                if usize::try_from(temp.base).map_err(|_| Error::Native)? >= index {
                    return Err(Error::Native);
                }
                let base = self.get(u64::from(temp.base))?;
                Ok(self.load(self.pointer(base, temp.offset.cast_unsigned()), temp.bits))
            }
            abi::OPLAB_LOCAL => Ok(self.load(self.slots[index], temp.bits)),
            _ => Err(Error::Native),
        }
    }
    fn put(&self, index: u64, value: LLVMValueRef) -> Result<()> {
        let index = usize::try_from(index).map_err(|_| Error::Native)?;
        let temp = self.temps.get(index).ok_or(Error::Native)?;
        let destination = match temp.storage {
            abi::OPLAB_GLOBAL => {
                self.pointer(self.get(u64::from(temp.base))?, temp.offset.cast_unsigned())
            }
            abi::OPLAB_LOCAL => self.slots[index],
            _ => return Err(Error::Native),
        };
        self.store(destination, self.cast(value, temp.bits, false));
        Ok(())
    }
    fn load(&self, pointer: LLVMValueRef, bits: u32) -> LLVMValueRef {
        // SAFETY: native offsets identify CPU-state storage or a local alloca of this width.
        unsafe {
            let value = LLVMBuildLoad2(self.builder, self.int(bits), pointer, c"".as_ptr());
            LLVMSetAlignment(value, 1);
            value
        }
    }
    fn store(&self, pointer: LLVMValueRef, value: LLVMValueRef) {
        // SAFETY: the destination is a CPU-state field or local alloca; no guest address reaches this path.
        unsafe {
            let store = LLVMBuildStore(self.builder, value, pointer);
            LLVMSetAlignment(store, 1);
        }
    }
    fn label(&mut self, index: u64) -> LLVMBasicBlockRef {
        // SAFETY: labels and branch destinations all belong to this function/context.
        unsafe {
            *self.labels.entry(index).or_insert_with(|| {
                LLVMAppendBasicBlockInContext(self.context, self.function, c"label".as_ptr())
            })
        }
    }
    fn continuation(&self) {
        // SAFETY: a fresh insertion block follows a terminator; dead blocks are removed by LLVM.
        unsafe {
            let next = LLVMAppendBasicBlockInContext(self.context, self.function, c"next".as_ptr());
            LLVMPositionBuilderAtEnd(self.builder, next);
        }
    }
    fn condition(&self, name: &str, a: LLVMValueRef, b: LLVMValueRef) -> Result<LLVMValueRef> {
        use LLVMIntPredicate::{
            LLVMIntEQ, LLVMIntNE, LLVMIntSGE, LLVMIntSGT, LLVMIntSLE, LLVMIntSLT, LLVMIntUGE,
            LLVMIntUGT, LLVMIntULE, LLVMIntULT,
        };
        let predicate = match name {
            "NEVER" | "ALWAYS" => {
                // SAFETY: comparing typed zero constants also preserves vector lane count.
                return Ok(unsafe {
                    let zero = LLVMConstNull(LLVMTypeOf(a));
                    LLVMBuildICmp(
                        self.builder,
                        if name == "ALWAYS" {
                            LLVMIntEQ
                        } else {
                            LLVMIntNE
                        },
                        zero,
                        zero,
                        c"".as_ptr(),
                    )
                });
            }
            "EQ" => LLVMIntEQ,
            "NE" => LLVMIntNE,
            "LT" => LLVMIntSLT,
            "GE" => LLVMIntSGE,
            "LE" => LLVMIntSLE,
            "GT" => LLVMIntSGT,
            "LTU" => LLVMIntULT,
            "GEU" => LLVMIntUGE,
            "LEU" => LLVMIntULE,
            "GTU" => LLVMIntUGT,
            "TSTEQ" | "TSTNE" => {
                // SAFETY: both TCG operands have the same scalar/vector type.
                return Ok(unsafe {
                    let value = LLVMBuildAnd(self.builder, a, b, c"".as_ptr());
                    LLVMBuildICmp(
                        self.builder,
                        if name == "TSTEQ" {
                            LLVMIntEQ
                        } else {
                            LLVMIntNE
                        },
                        value,
                        LLVMConstNull(LLVMTypeOf(value)),
                        c"".as_ptr(),
                    )
                });
            }
            _ => return Err(Error::Operation(name.into())),
        };
        // SAFETY: validated TCG comparison operands have identical types.
        Ok(unsafe { LLVMBuildICmp(self.builder, predicate, a, b, c"".as_ptr()) })
    }
    fn intrinsic(
        &self,
        name: &CStr,
        types: &mut [LLVMTypeRef],
        args: &mut [LLVMValueRef],
    ) -> LLVMValueRef {
        // SAFETY: each caller supplies the exact overload types and operands of an LLVM intrinsic.
        unsafe {
            let id = LLVMLookupIntrinsicID(name.as_ptr(), name.to_bytes().len());
            let function =
                LLVMGetIntrinsicDeclaration(self.module, id, types.as_mut_ptr(), types.len());
            LLVMBuildCall2(
                self.builder,
                LLVMGlobalGetValueType(function),
                function,
                args.as_mut_ptr(),
                LLVMCountParams(function),
                c"".as_ptr(),
            )
        }
    }
    fn emit(&mut self, op: &abi::OplabOp) -> Result<()> {
        // SAFETY: names point into the process-lifetime SDK, not the guest input.
        let name = unsafe { CStr::from_ptr(op.name) }
            .to_str()
            .map_err(|_| Error::Native)?;
        // SAFETY: a non-null condition is also an SDK-owned constant string.
        let condition = if op.condition.is_null() {
            ""
        } else {
            // SAFETY: condition points to an SDK-owned static string.
            unsafe { CStr::from_ptr(op.condition) }
                .to_str()
                .map_err(|_| Error::Native)?
        };
        let outputs = usize::try_from(op.outputs).map_err(|_| Error::Native)?;
        let inputs = usize::try_from(op.inputs).map_err(|_| Error::Native)?;
        let constants = usize::try_from(op.constants).map_err(|_| Error::Native)?;
        let args = op
            .args
            .get(..outputs + inputs + constants)
            .ok_or(Error::Native)?;
        let values = args[outputs..outputs + inputs]
            .iter()
            .map(|&index| self.get(index))
            .collect::<Result<Vec<_>>>()?;
        let immediates = &args[outputs + inputs..];
        self.operation(name, condition, op, &values, immediates)
    }
    fn operation(
        &mut self,
        name: &str,
        condition: &str,
        op: &abi::OplabOp,
        v: &[LLVMValueRef],
        k: &[u64],
    ) -> Result<()> {
        let bits = op.bits;
        let b = self.builder;
        let n = c"".as_ptr();
        // SAFETY: the native exporter supplies TCG's opcode arities and typed operands.
        // Instructions below add no overflow/exact/inbounds promises to guest arithmetic.
        let value = unsafe {
            match name {
                "mov_vec" => v[0],
                "insn_start" | "discard" => return Ok(()),
                "exit_tb" => {
                    LLVMBuildRetVoid(b);
                    self.continuation();
                    return Ok(());
                }
                "set_label" => {
                    let label = self.label(k[0]);
                    LLVMBuildBr(b, label);
                    LLVMPositionBuilderAtEnd(b, label);
                    return Ok(());
                }
                "br" => {
                    let label = self.label(k[0]);
                    LLVMBuildBr(b, label);
                    self.continuation();
                    return Ok(());
                }
                "brcond" => {
                    let target = self.label(k[1]);
                    let next = LLVMAppendBasicBlockInContext(
                        self.context,
                        self.function,
                        c"next".as_ptr(),
                    );
                    LLVMBuildCondBr(b, self.condition(condition, v[0], v[1])?, target, next);
                    LLVMPositionBuilderAtEnd(b, next);
                    return Ok(());
                }
                "mb" => {
                    LLVMBuildFence(
                        b,
                        llvm_sys::LLVMAtomicOrdering::LLVMAtomicOrderingSequentiallyConsistent,
                        0,
                        n,
                    );
                    return Ok(());
                }
                "muls2" | "mulu2" | "mulsh" | "muluh" | "divs2" | "divu2" => {
                    return self.wide(name, op, v);
                }
                "addco" | "addc1o" | "addci" | "addcio" | "subbo" | "subb1o" | "subbi"
                | "subbio" => self.carry_op(name, v, bits)?,
                "ld" | "ld8u" | "ld8s" | "ld16u" | "ld16s" | "ld32u" | "ld32s" | "ld_vec" => {
                    let width = match name {
                        "ld8u" | "ld8s" => 8,
                        "ld16u" | "ld16s" => 16,
                        "ld32u" | "ld32s" => 32,
                        _ => bits,
                    };
                    self.cast(
                        self.load(self.pointer(v[0], k[0]), width),
                        bits,
                        name.ends_with('s'),
                    )
                }
                "st" | "st8" | "st16" | "st32" | "st_vec" => {
                    let width = match name {
                        "st8" => 8,
                        "st16" => 16,
                        "st32" => 32,
                        _ => bits,
                    };
                    self.store(self.pointer(v[1], k[0]), self.cast(v[0], width, false));
                    return Ok(());
                }
                "qemu_ld" | "qemu_st" | "qemu_ld2" | "qemu_st2" => {
                    return self.memory(name, op, v, k[0]);
                }
                "call" => return self.call(op, v),
                _ if name.ends_with("_vec") => self.vector(name, condition, op, v, k)?,
                _ => self.integer(name, condition, op, v, k)?,
            }
        };
        self.put(op.args[0], value)
    }
}
