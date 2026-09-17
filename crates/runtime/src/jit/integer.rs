//! Scalar and wide-integer TCG lowering.
use super::{Error, Ir, Result, abi};
use llvm_sys::{
    core::{
        LLVMBuildAShr, LLVMBuildAdd, LLVMBuildAnd, LLVMBuildLShr, LLVMBuildMul, LLVMBuildNeg,
        LLVMBuildNot, LLVMBuildOr, LLVMBuildSDiv, LLVMBuildSRem, LLVMBuildSelect, LLVMBuildShl,
        LLVMBuildSub, LLVMBuildUDiv, LLVMBuildURem, LLVMBuildXor,
    },
    prelude::LLVMValueRef,
};
impl Ir<'_> {
    pub(super) fn carry_op(
        &self,
        name: &str,
        v: &[LLVMValueRef],
        bits: u32,
    ) -> Result<LLVMValueRef> {
        // SAFETY: widening makes the carry/borrow explicit without host flag assumptions.
        unsafe {
            let b = self.builder;
            let n = c"".as_ptr();
            let wide = bits * 2;
            let carry = if name.contains("1o") {
                self.constant(wide, 1)
            } else if name.ends_with("ci")
                || name.ends_with("cio")
                || name.ends_with("bi")
                || name.ends_with("bio")
            {
                self.cast(self.load(self.carry, 1), wide, false)
            } else {
                self.constant(wide, 0)
            };
            let a = self.cast(v[0], wide, false);
            let rhs = LLVMBuildAdd(b, self.cast(v[1], wide, false), carry, n);
            let result = if name.starts_with("add") {
                LLVMBuildAdd(b, a, rhs, n)
            } else {
                LLVMBuildSub(b, a, rhs, n)
            };
            if name.ends_with('o') {
                let next = if name.starts_with("add") {
                    self.cast(
                        LLVMBuildLShr(b, result, self.constant(wide, u64::from(bits)), n),
                        1,
                        false,
                    )
                } else {
                    self.condition("LTU", a, rhs)?
                };
                self.store(self.carry, next);
            }
            Ok(self.cast(result, bits, false))
        }
    }
    pub(super) fn integer(
        &self,
        name: &str,
        condition: &str,
        op: &abi::OplabOp,
        v: &[LLVMValueRef],
        k: &[u64],
    ) -> Result<LLVMValueRef> {
        let bits = op.bits;
        let b = self.builder;
        let n = c"".as_ptr();
        // SAFETY: typed TCG operands have matching widths; no overflow flags are added.
        Ok(unsafe {
            match name {
                "mov" => v[0],
                "add" => LLVMBuildAdd(b, v[0], v[1], n),
                "sub" => LLVMBuildSub(b, v[0], v[1], n),
                "mul" => LLVMBuildMul(b, v[0], v[1], n),
                "and" => LLVMBuildAnd(b, v[0], v[1], n),
                "or" => LLVMBuildOr(b, v[0], v[1], n),
                "xor" => LLVMBuildXor(b, v[0], v[1], n),
                "andc" => LLVMBuildAnd(b, v[0], LLVMBuildNot(b, v[1], n), n),
                "orc" => LLVMBuildOr(b, v[0], LLVMBuildNot(b, v[1], n), n),
                "nand" => LLVMBuildNot(b, LLVMBuildAnd(b, v[0], v[1], n), n),
                "nor" => LLVMBuildNot(b, LLVMBuildOr(b, v[0], v[1], n), n),
                "eqv" => LLVMBuildNot(b, LLVMBuildXor(b, v[0], v[1], n), n),
                "not" => LLVMBuildNot(b, v[0], n),
                "neg" => LLVMBuildNeg(b, v[0], n),
                "divs" => LLVMBuildSDiv(b, v[0], v[1], n),
                "divu" => LLVMBuildUDiv(b, v[0], v[1], n),
                "rems" => LLVMBuildSRem(b, v[0], v[1], n),
                "remu" => LLVMBuildURem(b, v[0], v[1], n),
                "shl" | "shr" | "sar" => {
                    // TCG permits unspecified out-of-range results, but never LLVM poison.
                    let count = LLVMBuildAnd(b, v[1], self.constant(bits, u64::from(bits - 1)), n);
                    match name {
                        "shl" => LLVMBuildShl(b, v[0], count, n),
                        "shr" => LLVMBuildLShr(b, v[0], count, n),
                        _ => LLVMBuildAShr(b, v[0], count, n),
                    }
                }
                "rotl" | "rotr" => self.intrinsic(
                    if name == "rotl" {
                        c"llvm.fshl"
                    } else {
                        c"llvm.fshr"
                    },
                    &mut [self.int(bits)],
                    &mut [v[0], v[0], v[1]],
                ),
                "clz" | "ctz" => {
                    let value = self.intrinsic(
                        if name == "clz" {
                            c"llvm.ctlz"
                        } else {
                            c"llvm.cttz"
                        },
                        &mut [self.int(bits)],
                        &mut [v[0], self.constant(1, 0)],
                    );
                    LLVMBuildSelect(
                        b,
                        self.condition("EQ", v[0], self.constant(bits, 0))?,
                        v[1],
                        value,
                        n,
                    )
                }
                "ctpop" => self.intrinsic(c"llvm.ctpop", &mut [self.int(bits)], &mut [v[0]]),
                "setcond" => self.cast(self.condition(condition, v[0], v[1])?, bits, false),
                "negsetcond" => self.cast(self.condition(condition, v[0], v[1])?, bits, true),
                "movcond" => {
                    LLVMBuildSelect(b, self.condition(condition, v[0], v[1])?, v[2], v[3], n)
                }
                _ => return self.bitfield(name, op, v, k),
            }
        })
    }
    pub(super) fn bitfield(
        &self,
        name: &str,
        op: &abi::OplabOp,
        v: &[LLVMValueRef],
        k: &[u64],
    ) -> Result<LLVMValueRef> {
        let bits = op.bits;
        let b = self.builder;
        let n = c"".as_ptr();
        // SAFETY: typed TCG operands have matching widths; no overflow flags are added.
        Ok(unsafe {
            match name {
                "extract" | "sextract" => {
                    let width = u32::try_from(k[1]).map_err(|_| Error::Native)?;
                    let shifted = LLVMBuildLShr(b, v[0], self.constant(bits, k[0]), n);
                    self.cast(self.cast(shifted, width, false), bits, name == "sextract")
                }
                "extract2" => self.intrinsic(
                    c"llvm.fshr",
                    &mut [self.int(bits)],
                    &mut [v[1], v[0], self.constant(bits, k[0])],
                ),
                "deposit" => {
                    let width = u32::try_from(k[1]).map_err(|_| Error::Native)?;
                    let mask = u64::MAX.checked_shr(64 - width).unwrap_or(0) << k[0];
                    LLVMBuildOr(
                        b,
                        LLVMBuildAnd(b, v[0], self.constant(bits, !mask), n),
                        LLVMBuildAnd(
                            b,
                            LLVMBuildShl(b, v[1], self.constant(bits, k[0]), n),
                            self.constant(bits, mask),
                            n,
                        ),
                        n,
                    )
                }
                "bswap16" | "bswap32" | "bswap64" => {
                    let width = match name {
                        "bswap16" => 16,
                        "bswap32" => 32,
                        _ => 64,
                    };
                    let value = self.intrinsic(
                        c"llvm.bswap",
                        &mut [self.int(width)],
                        &mut [self.cast(v[0], width, false)],
                    );
                    self.cast(value, bits, k[0] != 0)
                }
                "ext_i32_i64" => self.cast(v[0], 64, true),
                "extu_i32_i64" => self.cast(v[0], 64, false),
                "extrl_i64_i32" => self.cast(v[0], 32, false),
                "extrh_i64_i32" => {
                    self.cast(LLVMBuildLShr(b, v[0], self.constant(64, 32), n), 32, false)
                }
                _ => return Err(Error::Operation(name.into())),
            }
        })
    }
    pub(super) fn wide(&self, name: &str, op: &abi::OplabOp, v: &[LLVMValueRef]) -> Result<()> {
        let bits = op.bits;
        let b = self.builder;
        let n = c"".as_ptr();
        // SAFETY: typed TCG operands have matching widths; no overflow flags are added.
        unsafe {
            match name {
                "muls2" | "mulu2" | "mulsh" | "muluh" => {
                    let signed = matches!(name, "muls2" | "mulsh");
                    let product = LLVMBuildMul(
                        b,
                        self.cast(v[0], bits * 2, signed),
                        self.cast(v[1], bits * 2, signed),
                        n,
                    );
                    let high =
                        LLVMBuildLShr(b, product, self.constant(bits * 2, u64::from(bits)), n);
                    if op.outputs == 2 {
                        self.put(op.args[0], product)?;
                        self.put(op.args[1], high)?;
                        return Ok(());
                    }
                    self.put(op.args[0], self.cast(high, bits, false))?;
                }
                "divs2" | "divu2" => {
                    let signed = name == "divs2";
                    let wide = bits * 2;
                    let numerator = LLVMBuildOr(
                        b,
                        self.cast(v[0], wide, false),
                        LLVMBuildShl(
                            b,
                            self.cast(v[1], wide, false),
                            self.constant(wide, u64::from(bits)),
                            n,
                        ),
                        n,
                    );
                    let divisor = self.cast(v[2], wide, signed);
                    let quotient = if signed {
                        LLVMBuildSDiv(b, numerator, divisor, n)
                    } else {
                        LLVMBuildUDiv(b, numerator, divisor, n)
                    };
                    let remainder = if signed {
                        LLVMBuildSRem(b, numerator, divisor, n)
                    } else {
                        LLVMBuildURem(b, numerator, divisor, n)
                    };
                    self.put(op.args[0], quotient)?;
                    self.put(op.args[1], remainder)?;
                    return Ok(());
                }
                _ => return Err(Error::Operation(name.into())),
            }
        }
        Ok(())
    }
}
