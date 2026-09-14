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
        assert_eq!(decoded.last().map(|item| item.text.as_str()), Some("ret"));
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
    for (target, bytes) in [
        (Target::X86_64, &[0x48][..]),
        (Target::Aarch64, &[0x00, 0x04][..]),
    ] {
        assert!(matches!(
            decode(target, bytes, Address::new(0x1000), 16),
            Err(DecodeError::Invalid(_))
        ));
    }
    assert!(matches!(
        decode(Target::Aarch64, &[0; 4], Address::new(1), 1),
        Err(DecodeError::Range)
    ));
}
