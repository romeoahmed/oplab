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
