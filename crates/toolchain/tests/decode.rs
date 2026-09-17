//! Curated architectural encodings, independent of an assembler backend.

use oplab_core::{address::Address, target::Target};
use oplab_toolchain::decode::{DecodeError, decode};
use proptest::prelude::*;

#[test]
fn known_encodings_preserve_bytes_and_addresses() -> Result<(), DecodeError> {
    // Intel SDM: MOV r64, imm64 and RET; Arm ARM: ADD X0, X0, #1 and RET X30.
    let cases: &[(Target, &[u8], &[usize])] = &[
        (
            Target::X86_64,
            &[0x48, 0xb8, 1, 0, 0, 0, 0, 0, 0, 0, 0xc3],
            &[10, 1],
        ),
        (
            Target::Aarch64,
            &[0x00, 0x04, 0x00, 0x91, 0xc0, 0x03, 0x5f, 0xd6],
            &[4, 4],
        ),
    ];
    for &(target, bytes, lengths) in cases {
        let decoded = decode(target, bytes, Address::new(0x1000), 16)?;
        assert_eq!(
            decoded
                .iter()
                .map(|item| item.bytes.len())
                .collect::<Vec<_>>(),
            lengths
        );
        assert_eq!(
            decoded
                .iter()
                .flat_map(|item| item.bytes.clone())
                .collect::<Vec<_>>(),
            bytes
        );
        assert!(decoded.iter().all(|item| !item.text.trim().is_empty()));
        let mut address = 0x1000;
        for instruction in decoded {
            assert_eq!(instruction.address, Address::new(address));
            address += u64::try_from(instruction.bytes.len()).map_err(|_| DecodeError::Range)?;
        }
    }
    Ok(())
}

#[test]
fn incomplete_instructions_are_not_silently_skipped() {
    for (target, bytes, fault) in [
        (Target::X86_64, &[0x48][..], 0x1000),
        (Target::X86_64, &[0x90, 0x48][..], 0x1001),
        (Target::Aarch64, &[0x00, 0x04][..], 0x1000),
        (
            Target::Aarch64,
            &[0x1f, 0x20, 0x03, 0xd5, 0x00, 0x04][..],
            0x1004,
        ),
    ] {
        assert!(matches!(
            decode(target, bytes, Address::new(0x1000), 16),
            Err(DecodeError::Invalid(address)) if address.get() == fault
        ));
    }
    assert!(matches!(
        decode(Target::Aarch64, &[0; 4], Address::new(1), 1),
        Err(DecodeError::Range)
    ));
}

proptest! {
    #[test]
    fn decode_limits_preserve_mixed_instruction_boundaries(
        choices in prop::collection::vec(0_usize..4, 1..64),
        limit in 1_usize..64,
        page in 0_u64..=(u64::MAX >> 12),
    ) {
        // Independent encodings: NOP, MOV immediate, ADD and RET.
        let x86: [&[u8]; 4] = [&[0x90], &[0x48, 0xb8, 1, 0, 0, 0, 0, 0, 0, 0], &[0x48, 0x83, 0xc0, 1], &[0xc3]];
        let arm: [&[u8]; 4] = [&[0x1f, 0x20, 0x03, 0xd5], &[0x20, 0, 0x80, 0xd2], &[0, 4, 0, 0x91], &[0xc0, 3, 0x5f, 0xd6]];
        for (target, encodings) in [(Target::X86_64, x86), (Target::Aarch64, arm)] {
            let bytes: Vec<u8> = choices.iter().flat_map(|&index| encodings[index].iter().copied()).collect();
            let mut address = page << 12;
            let decoded = decode(target, &bytes, Address::new(address), limit)?;
            prop_assert_eq!(decoded.len(), choices.len().min(limit));
            for (instruction, &index) in decoded.iter().zip(&choices) {
                prop_assert_eq!(instruction.bytes.as_slice(), encodings[index]);
                prop_assert_eq!(instruction.address.get(), address);
                address += u64::try_from(encodings[index].len())?;
            }
        }
    }
}

#[test]
fn decode_bounds_reject_invalid_requests_and_accept_exact_limits() -> Result<(), DecodeError> {
    for (target, encoding) in [
        (Target::X86_64, &[0x90][..]),
        (Target::Aarch64, &[0x1f, 0x20, 0x03, 0xd5][..]),
    ] {
        let bytes = encoding.repeat(65536 / encoding.len());
        let base = Address::new(0x1000);
        assert_eq!(decode(target, &bytes, base, 4096)?.len(), 4096);
        let mut oversized = bytes;
        oversized.push(0);
        for (bytes, base, limit) in [
            (&[][..], base, 1),
            (oversized.as_slice(), base, 1),
            (encoding, base, 0),
            (encoding, base, 4097),
            (&oversized[..8], Address::new(u64::MAX - 3), 1),
        ] {
            assert!(matches!(
                decode(target, bytes, base, limit),
                Err(DecodeError::Range)
            ));
        }
    }
    Ok(())
}
