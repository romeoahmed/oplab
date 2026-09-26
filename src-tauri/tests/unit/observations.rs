use super::*;
use oplab_core::registers::VectorBits;
use oplab_core::{
    address::Address,
    protocol::{
        execution::{DataWatchpoint, MemoryWindow, Registers, SessionKey},
        scalar::{Counter, HexAddress},
        stream::ObservationDelta,
    },
    target::Target,
    watchpoint::WatchAccess,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn delivered_snapshots_preserve_metadata_replace_banks_and_clear_crashes(
        target in prop::sample::select(vec![Target::X86_64, Target::Aarch64]),
        samples in prop::collection::vec((
            prop::collection::btree_set(any::<u64>(), 0..16),
            prop::collection::btree_set(any::<u64>(), 0..8),
            any::<[[u8; 256]; 32]>(), any::<[[u8; 32]; 17]>(), 1_u16..=16, any::<u32>(),
            prop::collection::vec(any::<u8>(), 1..=256),
        ), 1..16),
    ) {
        let mut cache = Cache::default();
        let mut sequence = 0;
        for (set, watches, vectors, predicates, vq, control, bytes) in samples {
            sequence += 1;
            let mut expected = sample(target, sequence);
            expected.breakpoints = set.into_iter().map(|value| HexAddress::new(Address::new(value))).collect();
            expected.watchpoints = watches.into_iter().map(|value| DataWatchpoint {
                address: HexAddress::new(Address::new(value)), length: 1, access: WatchAccess::ReadWrite,
            }).collect();
            let length = u32::try_from(bytes.len()).map_err(|error| TestCaseError::fail(error.to_string()))?;
            expected.memory = Some(MemoryWindow {
                address: HexAddress::new(Address::new(0x2000)), length,
            });
            let initial = cache.apply(StreamMessage {
                event: StreamEvent { subscription: Counter::new(3), update: ObservationUpdate::Full(expected.clone()) },
                memory: Some(bytes.clone()),
            }).map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
            let initial_observation = expected.clone();
            let replaced_bytes: Vec<_> = bytes.iter().map(|byte| !byte).collect();
            let mut expected_memory = bytes.clone();
            let mut replacement = expected.registers.clone();
            match &mut replacement {
                Some(Registers::X86_64 { ymm, mxcsr, .. }) => {
                    **ymm = std::array::from_fn(|index| vector(&vectors[index][..32]));
                    *mxcsr = control;
                }
                Some(Registers::Aarch64 { z, p, ffr, vl, fpcr, fpsr, .. }) => {
                    **z = vectors.map(|bytes| vector(&bytes));
                    **p = std::array::from_fn(|index| vector(&predicates[index]));
                    *ffr = vector(&predicates[16]);
                    *vl = vq * 16;
                    *fpcr = control;
                    *fpsr = control;
                }
                None => return Err(TestCaseError::fail("missing register bank")),
            }
            for (update, memory) in [
                (RegisterUpdate::Replace(replacement.map(Box::new)), Some(replaced_bytes)),
                (RegisterUpdate::Unchanged, None),
                (RegisterUpdate::Replace(None), None),
            ] {
                let base = expected.sequence;
                sequence += 1;
                expected.sequence = Counter::new(sequence);
                if let RegisterUpdate::Replace(bank) = &update {
                    expected.registers = bank.as_deref().cloned();
                    if bank.is_none() { expected.status = Status::Crashed; }
                }
                let memory_bytes = memory.as_ref().map_or(0, |bytes| {
                    expected_memory = bytes.clone();
                    length
                });
                let delivered = cache.apply(StreamMessage {
                    event: StreamEvent {
                        subscription: Counter::new(3),
                        update: ObservationUpdate::Delta(Box::new(ObservationDelta {
                            key: expected.key, base, sequence: expected.sequence,
                            status: expected.status, instructions: expected.instructions,
                            dispatches: expected.dispatches, fault: None,
                            registers: update, memory_bytes,
                        })),
                    },
                    memory,
                }).map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
                prop_assert_eq!(&delivered.event.update, &ObservationUpdate::Full(expected.clone()));
                prop_assert_eq!(delivered.memory.as_ref(), Some(&expected_memory));
                prop_assert_eq!(&initial.event.update, &ObservationUpdate::Full(initial_observation.clone()));
                prop_assert_eq!(initial.memory.as_ref(), Some(&bytes));
            }
        }
    }
}

