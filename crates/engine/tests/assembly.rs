//! LLVM assembly is verified against architectural encodings, not decoder round trips.

use object::{Object, ObjectSection, ObjectSegment, ObjectSymbol};
use oplab_core::target::Target;
use oplab_core::{
    address::Address,
    protocol::{
        BuildIdentity, DiagnosticCode,
        scalar::{Counter, HexAddress},
    },
};
use oplab_engine::assembly::{self, BuildArtifact};

fn build(target: Target) -> BuildIdentity {
    BuildIdentity {
        document: "fixture".into(),
        revision: Counter::new(1),
        target,
        base: HexAddress::new(Address::new(0x1000)),
        assembler: assembly::identity(),
    }
}

#[test]
fn integer_and_backward_branch_encodings_match_architecture_fixtures()
-> Result<(), Box<dyn std::error::Error>> {
    // x86 JMP rel8 displacement -3; A64 B imm26 displacement -4, scaled by four.
    let cases = [
        (
            Target::X86_64,
            "again: nop\njmp again",
            vec![0x90, 0xeb, 0xfd],
        ),
        (
            Target::Aarch64,
            "again: add x0, x0, #1\nb again",
            vec![0x00, 0x04, 0x00, 0x91, 0xff, 0xff, 0xff, 0x17],
        ),
    ];
    for (target, source, expected) in cases {
        let result = assembly::assemble(build(target), source);
        let artifact = result.map_err(|error| format!("{error:?}"))?;
        assert_eq!(text(&artifact)?, expected);
        assert_eq!(symbol(&artifact, "again")?, 0x1000);
    }
    Ok(())
}

#[test]
fn assembly_rejects_limits_and_mismatched_settings() {
    for target in [Target::X86_64, Target::Aarch64] {
        let oversized = assembly::assemble(build(target), ".space 1048576");
        assert_eq!(
            oversized.map(|_| ()).map_err(|error| error.code),
            Err(DiagnosticCode::ResourceLimit)
        );
        let mut identity = build(target);
        identity.assembler.version = "unknown".into();
        assert_eq!(
            assembly::assemble(identity, "nop")
                .map(|_| ())
                .map_err(|error| error.code),
            Err(DiagnosticCode::BackendMismatch)
        );
    }
}

#[test]
fn relocation_boundaries_use_the_final_layout() -> Result<(), Box<dyn std::error::Error>> {
    let long_jump =
        assembly::assemble(build(Target::X86_64), "jmp target\n.space 200\ntarget: ret");
    let artifact = long_jump.map_err(|error| format!("{error:?}"))?;
    assert_eq!(text(&artifact)?.get(..5), Some(&[0xe9, 0xc8, 0, 0, 0][..]));
    assert_eq!(text(&artifact)?.len(), 206);

    let adr = assembly::assemble(build(Target::Aarch64), "adr x0, target\nnop\ntarget: nop");
    let artifact = adr.map_err(|error| format!("{error:?}"))?;
    assert_eq!(text(&artifact)?.get(..4), Some(&[0x40, 0, 0, 0x10][..]));

    let adrp = assembly::assemble(
        build(Target::Aarch64),
        "adrp x0, target\n.space 4092\ntarget: nop",
    );
    let artifact = adrp.map_err(|error| format!("{error:?}"))?;
    assert_eq!(text(&artifact)?.get(..4), Some(&[0, 0, 0, 0xb0][..]));

    Ok(())
}

#[test]
fn absolute_relocations_and_requested_operand_widths_are_preserved()
-> Result<(), Box<dyn std::error::Error>> {
    for (target, source, expected) in [
        (
            Target::X86_64,
            "movabs rax, offset target\ntarget: nop",
            vec![0x48, 0xb8, 0x0a, 0x10, 0, 0, 0, 0, 0, 0, 0x90],
        ),
        (
            Target::X86_64,
            "movabs rax, 42",
            vec![0x48, 0xb8, 0x2a, 0, 0, 0, 0, 0, 0, 0],
        ),
        (
            Target::Aarch64,
            ".quad target\ntarget: nop",
            vec![0x08, 0x10, 0, 0, 0, 0, 0, 0, 0x1f, 0x20, 0x03, 0xd5],
        ),
        (
            Target::X86_64,
            "lea rax, [rip + target]\ntarget: nop",
            vec![0x48, 0x8d, 0x05, 0, 0, 0, 0, 0x90],
        ),
    ] {
        let result = assembly::assemble(build(target), source);
        let artifact = result.map_err(|error| format!("{error:?}"))?;
        assert_eq!(text(&artifact)?, expected);
    }
    Ok(())
}

