//! Real guest stores independently verify register lane order and floating-point state.

use object::{Object, ObjectSymbol};
use oplab_core::{
    address::Address,
    execution::{ExecutionState, Termination},
    registers::{MachineRegisters, VectorBits},
    target::Target,
};
use oplab_engine::{
    load::{Image, MachineSetup},
    machine::Machine,
    session::Session,
};
use oplab_toolchain::assembly;
use proptest::prelude::*;
use std::fmt::Write;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn run(session: &mut Session) -> TestResult {
    session.start()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        if session.state() != ExecutionState::Running {
            break;
        }
        session.advance()?;
    }
    assert_eq!(
        session.state(),
        ExecutionState::Terminated(Termination::Completed),
        "{:?}",
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

fn vectors(bank: &MachineRegisters) -> Vec<u128> {
    let values = match bank {
        MachineRegisters::X86_64 { ymm, .. } => ymm.as_slice(),
        MachineRegisters::Aarch64 { z, .. } => z.as_slice(),
    };
    values
        .iter()
        .map(|value| {
            let mut bytes = [0; 16];
            bytes.copy_from_slice(&value.as_le_bytes()[..16]);
            u128::from_le_bytes(bytes)
        })
        .collect()
}

fn bits(value: u128, width: u16) -> TestResult<VectorBits> {
    let length = usize::from(width.div_ceil(8))
        .max(usize::try_from((128 - value.leading_zeros()).div_ceil(8))?);
    Ok(VectorBits::new(value.to_le_bytes()[..length].to_vec())?)
}

fn linked(target: Target, source: &str) -> TestResult<Vec<u8>> {
    let object = assembly::compile(target, source).map_err(|error| format!("{error:?}"))?;
    assembly::link(&object, Address::new(0x1000)).map_err(|error| format!("{error:?}").into())
}

fn execute(target: Target, source: &str, expected: [u8; 16], original: [u8; 16]) -> TestResult {
    let image = linked(target, source)?;
    let machine = Machine::load(Image::Elf(&image), target, MachineSetup::default())?;
    let mut session = Session::new(machine, symbol(&image, "done")?, 100)?;
    let before = session.read_registers()?;
    run(&mut session)?;
    let bank = session.read_registers()?;
    let values = vectors(&bank);
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

    #[test]
    fn wide_integer_addition_matches_wrapping_lanes(values in any::<[u32; 8]>()) {
        wide_packed_add(values).map_err(|error| TestCaseError::fail(error.to_string()))?;
    }
}

#[test]
fn packed_boundaries_preserve_significance_and_wrap_each_lane() -> TestResult {
    packed_add([0, 1, 0x8000_0000, u32::MAX])?;
    wide_packed_add([0, 1, u32::MAX, 0x8000_0000, 7, 31, 127, 255])
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
        let image = linked(target, &source)?;
        let machine = Machine::load(Image::Elf(&image), target, MachineSetup::default())?;
        let mut session = Session::new(machine, symbol(&image, "done")?, 100)?;
        let initial = session.read_registers()?;
        match &initial {
            MachineRegisters::X86_64 { ymm, mxcsr, .. } => {
                assert!(
                    ymm.iter()
                        .all(|value| value.as_le_bytes().iter().all(|&byte| byte == 0))
                );
                assert_eq!(*mxcsr, 0x1f80);
            }
            MachineRegisters::Aarch64 { z, fpcr, fpsr, .. } => {
                assert!(
                    z.iter()
                        .all(|value| value.as_le_bytes().iter().all(|&byte| byte == 0))
                );
                assert_eq!((*fpcr, *fpsr), (0, 0));
            }
        }
        run(&mut session)?;
        let bank = session.read_registers()?;
        let vectors = vectors(&bank);
        let observed: Vec<_> = vectors
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        assert_eq!(observed, expected, "{target:?}");
        assert_eq!(
            session.read_memory(symbol(&image, "output")?, count * 16)?,
            expected
        );
        session.reset()?;
        assert_eq!(session.read_registers()?, initial);
    }
    Ok(())
}

