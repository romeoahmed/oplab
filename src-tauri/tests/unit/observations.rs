use super::*;
use oplab_core::{
    address::Address,
    protocol::{
        execution::{Registers, SessionKey},
        scalar::{Counter, HexAddress},
        stream::ObservationDelta,
    },
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn delivered_snapshots_retain_breakpoints_until_the_next_full_baseline(
        sets in prop::collection::vec(prop::collection::btree_set(any::<u64>(), 0..16), 1..16),
    ) {
        let mut cache = Cache::default();
        let mut sequence = 0;
        for set in sets {
            sequence += 1;
            let mut expected = Box::new(Observation {
                key: SessionKey { session: Counter::new(2), generation: Counter::new(0) },
                sequence: Counter::new(sequence),
                status: Status::Ready,
                instructions: Counter::new(0),
                dispatches: Counter::new(0),
                registers: Some(Registers::X86_64 {
                    gpr: [Counter::new(0); 16],
                    rip: HexAddress::new(Address::new(0x1000)),
                    rflags: Counter::new(2),
                }),
                fault: None,
                breakpoints: set.into_iter().map(|value| HexAddress::new(Address::new(value))).collect(),
                memory: None,
            });
            cache.apply(StreamMessage {
                event: StreamEvent { subscription: Counter::new(3), update: ObservationUpdate::Full(expected.clone()) },
                memory: None,
            }).map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
            for _ in 0..2 {
                let base = expected.sequence;
                sequence += 1;
                expected.sequence = Counter::new(sequence);
                let delivered = cache.apply(StreamMessage {
                    event: StreamEvent {
                        subscription: Counter::new(3),
                        update: ObservationUpdate::Delta(Box::new(ObservationDelta {
                            key: expected.key, base, sequence: expected.sequence,
                            status: expected.status, instructions: expected.instructions,
                            dispatches: expected.dispatches, fault: None,
                            registers: RegisterUpdate::Unchanged, memory_bytes: 0,
                        })),
                    },
                    memory: None,
                }).map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
                prop_assert_eq!(delivered.event.update, ObservationUpdate::Full(expected.clone()));
            }
        }
    }
}
