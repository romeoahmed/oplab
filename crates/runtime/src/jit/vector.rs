//! Vector operations preserve TCG lane semantics.
use super::{Error, Ir, Result, abi};
use llvm_sys::{
    core::{
        LLVMBuildAShr, LLVMBuildAdd, LLVMBuildAnd, LLVMBuildBitCast, LLVMBuildInsertElement,
        LLVMBuildLShr, LLVMBuildMul, LLVMBuildNeg, LLVMBuildNot, LLVMBuildOr, LLVMBuildSExt,
        LLVMBuildSelect, LLVMBuildShl, LLVMBuildShuffleVector, LLVMBuildSub, LLVMBuildTrunc,
        LLVMBuildXor, LLVMConstNull, LLVMVectorType,
    },
    prelude::LLVMValueRef,
};
impl Ir<'_> {
    pub(super) fn splat(&self, value: LLVMValueRef, bits: u32, lane: u32) -> LLVMValueRef {
        // SAFETY: lane divides the validated vector width; the shuffle broadcasts initialized lane zero.
        unsafe {
            let ty = LLVMVectorType(self.int(lane), bits / lane);
            let vector = LLVMBuildInsertElement(
                self.builder,
                LLVMConstNull(ty),
                self.cast(value, lane, false),
                self.constant(32, 0),
                c"".as_ptr(),
            );
            LLVMBuildShuffleVector(
                self.builder,
                vector,
                LLVMConstNull(ty),
                LLVMConstNull(LLVMVectorType(self.int(32), bits / lane)),
                c"".as_ptr(),
            )
        }
    }
    pub(super) fn vector(
        &self,
        name: &str,
        condition: &str,
        op: &abi::OplabOp,
        values: &[LLVMValueRef],
        k: &[u64],
    ) -> Result<LLVMValueRef> {
        let bits = op.bits;
        let lane = op.lane;
        if !matches!(lane, 8 | 16 | 32 | 64) || !bits.is_multiple_of(lane) {
            return Err(Error::Native);
        }
        // SAFETY: vector operations preserve the bit width and operate on TCG's declared integer lanes.
        unsafe {
            let b = self.builder;
            let n = c"".as_ptr();
            let ty = LLVMVectorType(self.int(lane), bits / lane);
            let cast = |value| LLVMBuildBitCast(b, value, ty, n);
            let a = if name == "dup_vec" {
                self.splat(values[0], bits, lane)
            } else if name == "dupm_vec" {
                self.splat(self.load(self.pointer(values[0], k[0]), lane), bits, lane)
            } else {
                cast(values[0])
            };
            let rhs = if values.len() > 1
                && !matches!(name, "shls_vec" | "shrs_vec" | "sars_vec" | "rotls_vec")
            {
                cast(values[1])
            } else {
                LLVMConstNull(ty)
            };
            let value = match name {
                "dup_vec" | "dupm_vec" => a,
                "add_vec" => LLVMBuildAdd(b, a, rhs, n),
                "sub_vec" => LLVMBuildSub(b, a, rhs, n),
                "mul_vec" => LLVMBuildMul(b, a, rhs, n),
                "neg_vec" => LLVMBuildNeg(b, a, n),
                "not_vec" => LLVMBuildNot(b, a, n),
                "and_vec" => LLVMBuildAnd(b, a, rhs, n),
                "or_vec" => LLVMBuildOr(b, a, rhs, n),
                "xor_vec" => LLVMBuildXor(b, a, rhs, n),
                "andc_vec" => LLVMBuildAnd(b, a, LLVMBuildNot(b, rhs, n), n),
                "orc_vec" => LLVMBuildOr(b, a, LLVMBuildNot(b, rhs, n), n),
                "nand_vec" => LLVMBuildNot(b, LLVMBuildAnd(b, a, rhs, n), n),
                "nor_vec" => LLVMBuildNot(b, LLVMBuildOr(b, a, rhs, n), n),
                "eqv_vec" => LLVMBuildNot(b, LLVMBuildXor(b, a, rhs, n), n),
                "abs_vec" => self.intrinsic(c"llvm.abs", &mut [ty], &mut [a, self.constant(1, 0)]),
                "ssadd_vec" | "usadd_vec" | "sssub_vec" | "ussub_vec" | "smin_vec" | "umin_vec"
                | "smax_vec" | "umax_vec" => {
                    let intrinsic = match name {
                        "ssadd_vec" => c"llvm.sadd.sat",
                        "usadd_vec" => c"llvm.uadd.sat",
                        "sssub_vec" => c"llvm.ssub.sat",
                        "ussub_vec" => c"llvm.usub.sat",
                        "smin_vec" => c"llvm.smin",
                        "umin_vec" => c"llvm.umin",
                        "smax_vec" => c"llvm.smax",
                        _ => c"llvm.umax",
                    };
                    self.intrinsic(intrinsic, &mut [ty], &mut [a, rhs])
                }
                "cmp_vec" => LLVMBuildSExt(b, self.condition(condition, a, rhs)?, ty, n),
                "cmpsel_vec" => LLVMBuildSelect(
                    b,
                    self.condition(condition, a, rhs)?,
                    cast(values[2]),
                    cast(values[3]),
                    n,
                ),
                "bitsel_vec" => LLVMBuildOr(
                    b,
                    LLVMBuildAnd(b, a, rhs, n),
                    LLVMBuildAnd(b, LLVMBuildNot(b, a, n), cast(values[2]), n),
                    n,
                ),
                _ => return self.vector_shift(name, op, values, k),
            };
            Ok(LLVMBuildBitCast(b, value, self.int(bits), n))
        }
    }
    pub(super) fn vector_shift(
        &self,
        name: &str,
        op: &abi::OplabOp,
        values: &[LLVMValueRef],
        k: &[u64],
    ) -> Result<LLVMValueRef> {
        let bits = op.bits;
        let lane = op.lane;
        // SAFETY: shifts use declared lane widths and clamp counts before LLVM shifts.
        unsafe {
            let b = self.builder;
            let n = c"".as_ptr();
            let ty = LLVMVectorType(self.int(lane), bits / lane);
            let splat = |value| self.splat(self.constant(lane, value), bits, lane);
            let a = LLVMBuildBitCast(b, values[0], ty, n);
            let rhs = if values.len() > 1 {
                if name.ends_with("s_vec") {
                    self.splat(values[1], bits, lane)
                } else {
                    LLVMBuildBitCast(b, values[1], ty, n)
                }
            } else {
                LLVMConstNull(ty)
            };
            let result = match name {
                "shli_vec" | "shri_vec" | "sari_vec" | "rotli_vec" | "shls_vec" | "shrs_vec"
                | "sars_vec" | "rotls_vec" | "shlv_vec" | "shrv_vec" | "sarv_vec" | "rotlv_vec"
                | "rotrv_vec" => {
                    let count = if name.ends_with("i_vec") {
                        splat(k[0])
                    } else {
                        rhs
                    };
                    let count = LLVMBuildAnd(b, count, splat(u64::from(lane - 1)), n);
                    if name.starts_with("shl") {
                        LLVMBuildShl(b, a, count, n)
                    } else if name.starts_with("shr") {
                        LLVMBuildLShr(b, a, count, n)
                    } else if name.starts_with("sar") {
                        LLVMBuildAShr(b, a, count, n)
                    } else {
                        self.intrinsic(
                            if name.starts_with("rotl") {
                                c"llvm.fshl"
                            } else {
                                c"llvm.fshr"
                            },
                            &mut [ty],
                            &mut [a, a, count],
                        )
                    }
                }
                "aa64_sli_vec" => {
                    let mask = splat((1u64 << k[0]) - 1);
                    LLVMBuildOr(
                        b,
                        LLVMBuildAnd(b, a, mask, n),
                        LLVMBuildShl(b, rhs, splat(k[0]), n),
                        n,
                    )
                }
                "aa64_sshl_vec" | "aa64_ushl_vec" => {
                    // SSHL interprets the low byte as a signed shift count. Oversized
                    // right shifts sign-fill; oversized left shifts produce zero.
                    let byte_ty = LLVMVectorType(self.int(8), bits / lane);
                    let count = if lane == 8 {
                        rhs
                    } else {
                        LLVMBuildSExt(b, LLVMBuildTrunc(b, rhs, byte_ty, n), ty, n)
                    };
                    let negative = self.condition("LT", count, splat(0))?;
                    let left_count = LLVMBuildAnd(b, count, splat(u64::from(lane - 1)), n);
                    let right_count = LLVMBuildNeg(b, count, n);
                    let right_count = self.intrinsic(
                        c"llvm.umin",
                        &mut [ty],
                        &mut [right_count, splat(u64::from(lane - 1))],
                    );
                    let left = LLVMBuildSelect(
                        b,
                        self.condition("LTU", count, splat(u64::from(lane)))?,
                        LLVMBuildShl(b, a, left_count, n),
                        splat(0),
                        n,
                    );
                    let right = if name == "aa64_sshl_vec" {
                        LLVMBuildAShr(b, a, right_count, n)
                    } else {
                        let magnitude = LLVMBuildNeg(b, count, n);
                        LLVMBuildSelect(
                            b,
                            self.condition("LTU", magnitude, splat(u64::from(lane)))?,
                            LLVMBuildLShr(b, a, right_count, n),
                            splat(0),
                            n,
                        )
                    };
                    LLVMBuildSelect(b, negative, right, left, n)
                }
                _ => return Err(Error::Operation(name.into())),
            };
            Ok(LLVMBuildBitCast(b, result, self.int(bits), n))
        }
    }
}