fn sample(target: Target, sequence: u64) -> Box<Observation> {
    Box::new(Observation {
        key: SessionKey {
            session: Counter::new(2),
            generation: Counter::new(0),
        },
        sequence: Counter::new(sequence),
        status: Status::Ready,
        instructions: Counter::new(1),
        dispatches: Counter::new(2),
        registers: Some(match target {
            Target::X86_64 => Registers::X86_64 {
                gpr: [Counter::new(0); 16],
                rip: HexAddress::new(Address::new(0x1000)),
                rflags: Counter::new(2),
                ymm: Box::new(std::array::from_fn(|_| vector(&[0; 32]))),
                mxcsr: 0x1f80,
            },
            Target::Aarch64 => Registers::Aarch64 {
                x: [Counter::new(0); 31],
                sp: Counter::new(0),
                pc: HexAddress::new(Address::new(0x1000)),
                nzcv: 0,
                z: Box::new(std::array::from_fn(|_| vector(&[0; 256]))),
                p: Box::new(std::array::from_fn(|_| vector(&[0; 32]))),
                ffr: vector(&[0; 32]),
                vl: 256,
                max_vl: 256,
                fpcr: 0,
                fpsr: 0,
            },
        }),
        fault: None,
        breakpoints: Vec::new(),
        watchpoints: Vec::new(),
        memory: None,
    })
}

#[test]
fn rejected_deltas_preserve_the_last_valid_baseline() {
    for target in [Target::X86_64, Target::Aarch64] {
        let mut initial = sample(target, 7);
        initial.memory = Some(MemoryWindow {
            address: HexAddress::new(Address::new(0x2000)),
            length: 4,
        });
        let full = || StreamMessage {
            event: StreamEvent {
                subscription: Counter::new(3),
                update: ObservationUpdate::Full(initial.clone()),
            },
            memory: Some(vec![1, 2, 3, 4]),
        };
        let delta = ObservationDelta {
            key: initial.key,
            base: initial.sequence,
            sequence: Counter::new(9),
            status: initial.status,
            instructions: initial.instructions,
            dispatches: initial.dispatches,
            fault: None,
            registers: RegisterUpdate::Unchanged,
            memory_bytes: 0,
        };
        let packet = |delta| StreamMessage {
            event: StreamEvent {
                subscription: Counter::new(3),
                update: ObservationUpdate::Delta(Box::new(delta)),
            },
            memory: None,
        };
        let mut expected = full();
        let ObservationUpdate::Full(value) = &mut expected.event.update else {
            unreachable!()
        };
        value.sequence = delta.sequence;
        let wrong_key = SessionKey {
            generation: Counter::new(1),
            ..delta.key
        };
        let mut wrong_subscription = packet(delta.clone());
        wrong_subscription.event.subscription = Counter::new(4);
        for rejected in [
            wrong_subscription,
            packet(ObservationDelta {
                key: wrong_key,
                ..delta.clone()
            }),
            packet(ObservationDelta {
                base: Counter::new(6),
                ..delta.clone()
            }),
            packet(ObservationDelta {
                sequence: delta.base,
                ..delta.clone()
            }),
            packet(ObservationDelta {
                instructions: Counter::new(0),
                ..delta.clone()
            }),
            packet(ObservationDelta {
                dispatches: Counter::new(1),
                ..delta.clone()
            }),
            packet(ObservationDelta {
                memory_bytes: 3,
                ..delta.clone()
            }),
            packet(ObservationDelta {
                memory_bytes: 4,
                ..delta.clone()
            }),
            packet(ObservationDelta {
                status: Status::Crashed,
                ..delta.clone()
            }),
            packet(ObservationDelta {
                registers: RegisterUpdate::Replace(None),
                ..delta.clone()
            }),
        ] {
            let mut cache = Cache::default();
            assert!(cache.apply(full()).is_ok());
            assert!(cache.apply(rejected).is_err());
            // A subsequent valid delta still refers to the original baseline.
            assert_eq!(
                cache
                    .apply(packet(delta.clone()))
                    .map(|message| (message.event.clone(), message.memory.clone())),
                Ok((expected.event.clone(), expected.memory.clone()))
            );
        }
    }
}

fn vector(bytes: &[u8]) -> VectorBits {
    match VectorBits::new(bytes.to_vec()) {
        Ok(value) => value,
        Err(error) => panic!("invalid register fixture: {error}"),
    }
}
