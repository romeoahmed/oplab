//! Architectural effects from fixed encodings and independently generated operands.

use object::{Object, ObjectSection};
use oplab_core::{
    address::Address,
    protocol::analysis::{
        ArchitectureAnalysis, DataAccess, FlowControl, InstructionAnalysis, MemoryAccess,
        RegisterAccess,
    },
    target::Target,
};
use oplab_toolchain::decode::{DecodeError, analyze};
use proptest::prelude::*;

fn register(name: &str, access: DataAccess) -> RegisterAccess {
    RegisterAccess {
        name: name.into(),
        access,
    }
}

fn x86(bytes: &[u8]) -> Result<InstructionAnalysis, DecodeError> {
    analyze(Target::X86_64, bytes, Address::new(0x1000))
}

#[test]
fn implicit_stack_conditional_access_and_undefined_flags_are_distinct() -> Result<(), DecodeError> {
    let push = x86(&[0x50])?; // PUSH RAX
    assert!(push.registers.contains(&register("rax", DataAccess::Read)));
    assert!(
        push.registers
            .contains(&register("rsp", DataAccess::ReadWrite))
    );
    assert_eq!(
        push.memory,
        [MemoryAccess {
            access: DataAccess::Write,
            bytes: Some(8)
        }]
    );

    let cmov = x86(&[0x48, 0x0f, 0x44, 0xc3])?; // CMOVE RAX,RBX
    assert!(
        cmov.registers
            .contains(&register("rax", DataAccess::ConditionalWrite))
    );
    let ArchitectureAnalysis::X86 { flags, .. } = cmov.architecture else {
        panic!("wrong target");
    };
    assert_eq!(flags.read, ["ZF"]);

    // The same register is a conditional destination, base and index. Repeated
    // address reads must not turn CMOVE's conditional write into an unconditional one.
    let aliased = x86(&[0x48, 0x0f, 0x44, 0x04, 0x00])?; // CMOVE RAX,[RAX+RAX]
    assert!(
        aliased
            .registers
            .contains(&register("rax", DataAccess::ReadConditionalWrite))
    );

    let xor = x86(&[0x31, 0xd8])?; // XOR EAX,EBX reads operands and writes arithmetic flags.
    assert!(
        xor.registers
            .contains(&register("eax", DataAccess::ReadWrite))
    );
    assert!(xor.registers.contains(&register("ebx", DataAccess::Read)));
    let ArchitectureAnalysis::X86 { flags, .. } = xor.architecture else {
        panic!("wrong target");
    };
    assert_eq!(flags.undefined, ["AF"]);
    assert!(flags.cleared.contains(&"CF".into()));
    assert!(flags.written.contains(&"ZF".into()));

    let lea = x86(&[0x48, 0x8d, 0x03])?;
    assert!(lea.memory.is_empty()); // LEA computes an address; it does not read memory.
    assert!(lea.registers.contains(&register("rbx", DataAccess::Read)));
    for bytes in [&[0x0f, 0x18, 0x00][..], &[0x0f, 0x01, 0x38][..]] {
        let facts = x86(bytes)?; // PREFETCHNTA [RAX] and INVLPG [RAX] use an address.
        assert!(facts.memory.is_empty());
        assert!(facts.registers.contains(&register("rax", DataAccess::Read)));
    }
    Ok(())
}

