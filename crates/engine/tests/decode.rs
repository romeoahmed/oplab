//! Curated architectural encodings, independent of an assembler backend.

use oplab_core::{address::Address, target::Target};
use oplab_engine::decode::{DecodeError, decode};

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

proptest::proptest! {
    #[test]
    fn decode_limits_preserve_complete_instruction_prefixes(count in 1_usize..64, limit in 1_usize..64) {
        for (target, encoding) in [(Target::X86_64, &[0x90][..]), (Target::Aarch64, &[0x1f, 0x20, 0x03, 0xd5][..])] {
            let bytes = encoding.repeat(count);
            let decoded = decode(target, &bytes, Address::new(0x1000), limit)?;
            proptest::prop_assert_eq!(decoded.len(), count.min(limit));
            for (index, instruction) in decoded.iter().enumerate() {
                proptest::prop_assert_eq!(instruction.bytes.as_slice(), encoding);
                proptest::prop_assert_eq!(instruction.address.get(), 0x1000 + u64::try_from(index * encoding.len())?);
            }
        }
    }
}
