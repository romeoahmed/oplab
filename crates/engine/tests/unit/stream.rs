use super::super::{
    outbox,
    transport::{self, Output},
};
use super::*;
use oplab_core::{
    address::Address,
    protocol::{
        execution::Registers,
        scalar::{HexAddress, VectorBits},
    },
    target::CpuModel,
};
use proptest::prelude::*;

fn capture(sequence: u64) -> Capture {
    Capture {
        subscription: Counter::new(3),
        observation: Box::new(Observation {
            cpu: CpuModel::Haswell,
            key: SessionKey {
                session: Counter::new(2),
                generation: Counter::new(0),
            },
            sequence: Counter::new(sequence),
            status: Status::Running,
            instructions: Counter::new(sequence),
            dispatches: Counter::new(sequence),
            registers: Some(Registers::X86_64 {
                gpr: [Counter::new(0); 16],
                rip: HexAddress::new(Address::new(0x1000)),
                rflags: Counter::new(2),
                xmm: Box::new([VectorBits::new(0); 16]),
                mxcsr: 0x1f80,
            }),
            fault: None,
            breakpoints: Vec::new(),
            memory: Some(MemoryWindow {
                address: HexAddress::new(Address::new(0x2000)),
                length: 8,
            }),
        }),
        memory: Some(sequence.to_le_bytes().to_vec()),
    }
}

#[test]
fn coalesced_samples_reference_only_delivered_bases_and_keep_binary_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    let mut encoder = Encoder::default();
    let mut bytes = Vec::new();
    encoder.write(&mut bytes, Pending::Capture(capture(1)))?;
    let (sender, receiver) = outbox::channel();
    for sequence in 2..=4 {
        sender.observe(Some(Pending::Capture(capture(sequence))))?;
    }
    drop(sender);
    let Some(outbox::Delivery::Observation(pending)) = receiver.receive()? else {
        return Err("missing sample".into());
    };
    encoder.write(&mut bytes, pending)?;
    assert!(receiver.receive()?.is_none());
    let mut unchanged = capture(5);
    unchanged.memory = Some(4_u64.to_le_bytes().to_vec());
    encoder.write(&mut bytes, Pending::Capture(unchanged))?;
    let mut replacement = capture(6);
    replacement.subscription = Counter::new(9);
    encoder.write(&mut bytes, Pending::Capture(replacement))?;
    let mut input = bytes.as_slice();
    let Some(Output::Observation(full)) = transport::read_output(&mut input)? else {
        return Err("missing full".into());
    };
    assert!(matches!(full.event.update, ObservationUpdate::Full(_)));
    for (base, sequence, changed) in [(1, 4, true), (4, 5, false)] {
        let Some(Output::Observation(message)) = transport::read_output(&mut input)? else {
            return Err("missing delta".into());
        };
        let ObservationUpdate::Delta(delta) = message.event.update else {
            return Err("expected delta".into());
        };
        assert_eq!(delta.base.get(), base);
        assert_eq!(delta.sequence.get(), sequence);
        assert_eq!(delta.registers, RegisterUpdate::Unchanged);
        assert_eq!(delta.memory_bytes, if changed { 8 } else { 0 });
        assert_eq!(
            message.memory,
            changed.then(|| 4_u64.to_le_bytes().to_vec())
        );
    }
    let Some(Output::Observation(full)) = transport::read_output(&mut input)? else {
        return Err("missing replacement".into());
    };
    assert!(matches!(full.event.update, ObservationUpdate::Full(_)));
    assert!(input.is_empty());
    let mut broken = bytes.as_slice();
    // A partial final binary window never becomes an observable full event.
    broken = &broken[..broken.len() - 1];
    for _ in 0..3 {
        transport::read_output(&mut broken)?;
    }
    assert!(transport::read_output(&mut broken).is_err());
    Ok(())
}

proptest! {
    #[test]
    fn complete_samples_survive_stream_encoding_and_metadata_changes(
        samples in prop::collection::vec((
            prop::collection::btree_set(any::<u64>(), 0..16),
            any::<bool>(), any::<[u128; 16]>(), any::<u32>(),
        ), 1..16),
    ) {
        let mut encoder = Encoder::default();
        let mut sequence = 0;
        let mut retained: Option<Box<Observation>> = None;
        // Metadata changes, a bank-only change, then inheritance.
        for ((breakpoints, nehalem, vectors, control), replace) in samples.iter()
            .flat_map(|sample| [(sample, false), (sample, true), (sample, true)]) {
            sequence += 1;
            let mut sample = capture(sequence);
            sample.observation.cpu = if *nehalem { CpuModel::Nehalem } else { CpuModel::Haswell };
            sample.observation.breakpoints = breakpoints.iter()
                .map(|&value| HexAddress::new(Address::new(value))).collect();
            let Some(Registers::X86_64 { xmm, mxcsr, .. }) = &mut sample.observation.registers else {
                return Err(TestCaseError::fail("missing register bank"));
            };
            if replace {
                **xmm = vectors.map(VectorBits::new);
                *mxcsr = *control;
            }
            let expected = sample.observation.clone();
            let expected_memory = sample.memory.clone();
            let mut wire = Vec::new();
            encoder.write(&mut wire, Pending::Capture(sample))?;
            let mut input = wire.as_slice();
            let Some(Output::Observation(message)) = transport::read_output(&mut input)? else {
                return Err(TestCaseError::fail("missing observation"));
            };
            match message.event.update {
                ObservationUpdate::Full(observation) => retained = Some(observation),
                ObservationUpdate::Delta(delta) => {
                    let Some(base) = &mut retained else {
                        return Err(TestCaseError::fail("missing baseline"));
                    };
                    prop_assert_eq!(delta.base, base.sequence);
                    base.sequence = delta.sequence;
                    base.status = delta.status;
                    base.instructions = delta.instructions;
                    base.dispatches = delta.dispatches;
                    base.fault = delta.fault;
                    if let RegisterUpdate::Replace(registers) = delta.registers {
                        base.registers = registers.map(|bank| *bank);
                    }
                }
                ObservationUpdate::Ended(_) => return Err(TestCaseError::fail("unexpected end")),
            }
            prop_assert_eq!(&retained, &Some(expected));
            prop_assert_eq!(message.memory, expected_memory);
            prop_assert!(input.is_empty());
        }
    }
}