#[test]
fn assembler_filesystem_is_empty_and_failure_does_not_poison_the_next_call()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = tempfile::tempdir()?;
    let sentinel = scratch.path().join("sentinel.s");
    std::fs::write(&sentinel, "nop")?;
    for directive in [".include", "\".include\"", ".incbin"] {
        let source = format!("{directive} \"{}\"", sentinel.display());
        assert_eq!(
            assembly::assemble(build(Target::X86_64), &source)
                .map(|_| ())
                .map_err(|error| error.code),
            Err(DiagnosticCode::Assembly)
        );
    }
    for target in [Target::X86_64, Target::Aarch64] {
        let dependency = assembly::compile(target, ".globl external\nexternal: nop")
            .map_err(|error| format!("{error:?}"))?;
        let path = scratch.path().join("dependency.o");
        std::fs::write(&path, dependency.bytes())?;
        let source = format!(
            ".quad external\n.section .deplibs,\"MS\",%llvm_dependent_libraries,1\n.asciz \"{}\"",
            path.display()
        );
        let object = assembly::compile(target, &source).map_err(|error| format!("{error:?}"))?;
        assert!(
            object::File::parse(object.bytes())?
                .section_by_name(".deplibs")
                .is_some()
        );
        assert_eq!(
            assembly::link(&object, Address::new(0x1000)).map_err(|error| error.code),
            Err(DiagnosticCode::Assembly)
        );
    }
    assert!(assembly::assemble(build(Target::X86_64), "nop").is_ok());
    Ok(())
}

#[test]
fn standard_section_layout_preserves_origin_and_alignment() -> Result<(), Box<dyn std::error::Error>>
{
    for target in [Target::X86_64, Target::Aarch64] {
        for directive in [".org", "\".org\""] {
            let source = format!("{directive} 16, 0x42\ndone: nop");
            let result = assembly::assemble(build(target), &source);
            let artifact = result.map_err(|error| format!("{error:?}"))?;
            assert_eq!(text(&artifact)?.get(..16), Some(&[0x42; 16][..]));
            assert_eq!(symbol(&artifact, "done")?, 0x1010);
        }
        // Section alignment can make a legal PT_LOAD extent exceed a decode window.
        let artifact = assembly::assemble(
            build(target),
            ".text\nnop\n.section .code,\"ax\"\n.p2align 17\ndone: nop",
        )
        .map_err(|error| format!("{error:?}"))?;
        let file = object::File::parse(artifact.image.as_slice())?;
        assert!(
            file.segments()
                .any(|segment| segment.file_range().1 > 65536)
        );
        assert_eq!(symbol(&artifact, "done")?, 0x20000);
    }
    let source = "nop\n.p2align 4\ndone: ret";
    let result = assembly::assemble(build(Target::X86_64), source);
    let artifact = result.map_err(|error| format!("{error:?}"))?;
    assert_eq!(text(&artifact)?.len(), 17);
    assert_eq!(text(&artifact)?.first(), Some(&0x90));
    assert_eq!(text(&artifact)?.last(), Some(&0xc3));
    assert_eq!(symbol(&artifact, "done")?, 0x1010);

    let mut identity = build(Target::X86_64);
    identity.base = HexAddress::new(Address::new(0x1001));
    assert!(assembly::assemble(identity.clone(), "nop").is_ok());
    assert_eq!(
        assembly::assemble(identity, source)
            .map(|_| ())
            .map_err(|error| error.code),
        Err(DiagnosticCode::InvalidInput)
    );
    Ok(())
}

#[test]
fn diagnostics_refer_to_original_utf8_source_or_have_no_location()
-> Result<(), Box<dyn std::error::Error>> {
    for target in [Target::X86_64, Target::Aarch64] {
        for newline in ["\n", "\r\n"] {
            let source = format!("// 中文 😀e\u{301}{newline}  invalid_opcode");
            let error = assembly::compile(target, &source)
                .err()
                .ok_or("invalid opcode accepted")?;
            assert_eq!(error.code, DiagnosticCode::Assembly);
            assert_eq!(
                error.source_offset.map(|offset| offset as usize),
                source.find("invalid_opcode")
            );
        }
        let source = ".macro broken\ninvalid_opcode\n.endm\nbroken";
        let error = assembly::compile(target, source)
            .err()
            .ok_or("invalid macro accepted")?;
        assert_eq!(error.code, DiagnosticCode::Assembly);
        // Expanded macro buffers have no verified original-source location.
        assert!(error.source_offset.is_none());
        let error = assembly::assemble(
            build(target),
            if target == Target::X86_64 {
                "jmp missing_symbol"
            } else {
                "b missing_symbol"
            },
        )
        .err()
        .ok_or("undefined symbol accepted")?;
        assert_eq!(error.code, DiagnosticCode::Assembly);
        assert!(error.source_offset.is_none());
    }
    Ok(())
}

#[test]
fn literal_pools_and_macro_expansion_follow_standard_finalization()
-> Result<(), Box<dyn std::error::Error>> {
    for (target, source, expected) in [
        (
            Target::Aarch64,
            "ldr x0, =0x1122334455667788\nret",
            vec![
                0x40, 0, 0, 0x58, 0xc0, 0x03, 0x5f, 0xd6, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22,
                0x11,
            ],
        ),
        (
            Target::X86_64,
            ".macro twice\n.rept 2\nnop\n.endr\n.endm\ntwice\n1: jmp 1b",
            vec![0x90, 0x90, 0xeb, 0xfe],
        ),
    ] {
        let result = assembly::assemble(build(target), source);
        let artifact = result.map_err(|error| format!("{error:?}"))?;
        assert_eq!(text(&artifact)?, expected);
    }
    Ok(())
}

