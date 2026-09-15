//! Batch execution through the actual CLI, with independent result and exit-status oracles.

mod common;

use object::{Object, ObjectSection, ObjectSymbol};
use oplab_core::{address::Address, target::Target};
use oplab_engine::assembly;
use proptest::prelude::*;
use serde_json::Value;
use std::process::Output;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn run(arguments: &[&str], input: &[u8]) -> TestResult<(Output, Value)> {
    let output = common::run(env!("CARGO_BIN_EXE_oplab-cli"), arguments, input)?;
    assert!(
        output.stderr.is_empty(),
        "{arguments:?}: unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout.last(), Some(&b'\n'), "{arguments:?}");
    let json = serde_json::from_slice(&output.stdout)?;
    Ok((output, json))
}

fn reject(arguments: &[&str], input: &[u8], code: &str) -> TestResult {
    let (output, json) = run(arguments, input)?;
    assert_eq!(output.status.code(), Some(1), "{arguments:?}: {json}");
    assert_eq!(json["type"], "error");
    assert_eq!(json["data"]["code"], code);
    Ok(())
}

fn image(target: Target, source: &str) -> TestResult<Vec<u8>> {
    let object = assembly::compile(target, source).map_err(|error| format!("{error:?}"))?;
    assembly::link(&object, Address::new(0x1000)).map_err(|error| format!("{error:?}").into())
}

fn symbol(image: &[u8], name: &str) -> TestResult<u64> {
    object::File::parse(image)?
        .symbols()
        .find(|symbol| symbol.name() == Ok(name))
        .map(|symbol| symbol.address())
        .ok_or_else(|| "missing fixture symbol".into())
}

fn check_arithmetic(value: u64, add: u64) -> TestResult {
    for (guest, target, code, bank, pc, instructions) in [
        (
            "x86_64",
            Target::X86_64,
            "mov rax, [rip + value]\nadd rax, [rip + value + 8]\nmov [rip + value], rax",
            "gpr",
            "rip",
            "3",
        ),
        (
            "aarch64",
            Target::Aarch64,
            "adr x1, value\nldp x0, x2, [x1]\nadd x0, x0, x2\nstr x0, [x1]",
            "x",
            "pc",
            "4",
        ),
    ] {
        let source =
            format!("// 算术 λ\n.text\n{code}\ndone: nop\n.data\nvalue: .quad {value}, {add}");
        let image = image(target, &source)?;
        let memory = format!("0x{:016x}", symbol(&image, "value")?);
        let completion = format!("0x{:016x}", symbol(&image, "done")?);
        let policy = [
            "--until-symbol",
            "done",
            "--budget",
            instructions,
            "--memory",
            &memory,
            "--memory-bytes",
            "8",
        ];
        let source_args = [&["run", "source", guest, "0x1000"][..], &policy].concat();
        let elf_args = [&["run", "elf", guest][..], &policy].concat();
        let expected = value.wrapping_add(add);
        for (arguments, input) in [
            (&source_args, source.as_bytes()),
            (&elf_args, image.as_slice()),
        ] {
            let (output, result) = run(arguments, input)?;
            assert!(output.status.success(), "{arguments:?}: {result}");
            assert_eq!(result["type"], "executed");
            let report = &result["data"];
            assert_eq!(report["target"], guest);
            assert_eq!(report["outcome"], "completed");
            assert_eq!(report["instructions"], instructions);
            assert_eq!(report["completion"], completion);
            assert_eq!(report["registers"]["type"], guest);
            assert_eq!(report["registers"]["data"][pc], completion);
            assert_eq!(report["registers"]["data"][bank][0], expected.to_string());
            assert_eq!(report["memory"]["address"], memory);
            assert_eq!(
                report["memory"]["bytes"],
                serde_json::json!(expected.to_le_bytes())
            );
            assert_eq!(report.get("fault"), Some(&Value::Null));
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]

    #[test]
    fn source_and_elf_runs_preserve_full_width_results(value in any::<u64>(), add in any::<u64>()) {
        check_arithmetic(value, add).map_err(|error| TestCaseError::fail(error.to_string()))?;
    }
}

#[test]
fn arithmetic_boundaries_preserve_exact_json_and_wrapping_effects() -> TestResult {
    for (value, add) in [
        (0, 0),
        (1 << 53, 1),
        ((1 << 63) - 1, 1),
        (u64::MAX, 0),
        (u64::MAX, 1),
    ] {
        check_arithmetic(value, add)?;
    }
    Ok(())
}

#[test]
fn elf_entry_and_program_headers_work_without_section_headers() -> TestResult {
    let mut image = image(Target::X86_64, "nop\nmov eax, 42")?;
    // ELF64 e_entry, e_shoff, e_shentsize/e_shnum/e_shstrndx: no source-view metadata.
    image[24..32].copy_from_slice(&0x1001_u64.to_le_bytes());
    image[40..48].fill(0);
    image[58..64].fill(0);
    let (output, json) = run(
        &["run", "elf", "x86_64", "--until", "0x1006", "--budget", "1"],
        &image,
    )?;
    assert!(output.status.success());
    assert_eq!(json["data"]["outcome"], "completed");
    assert_eq!(json["data"]["registers"]["data"]["gpr"][0], "42");
    assert_eq!(
        json["data"]["registers"]["data"]["rip"],
        "0x0000000000001006"
    );
    assert_eq!(json["data"]["instructions"], "1");
    reject(
        &[
            "run",
            "elf",
            "x86_64",
            "--until-symbol",
            "done",
            "--budget",
            "1",
        ],
        &image,
        "invalid_input",
    )
}

#[test]
fn noncompletion_reports_partial_effects_and_never_exits_successfully() -> TestResult {
    for (guest, source, expected) in [
        (
            "x86_64",
            "mov eax, 42\nagain: jmp again\ndone: nop",
            "budget",
        ),
        (
            "aarch64",
            "mov x0, #42\nagain: b again\ndone: nop",
            "budget",
        ),
        (
            "x86_64",
            "mov eax, 42\nmov ecx, [0x900000]\ndone: nop",
            "guest_fault",
        ),
        (
            "aarch64",
            "mov x0, #42\nmov x1, #0x900000\nldr x1, [x1]\ndone: nop",
            "guest_fault",
        ),
        (
            "x86_64",
            "mov eax, 42\nsyscall\ndone: nop",
            "unsupported_environment",
        ),
        (
            "aarch64",
            "mov x0, #42\nsvc #0\ndone: nop",
            "unsupported_environment",
        ),
    ] {
        let (output, json) = run(
            &[
                "run",
                "source",
                guest,
                "0x1000",
                "--until-symbol",
                "done",
                "--budget",
                "3",
            ],
            source.as_bytes(),
        )?;
        assert_eq!(output.status.code(), Some(1), "{guest}: {source}: {json}");
        let report = &json["data"];
        assert_eq!(json["type"], "executed");
        assert_eq!(report["outcome"], expected);
        let bank = if guest == "x86_64" { "gpr" } else { "x" };
        assert_eq!(report["registers"]["data"][bank][0], "42");
        if expected == "budget" {
            assert_eq!(report["instructions"], "3");
        } else if expected == "guest_fault" {
            assert_eq!(report["fault"]["kind"]["type"], "unmapped");
            assert_eq!(report["fault"]["kind"]["data"], "read");
            assert_eq!(report["fault"]["address"], "0x0000000000900000");
        }
    }
    Ok(())
}

#[test]
fn timeout_stops_a_nonterminating_guest_with_an_observation() -> TestResult {
    let (output, json) = run(
        &[
            "run",
            "source",
            "x86_64",
            "0x1000",
            "--until-symbol",
            "done",
            "--budget",
            "100000000",
            "--timeout-ms",
            "1",
        ],
        b"again: jmp again\ndone: nop",
    )?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(json["type"], "executed");
    assert_eq!(json["data"]["outcome"], "timeout");
    assert_eq!(
        json["data"]["registers"]["data"]["rip"],
        "0x0000000000001000"
    );
    assert_eq!(json["data"].get("fault"), Some(&Value::Null));
    assert_eq!(json["data"].get("memory"), Some(&Value::Null));
    Ok(())
}

#[test]
fn rejected_policy_and_input_produce_structured_errors() -> TestResult {
    let command = ["run", "source", "x86_64", "0x1000", "--budget", "1"];
    for (policy, source) in [
        ("--until-symbol missing", &b"nop"[..]),
        ("--until-symbol done", &b".file \"done\"\nnop"[..]),
        ("--until 0x1000", &b"nop"[..]),
        ("--until 0x1001", &b"\xff"[..]),
        ("--until 0x1001", &b""[..]),
        ("--until 0x1001 --memory 0x900000", &b"nop"[..]),
        (
            "--until 0x1001 --memory 0xffffffffffffffff --memory-bytes 2",
            &b"nop"[..],
        ),
    ] {
        let arguments: Vec<_> = command
            .into_iter()
            .chain(policy.split_whitespace())
            .collect();
        reject(&arguments, source, "invalid_input")?;
    }
    let arguments = [&command[..], &["--until", "0x1001"]].concat();
    let (output, json) = run(&arguments, b"private_invalid_opcode")?;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(json["type"], "error");
    assert_eq!(json["data"]["code"], "assembly");
    assert!(!String::from_utf8(output.stdout)?.contains("private_invalid_opcode"));

    let arguments = ["run", "elf", "x86_64", "--until", "0x2000", "--budget", "1"];
    reject(&arguments, &image(Target::Aarch64, "nop")?, "invalid_input")?;
    reject(&arguments, b"not an ELF", "invalid_input")
}

#[test]
fn stdin_limits_accept_the_boundary_and_reject_one_more_byte() -> TestResult {
    for (arguments, mut input, limit) in [
        (
            vec!["run", "source", "x86_64", "0x1000"],
            b"nop\n//".to_vec(),
            262_144,
        ),
        (
            vec!["run", "elf", "x86_64"],
            image(Target::X86_64, "nop")?,
            1_048_576,
        ),
    ] {
        let arguments = [arguments, vec!["--until", "0x1001", "--budget", "1"]].concat();
        // Comments and trailing ELF bytes grow stdin without expanding guest memory.
        input.resize(limit, b' ');
        let (output, json) = run(&arguments, &input)?;
        assert!(output.status.success(), "{json}");
        assert_eq!(json["data"]["outcome"], "completed");
        input.push(b' ');
        reject(&arguments, &input, "resource_limit")?;
    }
    Ok(())
}

#[test]
fn clap_enforces_exclusive_completion_and_bounded_policies() -> TestResult {
    for policy in [
        "--budget 1",
        "--until 0x1001",
        "--until 0x1001 --until-symbol done --budget 1",
        "--until 0x1001 --budget 0",
        "--until 0x1001 --budget 100000001",
        "--until 0x1001 --budget 1 --timeout-ms 0",
        "--until 0x1001 --budget 1 --timeout-ms 3600001",
        "--until 0x1001 --budget 1 --memory-bytes 8",
        "--until 0x1001 --budget 1 --memory 0x1000 --memory-bytes 0",
        "--until 0x1001 --budget 1 --memory 0x1000 --memory-bytes 65537",
    ] {
        let arguments: Vec<_> = ["run", "source", "x86_64", "0x1000"]
            .into_iter()
            .chain(policy.split_whitespace())
            .collect();
        let output = common::run(env!("CARGO_BIN_EXE_oplab-cli"), &arguments, &[])?;
        assert_eq!(output.status.code(), Some(2), "{policy}");
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    Ok(())
}

#[test]
fn completion_symbols_preserve_absolute_values_and_reject_ambiguity() -> TestResult {
    let mut image = image(Target::X86_64, "nop\n.set done, 0x1001\n.set alias, 0x1002")?;
    let arguments = [
        "run",
        "elf",
        "x86_64",
        "--until-symbol",
        "done",
        "--budget",
        "1",
    ];
    let (output, json) = run(&arguments, &image)?;
    assert!(output.status.success());
    assert_eq!(json["data"]["completion"], "0x0000000000001001");
    let file = object::File::parse(image.as_slice())?;
    let symbols = file
        .section_by_name(".symtab")
        .ok_or("missing symbol table")?;
    let (offset, _) = symbols.file_range().ok_or("missing symbol bytes")?;
    let offset = usize::try_from(offset)?;
    let entry = |name| {
        file.symbols()
            .find(|symbol| symbol.name() == Ok(name))
            .map(|symbol| offset + symbol.index().0 * 24)
            .ok_or("missing symbol")
    };
    let done = entry("done")?;
    let alias = entry("alias")?;
    // ELF64 st_shndx must resolve; STT_TLS st_value is an offset, not an address.
    for (offset, bytes) in [
        (done + 6, 127_u16.to_le_bytes().to_vec()),
        (done + 4, vec![6]),
    ] {
        let mut invalid = image.clone();
        invalid[offset..offset + bytes.len()].copy_from_slice(&bytes);
        reject(&arguments, &invalid, "invalid_input")?;
    }
    // ELF64 st_name occupies the first four bytes; keep two distinct symbol values.
    let name: [u8; 4] = image[done..done + 4].try_into()?;
    image[alias..alias + 4].copy_from_slice(&name);
    reject(&arguments, &image, "invalid_input")
}
