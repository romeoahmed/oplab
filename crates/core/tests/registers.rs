//! Architectural bit preservation and rejection before native mutation.

use oplab_core::{
    registers::{RegisterEdit, RegisterStorage},
    target::Target,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn subregister_edits_preserve_only_architecturally_unaffected_bits(
        previous in any::<u64>(), byte in any::<u8>(), word in any::<u16>(), dword in any::<u32>(),
    ) {
        for (name, value, offset, length) in [
            ("al", u64::from(byte), 0, 1),
            ("ah", u64::from(byte), 1, 1),
            ("ax", u64::from(word), 0, 2),
            ("eax", u64::from(dword), 0, 4),
        ] {
            let mut expected = previous.to_le_bytes();
            expected[offset..offset + length].copy_from_slice(&value.to_le_bytes()[..length]);
            if length == 4 {
                expected[4..].fill(0);
            }
            let edit = RegisterEdit::new(Target::X86_64, name, value)?;
            prop_assert_eq!(edit.storage(), RegisterStorage::Gpr(0));
            prop_assert_eq!(edit.apply(previous), u64::from_le_bytes(expected));
        }
        for name in ["w0", "w30", "wsp"] {
            prop_assert_eq!(RegisterEdit::new(Target::Aarch64, name, u64::from(dword))?.apply(previous), u64::from(dword));
        }
    }

    #[test]
    fn flag_edits_preserve_every_other_bit(previous in any::<u64>(), set in any::<bool>()) {
        for (target, flags) in [
            (Target::X86_64, &[("cf", 0), ("pf", 2), ("af", 4), ("zf", 6), ("sf", 7), ("df", 10), ("of", 11)][..]),
            (Target::Aarch64, &[("n", 31), ("z", 30), ("c", 29), ("v", 28)][..]),
        ] {
            for &(name, bit) in flags {
                let mask = 1_u64 << bit;
                let value = RegisterEdit::new(target, name, u64::from(set))?.apply(previous);
                prop_assert_eq!(value & !mask, previous & !mask);
                prop_assert_eq!(value & mask != 0, set);
            }
        }
    }

    #[test]
    fn pc_edits_replace_the_address_and_enforce_only_guest_alignment(
        word in 0_u64..=u64::MAX / 4, previous in any::<u64>(),
    ) {
        for offset in 0..4 {
            let address = word * 4 + offset;
            let x86 = RegisterEdit::new(Target::X86_64, "rip", address)?;
            prop_assert_eq!(x86.storage(), RegisterStorage::InstructionPointer);
            prop_assert_eq!(x86.apply(previous), address);
            let arm = RegisterEdit::new(Target::Aarch64, "pc", address);
            if offset == 0 {
                let arm = arm?;
                prop_assert_eq!(arm.storage(), RegisterStorage::InstructionPointer);
                prop_assert_eq!(arm.apply(previous), address);
            } else {
                prop_assert!(arm.is_err());
            }
        }
    }
}

#[test]
fn unsupported_names_are_rejected() {
    for (target, names) in [
        (
            Target::X86_64,
            &[
                "RAX", "rflags", "eflags", "ripx", "eip", "if", "tf", "x0", "r16d",
            ][..],
        ),
        (
            Target::Aarch64,
            &[
                "x31", "w31", "w00", "w01", "xzr", "wzr", "pstate", "nzcv", "rax",
            ][..],
        ),
    ] {
        for &name in names {
            assert!(RegisterEdit::new(target, name, 0).is_err(), "{name}");
        }
    }
}

#[test]
fn write_widths_accept_their_limits_without_truncating_overflow() {
    for (target, name, maximum) in [
        (Target::X86_64, "al", u64::from(u8::MAX)),
        (Target::X86_64, "ah", u64::from(u8::MAX)),
        (Target::X86_64, "sp", u64::from(u16::MAX)),
        (Target::X86_64, "r15d", u64::from(u32::MAX)),
        (Target::X86_64, "zf", 1),
        (Target::Aarch64, "wsp", u64::from(u32::MAX)),
        (Target::Aarch64, "n", 1),
    ] {
        for value in [0, maximum] {
            assert!(
                RegisterEdit::new(target, name, value).is_ok(),
                "{name}={value}"
            );
        }
        for value in [maximum + 1, u64::MAX] {
            assert!(
                RegisterEdit::new(target, name, value).is_err(),
                "{name}={value}"
            );
        }
    }
}

