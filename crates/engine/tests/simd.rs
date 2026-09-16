//! Real guest stores independently verify register lane order and floating-point state.

use object::{Object, ObjectSymbol};
use oplab_core::{
    address::Address,
    execution::{ExecutionState, Termination},
    registers::MachineRegisters,
    target::{CpuModel, Target},
};
use oplab_engine::{
    assembly,
    load::{Image, MachineSetup},
    machine::Machine,
    session::Session,
};
use proptest::prelude::*;
use std::fmt::Write;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const fn models(target: Target) -> [CpuModel; 2] {
    match target {
        Target::X86_64 => [CpuModel::Haswell, CpuModel::Nehalem],
        Target::Aarch64 => [CpuModel::CortexA72, CpuModel::CortexA53],
    }
}

fn run(session: &mut Session) -> TestResult {
    session.start()?;
    for _ in 0..16 {
        if session.state() != ExecutionState::Running {
            break;
        }
        session.advance()?;
    }
    assert_eq!(
        session.state(),
        ExecutionState::Terminated(Termination::Completed),
        "{:?}: {:?}",
        session.cpu(),
        session.fault()
    );
    Ok(())
}

fn symbol(image: &[u8], name: &str) -> TestResult<Address> {
    object::File::parse(image)?
        .symbols()
        .find(|symbol| symbol.name() == Ok(name))
        .map(|symbol| Address::new(symbol.address()))
        .ok_or_else(|| "missing symbol".into())
}

fn execute(target: Target, source: &str, expected: [u8; 16], original: [u8; 16]) -> TestResult {
    let object = assembly::compile(target, source).map_err(|error| format!("{error:?}"))?;
    let image =
        assembly::link(&object, Address::new(0x1000)).map_err(|error| format!("{error:?}"))?;
    for cpu in models(target) {
        let machine = Machine::load(
            Image::Elf(&image),
            target,
            MachineSetup {
                cpu: Some(cpu),
                ..MachineSetup::default()
            },
        )?;
        let mut session = Session::new(machine, symbol(&image, "done")?, 100)?;
        assert_eq!(session.cpu(), cpu);
        let before = session.read_registers()?;
        run(&mut session)?;
        let bank = session.read_registers()?;
        let values: &[u128] = match &bank {
            MachineRegisters::X86_64 { xmm, .. } => xmm.as_slice(),
            MachineRegisters::Aarch64 { v, .. } => v.as_slice(),
        };
        assert_eq!(values[0].to_le_bytes(), expected);
        assert_eq!(values[values.len() - 1].to_le_bytes(), original);
        assert!(values[1..values.len() - 1].iter().all(|value| *value == 0));
        assert_eq!(
            session.read_memory(symbol(&image, "output")?, 16)?,
            expected
        );
        session.reset()?;
        assert_eq!(session.read_registers()?, before);
        assert_eq!(session.read_memory(symbol(&image, "output")?, 16)?, [0; 16]);
        assert_eq!(session.cpu(), cpu);
    }
    Ok(())
}

fn packed_add(values: [u32; 4]) -> TestResult {
    let original: [u8; 16] = values
        .map(u32::to_le_bytes)
        .as_flattened()
        .try_into()
        .map_err(|_| "input width")?;
    let expected: [u8; 16] = values
        .map(|value| value.wrapping_add(value).to_le_bytes())
        .as_flattened()
        .try_into()
        .map_err(|_| "output width")?;
    for (target, code) in [
        (
            Target::X86_64,
            "movdqu xmm15, [rip + input]\nmovdqa xmm0, xmm15\npaddd xmm0, xmm15\nmovdqu [rip + output], xmm0",
        ),
        (
            Target::Aarch64,
            "adr x0, input\nldr q31, [x0]\nadd v0.4s, v31.4s, v31.4s\nadr x0, output\nstr q0, [x0]",
        ),
    ] {
        let source = format!(
            ".text\n{code}\ndone: nop\n.data\ninput: .long {}, {}, {}, {}\noutput: .zero 16\n",
            values[0], values[1], values[2], values[3]
        );
        execute(target, &source, expected, original)?;
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(12))]
    #[test]
    fn packed_integer_addition_matches_wrapping_lanes(values in any::<[u32; 4]>()) {
        packed_add(values).map_err(|error| TestCaseError::fail(error.to_string()))?;
    }
}

#[test]
fn packed_boundaries_preserve_significance_and_wrap_each_lane() -> TestResult {
    packed_add([0, 1, 0x8000_0000, u32::MAX])
}