#[test]
fn packed_double_precision_arithmetic_matches_exact_ieee_values() -> TestResult {
    let original = [1.5_f64.to_le_bytes(), (-4.0_f64).to_le_bytes()]
        .as_flattened()
        .try_into()
        .map_err(|_| "input width")?;
    let expected = [3.75_f64.to_le_bytes(), 6.5_f64.to_le_bytes()]
        .as_flattened()
        .try_into()
        .map_err(|_| "output width")?;
    for (target, code) in [
        (
            Target::X86_64,
            "movupd xmm15, [rip + input]\nmovupd xmm0, [rip + addend]\naddpd xmm0, xmm15\nmovupd [rip + output], xmm0",
        ),
        (
            Target::Aarch64,
            "adr x0, input\nldr q31, [x0]\nadr x0, addend\nldr q0, [x0]\nfadd v0.2d, v31.2d, v0.2d\nadr x0, output\nstr q0, [x0]",
        ),
    ] {
        execute(
            target,
            &format!(
                ".text\n{code}\ndone: nop\n.data\ninput: .double 1.5, -4.0\naddend: .double 2.25, 10.5\noutput: .zero 16\n"
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
        let image = linked(target, &source)?;
        let machine = Machine::load(Image::Elf(&image), target, MachineSetup::default())?;
        let mut session = Session::new(machine, symbol(&image, "done")?, 100)?;
        let initial = session.read_registers()?;
        run(&mut session)?;
        let observed = session.read_registers()?;
        let values = vectors(&observed);
        let stored_status = match observed {
            MachineRegisters::X86_64 { mxcsr, .. } => {
                assert_eq!(mxcsr, status | 0x24); // Divide-by-zero and inexact.
                mxcsr
            }
            MachineRegisters::Aarch64 { fpcr, fpsr, .. } => {
                assert_eq!(fpcr, 0x0040_0000);
                assert_eq!(fpsr, status); // Divide-by-zero and inexact.
                fpsr
            }
        };
        assert_eq!(values[0], 0x7f80_0000); // Positive infinity.
        // 1 + 2^-24 rounds upward to 1 + 2^-23, rather than nearest-even's 1.
        assert_eq!(values[2], 0x3f80_0001);
        assert_eq!(
            session.read_memory(symbol(&image, "status")?, 4)?,
            stored_status.to_le_bytes()
        );
        session.reset()?;
        assert_eq!(session.read_registers()?, initial);
    }
    Ok(())
}

#[test]
fn edited_vectors_drive_guest_arithmetic_and_stores_and_reset_cleanly() -> TestResult {
    for (target, count, prefix) in [(Target::X86_64, 16, "xmm"), (Target::Aarch64, 32, "v")] {
        let mut source = String::from(".text\n");
        if target == Target::Aarch64 {
            source.push_str("adr x0, output\n");
        }
        for index in 0..count {
            let offset = index * 16;
            match target {
                Target::X86_64 => writeln!(
                    source,
                    "paddb xmm{index}, xmm{index}\nmovdqu [rip + output + {offset}], xmm{index}"
                )?,
                Target::Aarch64 => writeln!(
                    source,
                    "add v{index}.16b, v{index}.16b, v{index}.16b\nstr q{index}, [x0, #{offset}]"
                )?,
            }
        }
        writeln!(source, "done: nop\n.data\noutput: .zero {}", count * 16)?;
        let image = linked(target, &source)?;
        let machine = Machine::load(Image::Elf(&image), target, MachineSetup::default())?;
        let mut session = Session::new(machine, symbol(&image, "done")?, 100)?;
        let initial = session.read_registers()?;
        let mut expected = Vec::new();
        for index in 0..count {
            let name = format!("{prefix}{index}");
            let mut bytes =
                (0x0123_4567_89ab_cdef_fedc_ba98_7654_3200_u128 + u128::from(index)).to_le_bytes();
            session.write_vector(&name, 128, 0, VectorBits::new(bytes.to_vec())?)?;
            // Each edit must merge into current storage, including overlapping lanes.
            for (width, lane, value) in [
                (8, 15, 255_u128),
                (16, 2, 0x8001),
                (32, 2, 0x7fc1_2345),
                (64, 0, 0xfedc_ba98_7654_3210),
            ] {
                session.write_vector(&name, width, lane, bits(value, width)?)?;
                let start = usize::from(width / 8) * usize::from(lane);
                bytes[start..start + usize::from(width / 8)]
                    .copy_from_slice(&value.to_le_bytes()[..usize::from(width / 8)]);
                assert_eq!(
                    vectors(&session.read_registers()?)[usize::try_from(index)?].to_le_bytes(),
                    bytes
                );
            }
            expected.extend(bytes.map(|byte| byte.wrapping_mul(2)));
        }
        let edited = session.read_registers()?;
        for (name, width, lane, value) in [
            (format!("{prefix}{count}"), 128, 0, 0),
            (format!("{prefix}0"), 32, 4, 0),
            (format!("{prefix}0"), 8, 0, 256),
        ] {
            assert!(
                session
                    .write_vector(&name, width, lane, bits(value, width)?)
                    .is_err()
            );
            assert_eq!(session.read_registers()?, edited);
        }
        assert_eq!(session.instructions(), 0);
        session.start()?;
        assert!(
            session
                .write_vector(&format!("{prefix}0"), 128, 0, bits(0, 128)?)
                .is_err()
        );
        session.pause()?;
        session.step()?;
        // A paused lane write is permitted and does not advance PC/counters.
        let paused = session.read_registers()?;
        session.write_vector(&format!("{prefix}{}", count - 1), 8, 15, bits(255, 8)?)?;
        assert_eq!(
            session.read_registers()?.instruction_pointer(),
            paused.instruction_pointer()
        );
        run(&mut session)?;
        assert_eq!(
            session.read_memory(symbol(&image, "output")?, count * 16)?,
            expected,
            "{target:?}"
        );
        assert!(
            session
                .write_vector(&format!("{prefix}0"), 128, 0, bits(0, 128)?)
                .is_err()
        );
        session.reset()?;
        assert_eq!(session.read_registers()?, initial);
        assert_eq!(
            session.read_memory(symbol(&image, "output")?, count * 16)?,
            vec![0; usize::try_from(count * 16)?]
        );
    }
    Ok(())
}

#[test]
fn live_rounding_changes_both_signs_without_overwriting_other_controls() -> TestResult {
    use oplab_core::registers::RoundingMode;
    for (target, source, prefix_steps, vector) in [
        (
            Target::X86_64,
            "ldmxcsr [rip + control]\naddps xmm0, xmm1\nmovdqu [rip + output], xmm0\ndone: nop\n.data\ncontrol: .long 0x9fc0\noutput: .zero 16",
            1,
            "xmm",
        ),
        (
            Target::Aarch64,
            "mov x0, #0x3000000\nmsr fpcr, x0\nfadd s0, s0, s1\nfadd s2, s2, s3\nadr x0, output\nstr s0, [x0]\nstr s2, [x0, #4]\ndone: nop\n.data\noutput: .zero 16",
            2,
            "v",
        ),
    ] {
        let image = linked(target, source)?;
        for mode in [
            RoundingMode::NearestEven,
            RoundingMode::Down,
            RoundingMode::Up,
            RoundingMode::TowardZero,
        ] {
            let machine = Machine::load(Image::Elf(&image), target, MachineSetup::default())?;
            let mut session = Session::new(machine, symbol(&image, "done")?, 100)?;
            let initial = session.read_registers()?;
            for _ in 0..prefix_steps {
                session.step()?;
            }
            session.set_rounding(mode)?;
            let (actual, expected) = match session.read_registers()? {
                MachineRegisters::X86_64 { mxcsr, .. } => (
                    mxcsr,
                    0x9fc0
                        | match mode {
                            RoundingMode::NearestEven => 0,
                            RoundingMode::Down => 0x2000,
                            RoundingMode::Up => 0x4000,
                            RoundingMode::TowardZero => 0x6000,
                        },
                ),
                MachineRegisters::Aarch64 { fpcr, .. } => (
                    fpcr,
                    0x0300_0000
                        | match mode {
                            RoundingMode::NearestEven => 0,
                            RoundingMode::Down => 0x0080_0000,
                            RoundingMode::Up => 0x0040_0000,
                            RoundingMode::TowardZero => 0x00c0_0000,
                        },
                ),
            };
            assert_eq!(actual, expected);
            session.write_vector(
                &format!("{vector}0"),
                128,
                0,
                bits(0xbf80_0000_3f80_0000_bf80_0000_3f80_0000, 128)?,
            )?;
            session.write_vector(
                &format!("{vector}1"),
                128,
                0,
                bits(0xb380_0000_3380_0000_b380_0000_3380_0000, 128)?,
            )?;
            if target == Target::Aarch64 {
                session.write_vector("v2", 128, 0, bits(0xbf80_0000, 128)?)?;
                session.write_vector("v3", 128, 0, bits(0xb380_0000, 128)?)?;
            }
            run(&mut session)?;
            let expected = rounding_results(target, mode).map(u32::to_le_bytes);
            assert_eq!(
                session.read_memory(symbol(&image, "output")?, 16)?,
                expected.as_flattened(),
                "{target:?}, {mode:?}"
            );
            assert!(session.set_rounding(mode).is_err());
            session.reset()?;
            assert_eq!(session.read_registers()?, initial);
        }
    }
    Ok(())
}

fn rounding_results(target: Target, mode: oplab_core::registers::RoundingMode) -> [u32; 4] {
    use oplab_core::registers::RoundingMode;
    let positive = if mode == RoundingMode::Up {
        0x3f80_0001
    } else {
        0x3f80_0000
    };
    let negative = if mode == RoundingMode::Down {
        0xbf80_0001
    } else {
        0xbf80_0000
    };
    if target == Target::X86_64 {
        [positive, negative, positive, negative]
    } else {
        [positive, negative, 0, 0]
    }
}

fn wide_packed_add(values: [u32; 8]) -> TestResult {
    let expected: Vec<_> = values
        .into_iter()
        .flat_map(|value| value.wrapping_add(value).to_le_bytes())
        .collect();
    for (target, code) in [
        (
            Target::X86_64,
            "vmovdqu ymm0, [rip + input]\nvpaddd ymm0, ymm0, ymm0\nvmovdqu [rip + output], ymm0",
        ),
        (
            Target::Aarch64,
            ".arch armv9-a+sve2\nadr x0, input\nadr x1, output\nmov x2, #8\nwhilelo p0.s, xzr, x2\nld1w {z0.s}, p0/z, [x0]\nadd z0.s, p0/m, z0.s, z0.s\nst1w {z0.s}, p0, [x1]",
        ),
    ] {
        let mut source = format!(".text\n{code}\ndone: nop\n.data\ninput:\n");
        for value in values {
            writeln!(source, ".long {value}")?;
        }
        source.push_str("output: .zero 32\n");
        let image = linked(target, &source)?;
        let mut session = Session::from_elf(&image, target, symbol(&image, "done")?, 100)?;
        run(&mut session)?;
        assert_eq!(
            session.read_memory(symbol(&image, "output")?, 32)?,
            expected
        );
    }
    Ok(())
}

#[test]
fn full_width_vectors_and_predicates_survive_edits_guest_stores_and_reset() -> TestResult {
    for (target, size, register, alias, source) in [
        (
            Target::X86_64,
            32,
            "ymm15",
            "xmm15",
            "vmovdqu [rip + output], ymm15\ndone: nop\n.data\noutput: .zero 32",
        ),
        (
            Target::Aarch64,
            256,
            "z31",
            "v31",
            ".arch armv9-a+sve2\nadr x0, output\nstr z31, [x0]\nadd x0, x0, #256\nstr p15, [x0]\nrdffr p14.b\nadd x0, x0, #32\nstr p14, [x0]\ndone: nop\n.data\noutput: .zero 320",
        ),
    ] {
        let image = linked(target, source)?;
        let machine = Machine::load(Image::Elf(&image), target, MachineSetup::default())?;
        let mut session = Session::new(machine, symbol(&image, "done")?, 100)?;
        let initial = session.read_registers()?;
        let mut expected: Vec<u8> = (0..size)
            .map(|index| u8::try_from(index).map(|byte| byte ^ 0xa5))
            .collect::<Result<_, _>>()?;
        session.write_vector(
            register,
            u16::try_from(size * 8)?,
            0,
            VectorBits::new(expected.clone())?,
        )?;
        session.write_vector(alias, 32, 3, bits(0x7fc1_2345, 32)?)?;
        expected[12..16].copy_from_slice(&0x7fc1_2345_u32.to_le_bytes());
        session.write_vector(register, 8, u16::try_from(size - 1)?, bits(0xfe, 8)?)?;
        expected[size - 1] = 0xfe;
        let predicates = [0xa5; 32];
        let mut ffr = [0xff; 32];
        ffr[31] = 0x7f;
        if target == Target::Aarch64 {
            session.write_vector("p15", 256, 0, VectorBits::new(predicates.to_vec())?)?;
            session.write_vector("ffr", 256, 0, VectorBits::new(vec![0xff; 32])?)?;
            session.write_vector("ffr", 1, 255, VectorBits::new(vec![0])?)?;
        }
        let edited = session.read_registers()?;
        match &edited {
            MachineRegisters::X86_64 { ymm, .. } => assert_eq!(ymm[15].as_le_bytes(), expected),
            MachineRegisters::Aarch64 {
                z,
                p,
                ffr: observed,
                vl,
                max_vl,
                ..
            } => {
                assert_eq!((*vl, *max_vl), (256, 256));
                assert_eq!(z[31].as_le_bytes(), expected);
                assert_eq!(p[15].as_le_bytes(), predicates);
                assert_eq!(observed.as_le_bytes(), ffr);
            }
        }
        assert!(
            session
                .write_vector(register, 8, u16::try_from(size)?, bits(1, 8)?)
                .is_err()
        );
        assert_eq!(session.read_registers()?, edited);
        run(&mut session)?;
        if target == Target::Aarch64 {
            expected.extend(predicates);
            expected.extend(ffr);
        }
        assert_eq!(
            session.read_memory(symbol(&image, "output")?, u64::try_from(expected.len())?)?,
            expected
        );
        session.reset()?;
        assert_eq!(session.read_registers()?, initial);
    }
    Ok(())
}
