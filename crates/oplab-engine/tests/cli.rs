//! CLI and worker operations share the same real assembly implementation.

mod common;

use object::{Object, ObjectSection};
use oplab_core::protocol::{Reply, Response};

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
fn command_discovery_and_usage_errors_do_not_read_guest_input()
-> Result<(), Box<dyn std::error::Error>> {
    for arguments in [
        vec!["--help"],
        vec!["--version"],
        vec!["assemble", "--help"],
    ] {
        let output = common::run(env!("CARGO_BIN_EXE_oplab-cli"), &arguments, &[])?;
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    for arguments in [
        vec![],
        vec!["unknown"],
        vec!["assemble", "arm", "0x1000"],
        vec!["decode", "aarch64", "invalid"],
    ] {
        let output = common::run(env!("CARGO_BIN_EXE_oplab-cli"), &arguments, &[])?;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    Ok(())
}