#[test]
fn arm_detail_preserves_writeback_flags_and_implicit_link_register() -> Result<(), DecodeError> {
    let store = analyze(
        Target::Aarch64,
        &[0x20, 0x8c, 0x00, 0xf8],
        Address::new(0x1000),
    )?; // STR X0,[X1,#8]!
    assert!(store.registers.contains(&register("x0", DataAccess::Read)));
    assert!(
        store
            .registers
            .contains(&register("x1", DataAccess::ReadWrite))
    );
    assert_eq!(store.memory.len(), 1);
    let memory = &store.memory[0];
    // STR writes memory; reporting an unknown effect is preferable to inventing a read.
    assert!(matches!(
        memory.access,
        DataAccess::Unknown | DataAccess::Write
    ));
    assert!(memory.bytes.is_none_or(|bytes| bytes == 8));
    assert!(matches!(
        store.architecture,
        ArchitectureAnalysis::Aarch64 {
            writeback: true,
            ..
        }
    ));
    let adds = analyze(
        Target::Aarch64,
        &[0x20, 0x04, 0x00, 0xb1],
        Address::new(0x1000),
    )?;
    assert!(
        adds.registers
            .contains(&register("nzcv", DataAccess::Write))
    );
    assert!(matches!(
        adds.architecture,
        ArchitectureAnalysis::Aarch64 {
            updates_flags: true,
            ..
        }
    ));
    let call = analyze(
        Target::Aarch64,
        &[0x02, 0x00, 0x00, 0x94],
        Address::new(0x1000),
    )?;
    assert_eq!(
        call.branch_target.map(|value| value.address().get()),
        Some(0x1008)
    );
    assert!(
        call.registers
            .iter()
            .any(|value| ["lr", "x30"].contains(&value.name.as_str())
                && value.access == DataAccess::Write)
    );
    let ret = analyze(
        Target::Aarch64,
        &[0xc0, 0x03, 0x5f, 0xd6],
        Address::new(0x1000),
    )?;
    assert_eq!(ret.branch_target, None);
    Ok(())
}

#[test]
fn analysis_requires_exactly_one_complete_nonwrapping_instruction() -> Result<(), DecodeError> {
    for (target, bytes, base) in [
        (Target::X86_64, vec![], 0),
        (Target::X86_64, vec![0x48], 0),
        (Target::X86_64, vec![0x90, 0x90], 0),
        (Target::X86_64, vec![0x66; 16], 0),
        (Target::X86_64, vec![0xeb, 0], u64::MAX),
        (Target::Aarch64, vec![0x1f, 0x20, 0x03], 0),
        (Target::Aarch64, vec![0x1f, 0x20, 0x03, 0xd5], 1),
        (Target::Aarch64, vec![0xff; 4], 0),
        (Target::Aarch64, [0x1f, 0x20, 0x03, 0xd5].repeat(2), 0),
    ] {
        assert!(analyze(target, &bytes, Address::new(base)).is_err());
    }
    // Input bytes cannot wrap, but an architectural branch destination can.
    for (target, bytes, base, destination) in [
        (Target::X86_64, &[0x90][..], u64::MAX, None),
        (
            Target::Aarch64,
            &[0x1f, 0x20, 0x03, 0xd5][..],
            u64::MAX - 3,
            None,
        ),
        (Target::X86_64, &[0xeb, 0xfc][..], 0, Some(u64::MAX - 1)),
        (
            Target::Aarch64,
            &[0x40, 0x00, 0x18, 0x36][..],
            u64::MAX - 3,
            Some(4),
        ),
    ] {
        let facts = analyze(target, bytes, Address::new(base))?;
        assert_eq!(
            facts.branch_target.map(|value| value.address().get()),
            destination
        );
    }
    Ok(())
}