proptest! {
    #[test]
    fn overlapping_vector_edits_preserve_unselected_and_inactive_bytes(
        previous in any::<[u8; 256]>(), vq in 1_usize..=16,
        edits in prop::collection::vec((0_usize..6, any::<u16>(), any::<[u8; 256]>()), 1..16),
    ) {
        use oplab_core::registers::{VectorBits, VectorEdit};
        for (target, size, name, active) in [
            (Target::X86_64, 32, "ymm15", 32),
            (Target::X86_64, 32, "xmm15", 16),
            (Target::Aarch64, vq * 16, "z31", vq * 16),
            (Target::Aarch64, vq * 16, "v31", 16),
        ] {
            let mut actual = previous;
            let mut expected = previous;
            for &(kind, position, value) in &edits {
                let width = [8, 16, 32, 64, 128, u16::try_from(active * 8)?][kind];
                let count = u16::try_from(active * 8)? / width;
                let lane = position % count;
                let length = usize::from(width / 8);
                let bits = VectorBits::new(value[..length].to_vec())?;
                VectorEdit::new(target, size, name, width, lane, bits.clone())?.apply(&mut actual)?;
                let start = usize::from(lane) * length;
                expected[start..start + length].copy_from_slice(&value[..length]);
                prop_assert_eq!(actual, expected);
                prop_assert!(VectorEdit::new(target, size, name, width, count, bits).is_err());
            }
        }
    }

    #[test]
    fn predicate_edits_preserve_adjacent_bits_and_inactive_storage(
        previous in any::<[u8; 32]>(), set in any::<bool>(),
        vq in 1_usize..=16, position in any::<u16>(),
    ) {
        use oplab_core::registers::{VectorBits, VectorEdit};
        let lane = position % u16::try_from(vq * 16)?;
        for name in ["p0", "p15", "ffr"] {
            let edit = VectorEdit::new(Target::Aarch64, vq * 16, name, 1, lane, VectorBits::new(vec![u8::from(set)])?)?;
            let mut actual = previous;
            edit.apply(&mut actual)?;
            for bit in 0..256 {
                let observed = actual[bit / 8] >> (bit % 8) & 1;
                let expected = if bit == usize::from(lane) { u8::from(set) } else { previous[bit / 8] >> (bit % 8) & 1 };
                prop_assert_eq!(observed, expected);
            }
        }
    }
}

#[test]
fn vector_edits_reject_ambiguous_names_invalid_lengths_and_mismatched_values()
-> Result<(), Box<dyn std::error::Error>> {
    use oplab_core::registers::{VectorBits, VectorEdit};
    let zero = VectorBits::new(vec![0; 16])?;
    for (target, size, names) in [
        (
            Target::X86_64,
            32,
            &["ymm16", "xmm16", "xmm00", "xmm+1", "XMM0", "z0", "rax"][..],
        ),
        (
            Target::Aarch64,
            256,
            &["z32", "p16", "v00", "v+1", "V0", "q0", "s0", "v0.4s"][..],
        ),
    ] {
        for name in names {
            assert!(VectorEdit::new(target, size, name, 128, 0, zero.clone()).is_err());
        }
    }
    for size in [0, 15, 17, 255, 272] {
        assert!(VectorEdit::new(Target::Aarch64, size, "z0", 128, 0, zero.clone()).is_err());
    }
    for (name, width, lane, value) in [
        ("z0", 0, 0, vec![0]),
        ("z0", 24, 0, vec![0; 3]),
        ("z0", 64, 0, vec![0; 16]),
        ("p0", 1, 0, vec![2]),
        ("ffr", 1, 256, vec![1]),
    ] {
        assert!(
            VectorEdit::new(
                Target::Aarch64,
                256,
                name,
                width,
                lane,
                VectorBits::new(value)?
            )
            .is_err()
        );
    }
    let edit = VectorEdit::new(Target::Aarch64, 256, "z0", 128, 1, zero)?;
    let mut short = [7; 16];
    assert!(edit.apply(&mut short).is_err());
    assert_eq!(short, [7; 16]);
    Ok(())
}

proptest! {
    #[test]
    fn rounding_edits_select_architectural_modes_and_preserve_other_bits(previous in any::<u32>()) {
        use oplab_core::registers::RoundingMode;
        // MXCSR.RC and FPCR.RMode encode downward/upward rounding in opposite orders.
        for (mode, x86, arm) in [
            (RoundingMode::NearestEven, 0, 0),
            (RoundingMode::Down, 0x2000, 0x0080_0000),
            (RoundingMode::Up, 0x4000, 0x0040_0000),
            (RoundingMode::TowardZero, 0x6000, 0x00c0_0000),
        ] {
            for (target, mask, bits) in [(Target::X86_64, 0x6000, x86), (Target::Aarch64, 0x00c0_0000, arm)] {
                let edited = mode.apply(target, previous);
                prop_assert_eq!(edited & mask, bits);
                prop_assert_eq!(edited & !mask, previous & !mask);
            }
        }
    }
}
