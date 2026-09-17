//! Source attribution is checked against known encodings and linked addresses.
use object::{Object, ObjectSection};
use oplab_core::{
    address::Address,
    protocol::{
        BuildIdentity,
        scalar::{Counter, HexAddress},
    },
    target::Target,
};
use oplab_toolchain::assembly;
use proptest::prelude::*;
use std::fmt::Write;

fn build(
    target: Target,
    source: &str,
    base: u64,
) -> Result<assembly::BuildArtifact, Box<dyn std::error::Error>> {
    Ok(assembly::assemble(
        BuildIdentity {
            document: "source".into(),
            revision: Counter::new(1),
            target,
            base: HexAddress::new(Address::new(base)),
            assembler: assembly::identity(),
        },
        source,
    )
    .map_err(|error| format!("{error:?}"))?)
}

#[test]
fn exact_points_follow_linking_without_attributing_data_or_padding()
-> Result<(), Box<dyn std::error::Error>> {
    for target in [Target::X86_64, Target::Aarch64] {
        for base in [0x1000, 0x8000] {
            let artifact = build(
                target,
                "# \u{1f680}\n.text\nfirst: nop\n.byte 0, 0, 0, 0\n.balign 16\nlast: nop\n",
                base,
            )?;
            let points = artifact
                .source_map
                .locations
                .iter()
                .map(|p| (p.address.address().get(), p.line))
                .collect::<Vec<_>>();
            assert_eq!(points, [(base, 3), (base + 16, 6)], "{target:?}");
            assert!(!artifact.source_map.truncated);
        }
    }
    Ok(())
}

#[test]
fn macro_and_repeat_expansions_retain_llvm_locations() -> Result<(), Box<dyn std::error::Error>> {
    for target in [Target::X86_64, Target::Aarch64] {
        let artifact = build(
            target,
            ".macro twice\nnop\nnop\n.endm\ntwice\n.rept 2\nnop\n.endr\n",
            0x1000,
        )?;
        let points = artifact
            .source_map
            .locations
            .iter()
            .map(|p| (p.address.address().get(), p.line))
            .collect::<Vec<_>>();
        let width = match target {
            Target::X86_64 => 1,  // NOP is 0x90.
            Target::Aarch64 => 4, // NOP is 0xd503201f.
        };
        assert_eq!(
            points,
            [
                (0x1000, 5),
                (0x1000 + width, 5),
                (0x1000 + 2 * width, 6),
                (0x1000 + 3 * width, 6)
            ]
        );
    }
    Ok(())
}

#[test]
fn foreign_debug_files_and_data_only_sources_have_no_document_mapping()
-> Result<(), Box<dyn std::error::Error>> {
    for source in [
        ".data\n.byte 1,2,3\n",
        ".file 1 \"foreign.c\"\n.loc 1 1\nnop\n",
    ] {
        let artifact = build(Target::X86_64, source, 0x1000)?;
        assert!(artifact.source_map.locations.is_empty());
        assert!(!artifact.source_map.truncated);
    }
    // An unnumbered .file emits a file symbol, not an authored DWARF line table.
    let artifact = build(Target::X86_64, ".file \"unit.s\"\nnop\n", 0x1000)?;
    assert_eq!(artifact.source_map.locations.len(), 1);
    assert_eq!(artifact.source_map.locations[0].line, 2);
    Ok(())
}

#[test]
fn source_view_is_bounded_without_truncating_elf() -> Result<(), Box<dyn std::error::Error>> {
    for (source, emitted, retained, truncated) in [
        ("nop\n".repeat(4096), 4096, 4096, false),
        ("nop\n".repeat(4097), 4097, 4096, true),
        (".rept 4097\nnop\n.endr\n".into(), 4097, 0, true),
        (
            format!("{}.rept 2\nnop\n.endr\n", "nop\n".repeat(4095)),
            4097,
            4095,
            true,
        ),
    ] {
        let artifact = build(Target::X86_64, &source, 0x1000)?;
        assert_eq!(artifact.source_map.truncated, truncated);
        let image = object::File::parse(artifact.image.as_slice())?;
        let text = image
            .section_by_name(".text")
            .ok_or("Missing text section")?;
        assert_eq!(text.data()?, vec![0x90; emitted]);
        let points = artifact
            .source_map
            .locations
            .iter()
            .map(|point| (point.address.address().get(), point.line))
            .collect::<Vec<_>>();
        let expected = (0..retained)
            .map(|index| (0x1000 + u64::from(index), index + 1))
            .collect::<Vec<_>>();
        assert_eq!(points, expected);
    }
    Ok(())
}

#[test]
fn source_lines_agree_with_editor_newline_normalization() -> Result<(), Box<dyn std::error::Error>>
{
    for separator in ["\n", "\r\n"] {
        let source = format!("nop{separator}nop{separator}");
        let artifact = build(Target::X86_64, &source, 0x1000)?;
        let lines = artifact
            .source_map
            .locations
            .iter()
            .map(|point| point.line)
            .collect::<Vec<_>>();
        assert_eq!(lines, [1, 2], "{separator:?}");
    }
    let artifact = build(Target::X86_64, "nop\rnop\nnop\n", 0x1000)?;
    let points = artifact
        .source_map
        .locations
        .iter()
        .map(|point| (point.address.address().get(), point.line))
        .collect::<Vec<_>>();
    assert_eq!(points, [(0x1002, 3)]);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn repeated_lines_keep_every_expansion_at_the_linked_base(
        repetitions in prop::collection::vec(0_u32..8, 1..12),
        page in 1_u64..0x10000,
    ) {
        let base = page * 4096;
        for (target, width) in [(Target::X86_64, 1_u64), (Target::Aarch64, 4)] {
            let mut source = String::from("# \u{1f680}\nnop\n");
            let mut expected = vec![(base, 2)];
            let mut address = base + width;
            let mut line = 3;
            for count in &repetitions {
                writeln!(source, ".rept {count}\nnop\n.endr")?;
                for _ in 0..*count {
                    expected.push((address, line));
                    address += width;
                }
                line += 3;
            }
            let artifact = build(target, &source, base)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let actual = artifact.source_map.locations.iter()
                .map(|point| (point.address.address().get(), point.line))
                .collect::<Vec<_>>();
            prop_assert_eq!(actual, expected);
            prop_assert!(!artifact.source_map.truncated);
        }
    }
}