#[test]
fn every_vector_register_matches_distinct_guest_loads_and_stores() -> TestResult {
    for (target, count) in [(Target::X86_64, 16), (Target::Aarch64, 32)] {
        let mut source = String::from(".text\n");
        let mut data = String::from(".data\ninput:\n");
        let mut expected = Vec::new();
        if target == Target::Aarch64 {
            source.push_str("adr x0, input\nadr x1, output\n");
        }
        for index in 0..count {
            let low = 0x0123_4567_89ab_cd00_u64 + index;
            let high = 0xfedc_ba98_7654_3200_u64 + index;
            writeln!(data, ".quad {low}, {high}")?;
            expected.extend_from_slice(&low.to_le_bytes());
            expected.extend_from_slice(&high.to_le_bytes());
            let offset = index * 16;
            match target {
                Target::X86_64 => writeln!(
                    source,
                    "movdqu xmm{index}, [rip + input + {offset}]\nmovdqu [rip + output + {offset}], xmm{index}"
                )?,
                Target::Aarch64 => writeln!(
                    source,
                    "ldr q{index}, [x0, #{offset}]\nstr q{index}, [x1, #{offset}]"
                )?,
            }
        }
        writeln!(source, "done: nop\n{data}output: .zero {}", count * 16)?;
        let object = assembly::compile(target, &source).map_err(|error| format!("{error:?}"))?;
        let image =
            assembly::link(&object, Address::new(0x1000)).map_err(|error| format!("{error:?}"))?;
        for cpu in models(target) {
            let machine = Machine::load(
                Image::Elf(&image),
                target,
                MachineSetup {
                    cpu: Some(cpu),
                    ..MachineSetup::default()
                },
            )?;
            let mut session = Session::new(machine, symbol(&image, "done")?, 100)?;
            let initial = session.read_registers()?;
            match &initial {
                MachineRegisters::X86_64 { xmm, mxcsr, .. } => {
                    assert!(xmm.iter().all(|value| *value == 0));
                    assert_eq!(*mxcsr, 0x1f80);
                }
                MachineRegisters::Aarch64 { v, fpcr, fpsr, .. } => {
                    assert!(v.iter().all(|value| *value == 0));
                    assert_eq!((*fpcr, *fpsr), (0, 0));
                }
            }
            run(&mut session)?;
            let bank = session.read_registers()?;
            let vectors: &[u128] = match &bank {
                MachineRegisters::X86_64 { xmm, .. } => xmm.as_slice(),
                MachineRegisters::Aarch64 { v, .. } => v.as_slice(),
            };
            let observed: Vec<_> = vectors
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            assert_eq!(observed, expected, "{cpu:?}");
            assert_eq!(
                session.read_memory(symbol(&image, "output")?, count * 16)?,
                expected
            );
            session.reset()?;
            assert_eq!(session.read_registers()?, initial);
        }
    }
    Ok(())
}

#[test]
fn packed_double_precision_arithmetic_matches_exact_ieee_values() -> TestResult {
    let original = [1.5_f64.to_le_bytes(), (-2.25_f64).to_le_bytes()]
        .as_flattened()
        .try_into()
        .map_err(|_| "input width")?;
    let expected = [3.0_f64.to_le_bytes(), (-4.5_f64).to_le_bytes()]
        .as_flattened()
        .try_into()
        .map_err(|_| "output width")?;
    for (target, code) in [
        (
            Target::X86_64,
            "movupd xmm15, [rip + input]\nmovapd xmm0, xmm15\naddpd xmm0, xmm15\nmovupd [rip + output], xmm0",
        ),
        (
            Target::Aarch64,
            "adr x0, input\nldr q31, [x0]\nfadd v0.2d, v31.2d, v31.2d\nadr x0, output\nstr q0, [x0]",
        ),
    ] {
        execute(
            target,
            &format!(
                ".text\n{code}\ndone: nop\n.data\ninput: .double 1.5, -2.25\noutput: .zero 16\n"
            ),
            expected,
            original,
        )?;
    }
    Ok(())
}

#[test]
fn rounding_controls_affect_arithmetic_and_status_matches_guest_stores() -> TestResult {
    for (target, code, status) in [
        (
            Target::X86_64,
            "ldmxcsr [rip + control]\npxor xmm1, xmm1\nmovss xmm0, [rip + one]\ndivss xmm0, xmm1\nmovss xmm2, [rip + one]\naddss xmm2, [rip + half_ulp]\nstmxcsr [rip + status]",
            0x5f80_u32,
        ),
        (
            Target::Aarch64,
            "mov x0, #0x400000\nmsr fpcr, x0\nfmov s0, #1.0\nmovi v1.16b, #0\nfdiv s0, s0, s1\nfmov s2, #1.0\nadr x0, half_ulp\nldr s3, [x0]\nfadd s2, s2, s3\nmrs x0, fpsr\nadr x1, status\nstr w0, [x1]",
            0x12,
        ),
    ] {
        let source = format!(
            ".text\n{code}\ndone: nop\n.data\ncontrol: .long 0x5f80\none: .float 1.0\nhalf_ulp: .long 0x33800000\nstatus: .long 0\n"
        );
        let object = assembly::compile(target, &source).map_err(|error| format!("{error:?}"))?;
        let image =
            assembly::link(&object, Address::new(0x1000)).map_err(|error| format!("{error:?}"))?;
        for cpu in models(target) {
            let machine = Machine::load(
                Image::Elf(&image),
                target,
                MachineSetup {
                    cpu: Some(cpu),
                    ..MachineSetup::default()
                },
            )?;
            let mut session = Session::new(machine, symbol(&image, "done")?, 100)?;
            let initial = session.read_registers()?;
            run(&mut session)?;
            let (vectors, stored_status) = match session.read_registers()? {
                MachineRegisters::X86_64 { xmm, mxcsr, .. } => {
                    // Unicorn omits accrued SSE flags, including in STMXCSR.
                    // Verify controls without requiring that omission to persist.
                    assert_eq!(mxcsr & !0x3f, status);
                    ([xmm[0], xmm[2]], mxcsr)
                }
                MachineRegisters::Aarch64 { v, fpcr, fpsr, .. } => {
                    assert_eq!(fpcr, 0x0040_0000);
                    assert_eq!(fpsr, status); // Divide-by-zero and inexact.
                    ([v[0], v[2]], fpsr)
                }
            };
            assert_eq!(vectors[0], 0x7f80_0000); // Positive infinity.
            // 1 + 2^-24 rounds upward to 1 + 2^-23, rather than nearest-even's 1.
            assert_eq!(vectors[1], 0x3f80_0001);
            assert_eq!(
                session.read_memory(symbol(&image, "status")?, 4)?,
                stored_status.to_le_bytes()
            );
            session.reset()?;
            assert_eq!(session.read_registers()?, initial);
        }
    }
    Ok(())
}
