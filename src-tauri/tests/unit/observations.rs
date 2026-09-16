use super::*;
use oplab_core::{
    address::Address,
    protocol::{
        execution::{Registers, SessionKey},
        scalar::{Counter, HexAddress, VectorBits},
        stream::ObservationDelta,
    },
    target::CpuModel,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn delivered_snapshots_preserve_metadata_replace_banks_and_clear_crashes(
        samples in prop::collection::vec((
            prop::collection::btree_set(any::<u64>(), 0..16),
            any::<bool>(), any::<[u128; 16]>(), any::<u32>(),
        ), 1..16),
    ) {
        let mut cache = Cache::default();
        let mut sequence = 0;
        for (set, nehalem, vectors, control) in samples {
            sequence += 1;
            let mut expected = Box::new(Observation {
                cpu: if nehalem { CpuModel::Nehalem } else { CpuModel::Haswell },
                key: SessionKey { session: Counter::new(2), generation: Counter::new(0) },
                sequence: Counter::new(sequence),
                status: Status::Ready,
                instructions: Counter::new(0),
                dispatches: Counter::new(0),
                registers: Some(Registers::X86_64 {
                    gpr: [Counter::new(0); 16],
                    rip: HexAddress::new(Address::new(0x1000)),
                    rflags: Counter::new(2),
                    xmm: Box::new([VectorBits::new(0); 16]),
                    mxcsr: 0x1f80,
                }),
                fault: None,
                breakpoints: set.into_iter().map(|value| HexAddress::new(Address::new(value))).collect(),
                memory: None,
            });
            cache.apply(StreamMessage {
                event: StreamEvent { subscription: Counter::new(3), update: ObservationUpdate::Full(expected.clone()) },
                memory: None,
            }).map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
            let mut replacement = expected.registers.clone();
            let Some(Registers::X86_64 { xmm, mxcsr, .. }) = &mut replacement else {
                return Err(TestCaseError::fail("missing register bank"));
            };
            **xmm = vectors.map(VectorBits::new);
            *mxcsr = control;
            for update in [
                RegisterUpdate::Replace(replacement.map(Box::new)),
                RegisterUpdate::Unchanged,
                RegisterUpdate::Replace(None),
            ] {
                let base = expected.sequence;
                sequence += 1;
                expected.sequence = Counter::new(sequence);
                if let RegisterUpdate::Replace(bank) = &update {
                    expected.registers = bank.as_deref().cloned();
                    if bank.is_none() { expected.status = Status::Crashed; }
                }
                let delivered = cache.apply(StreamMessage {
                    event: StreamEvent {
                        subscription: Counter::new(3),
                        update: ObservationUpdate::Delta(Box::new(ObservationDelta {
                            key: expected.key, base, sequence: expected.sequence,
                            status: expected.status, instructions: expected.instructions,
                            dispatches: expected.dispatches, fault: None,
                            registers: update, memory_bytes: 0,
                        })),
                    },
                    memory: None,
                }).map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
                prop_assert_eq!(delivered.event.update, ObservationUpdate::Full(expected.clone()));
            }
        }
    }
}