fn text(artifact: &BuildArtifact) -> Result<&[u8], Box<dyn std::error::Error>> {
    let file = object::File::parse(artifact.image.as_slice())?;
    Ok(file
        .section_by_name(".text")
        .ok_or("missing text")?
        .data()?)
}

fn symbol(artifact: &BuildArtifact, name: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let file = object::File::parse(artifact.image.as_slice())?;
    file.symbols()
        .find(|symbol| symbol.name() == Ok(name))
        .map(|symbol| symbol.address())
        .ok_or_else(|| "missing symbol".into())
}

#[test]
fn complete_objects_preserve_relocations_sections_symbols_and_zero_fill()
-> Result<(), Box<dyn std::error::Error>> {
    for target in [Target::X86_64, Target::Aarch64] {
        let source = ".text\n.globl start\n.type start, %function\nstart: nop\n.size start, .-start\n.data\n.globl pointer\npointer: .quad start\n.section .rodata\nmessage: .asciz \"ok\"\n.bss\n.balign 16\nbuffer: .skip 32\n.section .custom,\"a\"\n.quad buffer";
        let object = assembly::compile(target, source).map_err(|error| format!("{error:?}"))?;
        let parsed = object::File::parse(object.bytes())?;
        assert_eq!(parsed.kind(), object::ObjectKind::Relocatable);
        assert_eq!(
            parsed
                .section_by_name(".data")
                .ok_or("missing data")?
                .relocations()
                .count(),
            1
        );
        assert!(parsed.section_by_name(".debug_line").is_some());
        for base in [0x1000, 0x8000] {
            let bytes = assembly::link(&object, Address::new(base))
                .map_err(|error| format!("{error:?}"))?;
            let image = object::File::parse(bytes.as_slice())?;
            assert_eq!(image.kind(), object::ObjectKind::Executable);
            assert_eq!(
                image
                    .section_by_name(".text")
                    .ok_or("missing text")?
                    .address(),
                base
            );
            assert_eq!(
                image
                    .section_by_name(".data")
                    .ok_or("missing data")?
                    .data()?,
                base.to_le_bytes()
            );
            assert_eq!(
                image
                    .section_by_name(".rodata")
                    .ok_or("missing rodata")?
                    .data()?,
                b"ok\0"
            );
            assert_eq!(
                image.section_by_name(".bss").ok_or("missing bss")?.size(),
                32
            );
            assert!(image.section_by_name(".custom").is_some());
            assert!(
                image
                    .segments()
                    .any(|segment| segment.size() > segment.file_range().1)
            );
            let start = image
                .symbols()
                .find(|symbol| symbol.name() == Ok("start"))
                .ok_or("missing start")?;
            assert!(start.is_global());
            assert_eq!(start.kind(), object::SymbolKind::Text);
            assert_eq!(start.size(), target.instruction_alignment());
            assert_segment_permissions(&image)?;
        }
    }
    let unresolved =
        assembly::compile(Target::X86_64, "call external").map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        assembly::link(&unresolved, Address::new(0x1000))
            .map(|_| ())
            .map_err(|error| error.code),
        Err(DiagnosticCode::Assembly)
    );
    Ok(())
}

#[test]
fn data_only_images_and_bss_obey_standard_geometry() -> Result<(), Box<dyn std::error::Error>> {
    for target in [Target::X86_64, Target::Aarch64] {
        let object =
            assembly::compile(target, ".data\n.quad 42").map_err(|error| format!("{error:?}"))?;
        let bytes =
            assembly::link(&object, Address::new(0x1000)).map_err(|error| format!("{error:?}"))?;
        let image = object::File::parse(bytes.as_slice())?;
        assert_eq!(
            image
                .section_by_name(".data")
                .ok_or("missing data")?
                .data()?,
            42_u64.to_le_bytes()
        );
        assert_eq!(
            assembly::compile(target, "nop\n.bss\n.space 65537")
                .map(|_| ())
                .map_err(|error| error.code),
            Err(DiagnosticCode::ResourceLimit)
        );
        let object = assembly::compile(target, "nop\n.data\n.quad 42")
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(
            assembly::link(&object, Address::new(u64::MAX - 3)).map_err(|error| error.code),
            Err(DiagnosticCode::InvalidInput)
        );
    }
    Ok(())
}

fn assert_segment_permissions(image: &object::File<'_>) -> Result<(), Box<dyn std::error::Error>> {
    for (name, writable, executable) in [
        (".text", false, true),
        (".rodata", false, false),
        (".data", true, false),
    ] {
        let section = image.section_by_name(name).ok_or("missing section")?;
        let segment = image
            .segments()
            .find(|segment| {
                section.address() >= segment.address()
                    && u128::from(section.address())
                        < u128::from(segment.address()) + u128::from(segment.size())
            })
            .ok_or("missing load segment")?;
        assert_eq!(
            segment.permissions(),
            object::Permissions::new(true, writable, executable)
        );
    }
    Ok(())
}
