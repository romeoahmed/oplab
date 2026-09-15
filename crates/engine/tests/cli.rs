//! CLI input, output and error contracts against real engine operations.

mod common;

use object::{Object, ObjectSection};
use oplab_core::protocol::{DiagnosticCode, Reply, Response};

#[test]
fn cli_analysis_distinguishes_instruction_errors_from_input_budget_failures()
-> Result<(), Box<dyn std::error::Error>> {
    for (target, bytes, destination) in [
        ("x86_64", &[0xeb, 0xfe][..], 0x1000),
        ("aarch64", &[0xff, 0xff, 0xff, 0x17][..], 0xffc),
    ] {
        let output = common::run(
            env!("CARGO_BIN_EXE_oplab-cli"),
            &["analyze", target, "0x1000"],
            bytes,
        )?;
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let Reply::Analyzed(facts) = serde_json::from_slice::<Response>(&output.stdout)?.result
        else {
            return Err("missing analysis".into());
        };
        assert_eq!(
            facts.branch_target.map(|value| value.address().get()),
            Some(destination)
        );
        for invalid in [vec![], bytes.repeat(2)] {
            let output = common::run(
                env!("CARGO_BIN_EXE_oplab-cli"),
                &["analyze", target, "0x1000"],
                &invalid,
            )?;
            assert!(!output.status.success());
            assert!(output.stderr.is_empty());
            assert!(matches!(
                serde_json::from_slice::<Response>(&output.stdout)?.result,
                Reply::Error(error) if error.code == DiagnosticCode::Decode
            ));
        }
    }
    let oversized = common::run(
        env!("CARGO_BIN_EXE_oplab-cli"),
        &["analyze", "x86_64", "0x1000"],
        &[0x90; 16],
    )?;
    assert!(!oversized.status.success());
    assert!(oversized.stdout.is_empty());
    assert!(!oversized.stderr.is_empty());
    Ok(())
}

#[test]
fn cli_assembles_both_targets_and_reports_guest_bytes() -> Result<(), Box<dyn std::error::Error>> {
    for (target, source, expected) in [
        (
            "x86_64",
            "mov rax, 42",
            vec![0x48, 0xc7, 0xc0, 0x2a, 0, 0, 0],
        ),
        ("aarch64", "add x0, x0, #1", vec![0x00, 0x04, 0x00, 0x91]),
    ] {
        let output = common::run(
            env!("CARGO_BIN_EXE_oplab-cli"),
            &["assemble", target, "0x1000"],
            source.as_bytes(),
        )?;
        assert!(output.status.success(), "CLI failed");
        assert!(output.stderr.is_empty());
        let image = object::File::parse(output.stdout.as_slice())?;
        assert_eq!(image.kind(), object::ObjectKind::Executable);
        assert_eq!(
            image
                .section_by_name(".text")
                .ok_or("missing text")?
                .data()?,
            expected
        );
    }
    let invalid = common::run(
        env!("CARGO_BIN_EXE_oplab-cli"),
        &["assemble", "x86_64", "0x0000000000001000"],
        b"jmp missing",
    )?;
    assert!(!invalid.status.success());
    assert!(matches!(
        serde_json::from_slice::<Response>(&invalid.stderr)?.result,
        Reply::Error(_)
    ));
    Ok(())
}

#[test]
fn address_space_end_is_valid_but_wrapping_output_is_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    for (target, base, expected) in [
        ("x86_64", "0xffffffffffffffff", vec![0x90]),
        (
            "aarch64",
            "0xfffffffffffffffc",
            vec![0x1f, 0x20, 0x03, 0xd5],
        ),
    ] {
        let output = common::run(
            env!("CARGO_BIN_EXE_oplab-cli"),
            &["assemble", target, base],
            b"nop",
        )?;
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let image = object::File::parse(output.stdout.as_slice())?;
        assert_eq!(
            image
                .section_by_name(".text")
                .ok_or("missing text")?
                .data()?,
            expected
        );
        let wrapping = common::run(
            env!("CARGO_BIN_EXE_oplab-cli"),
            &["assemble", target, base],
            b"nop\nnop",
        )?;
        assert!(!wrapping.status.success());
        assert!(wrapping.stdout.is_empty());
        assert!(matches!(
            serde_json::from_slice::<Response>(&wrapping.stderr)?.result,
            Reply::Error(_)
        ));
    }
    Ok(())
}

#[test]
fn source_diagnostics_and_host_output_cannot_corrupt_cli_responses()
-> Result<(), Box<dyn std::error::Error>> {
    for source in [
        ".print \"private text\"\nnop",
        "\".print\" \"private text\"\nnop",
        ".macro broken\ninvalid_opcode\n.endm\nbroken",
        ".warning \"private text\"\nnop",
    ] {
        let output = common::run(
            env!("CARGO_BIN_EXE_oplab-cli"),
            &["assemble", "x86_64", "0x0000000000001000"],
            source.as_bytes(),
        )?;
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(matches!(
            serde_json::from_slice::<Response>(&output.stderr)?.result,
            Reply::Error(_)
        ));
    }
    Ok(())
}

#[test]
fn cli_decodes_raw_stdin_and_reports_invalid_arguments_separately()
-> Result<(), Box<dyn std::error::Error>> {
    let capabilities = common::run(env!("CARGO_BIN_EXE_oplab-cli"), &["capabilities"], &[])?;
    assert!(capabilities.status.success());
    assert!(capabilities.stderr.is_empty());
    let Reply::Hello(capabilities) =
        serde_json::from_slice::<Response>(&capabilities.stdout)?.result
    else {
        return Err("missing capabilities".into());
    };
    assert!(capabilities.execution);
    for target in [
        oplab_core::target::Target::X86_64,
        oplab_core::target::Target::Aarch64,
    ] {
        assert!(capabilities.targets.contains(&target));
    }
    for (target, bytes) in [
        ("x86_64", &[0x90][..]),
        ("aarch64", &[0x1f, 0x20, 0x03, 0xd5][..]),
    ] {
        let output = common::run(
            env!("CARGO_BIN_EXE_oplab-cli"),
            &["decode", target, "0x1000"],
            bytes,
        )?;
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let Reply::Decoded(decoded) = serde_json::from_slice::<Response>(&output.stdout)?.result
        else {
            return Err("missing decoded response".into());
        };
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].bytes, bytes);
        assert_eq!(decoded[0].address.address().get(), 0x1000);
    }
    let output = common::run(
        env!("CARGO_BIN_EXE_oplab-cli"),
        &["assemble", "arm", "0x1000"],
        &[],
    )?;
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    Ok(())
}