proptest! {
    #[test]
    fn immediate_moves_write_only_the_encoded_x86_register(index in 0_u8..16, immediate in any::<u64>()) {
        let mut bytes = vec![0x48 | (index >> 3), 0xb8 | (index & 7)];
        bytes.extend(immediate.to_le_bytes());
        let facts = x86(&bytes)?;
        let names = ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];
        prop_assert_eq!(facts.registers, [register(names[usize::from(index)], DataAccess::Write)]);
        prop_assert!(facts.memory.is_empty());
        prop_assert_eq!(facts.branch_target, None);
    }

    #[test]
    fn relative_branches_preserve_signed_offsets_and_full_address_width(
        base in any::<u64>(),
        short_offset in any::<i8>(),
        word_offset in -8192_i16..8192,
    ) {
        let base = base & !3;
        for (opcode, expected_flow) in [
            (0xeb, FlowControl::Branch),
            (0x74, FlowControl::ConditionalBranch),
        ] {
            let facts = analyze(Target::X86_64, &[opcode, short_offset.cast_unsigned()], Address::new(base))?;
            prop_assert_eq!(facts.branch_target.map(|value| value.address().get()), Some(base.wrapping_add(2).wrapping_add_signed(i64::from(short_offset))));
            let ArchitectureAnalysis::X86 { flow, .. } = facts.architecture else {
                return Err(TestCaseError::fail("wrong architecture"));
            };
            prop_assert_eq!(flow, expected_flow);
        }
        // TBZ W0,#3: imm14 encodes a signed word offset; #3 is a separate bit index.
        let instruction = 0x3618_0000 | ((u32::from(word_offset.cast_unsigned()) & 0x3fff) << 5);
        let facts = analyze(Target::Aarch64, &instruction.to_le_bytes(), Address::new(base))?;
        prop_assert_eq!(facts.branch_target.map(|value| value.address().get()), Some(base.wrapping_add_signed(i64::from(word_offset) * 4)));
    }
}

#[test]
fn x86_control_flow_distinguishes_direct_indirect_and_exception_paths() -> Result<(), DecodeError> {
    for (bytes, expected) in [
        (&[0x90][..], FlowControl::Next),
        (&[0xeb, 0][..], FlowControl::Branch),
        (&[0x75, 0][..], FlowControl::ConditionalBranch),
        (&[0xff, 0xe0][..], FlowControl::IndirectBranch),
        (&[0xe8, 0, 0, 0, 0][..], FlowControl::Call),
        (&[0xff, 0xd0][..], FlowControl::IndirectCall),
        (&[0xc3][..], FlowControl::Return),
        (&[0xcc][..], FlowControl::Interrupt),
        (&[0x0f, 0x0b][..], FlowControl::Exception),
        (&[0xc7, 0xf8, 0, 0, 0, 0][..], FlowControl::Transaction),
    ] {
        let ArchitectureAnalysis::X86 { flow, .. } = x86(bytes)?.architecture else {
            panic!("wrong target");
        };
        assert_eq!(flow, expected);
    }
    Ok(())
}

#[test]
fn representative_extensions_assemble_to_known_bytes_and_retain_decoder_tags()
-> Result<(), Box<dyn std::error::Error>> {
    for (target, source, bytes, feature) in [
        (
            Target::X86_64,
            "pxor xmm0, xmm0",
            &[0x66, 0x0f, 0xef, 0xc0][..],
            Some("SSE2"),
        ),
        (
            Target::X86_64,
            "vaddps ymm0, ymm1, ymm2",
            &[0xc5, 0xf4, 0x58, 0xc2][..],
            Some("AVX"),
        ),
        (
            Target::Aarch64,
            "add v0.16b, v1.16b, v2.16b",
            &[0x20, 0x84, 0x22, 0x4e][..],
            None,
        ),
        (
            Target::Aarch64,
            ".arch armv8-a+crypto\naese v0.16b, v1.16b",
            &[0x20, 0x48, 0x28, 0x4e][..],
            None,
        ),
        (
            Target::Aarch64,
            ".arch armv8-a+sve\nptrue p0.b",
            &[0xe0, 0xe3, 0x18, 0x25][..],
            None,
        ),
    ] {
        let object = oplab_toolchain::assembly::compile(target, source)
            .map_err(|error| format!("{error:?}"))?;
        let file = object::File::parse(object.bytes())?;
        assert_eq!(
            file.section_by_name(".text")
                .ok_or("missing text")?
                .data()?,
            bytes
        );
        let facts = analyze(target, bytes, Address::new(0x1000))?;
        let tags = match facts.architecture {
            ArchitectureAnalysis::X86 { isa, .. } => vec![isa],
            ArchitectureAnalysis::Aarch64 { groups, .. } => groups,
        };
        if let Some(feature) = feature {
            assert!(
                tags.iter().any(|tag| tag == feature),
                "missing {feature}: {tags:?}"
            );
        }
    }
    Ok(())
}
