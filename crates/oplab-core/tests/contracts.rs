//! Semantic boundaries shared by every engine and UI entry point.

use oplab_core::{
    address::{Address, AddressRange},
    diagnostic::ValidationError,
    execution::{ControlEvent, ExecutionState, PauseReason, Termination},
    memory::{MemoryLayout, MemoryRegion, Permissions},
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn hexadecimal_addresses_preserve_all_bits(value in any::<u64>()) {
        let address = Address::new(value);
        prop_assert_eq!(address.to_string().parse::<Address>(), Ok(address));
    }

    #[test]
    fn range_validation_matches_wide_arithmetic(start in any::<u64>(), length in 1_u64..=u64::MAX) {
        let expected_end = u128::from(start) + u128::from(length);
        let range = AddressRange::new(Address::new(start), length, u64::MAX);
        if expected_end <= 1_u128 << 64 {
            let range = range?;
            prop_assert_eq!(range.end(), expected_end);
            prop_assert!(range.contains(Address::new(start)));
            prop_assert!(range.contains(Address::new(u64::try_from(expected_end - 1)?)));
        } else {
            prop_assert_eq!(range, Err(ValidationError::AddressOverflow));
        }
    }
}

#[test]
fn final_address_is_a_valid_byte_but_cannot_advance() -> Result<(), ValidationError> {
    let last = Address::new(u64::MAX);
    let range = AddressRange::new(last, 1, 1)?;
    assert_eq!(range.end(), 1_u128 << 64);
    assert_eq!(last.checked_add(1), Err(ValidationError::AddressOverflow));
    for invalid in [
        "1",
        "0x",
        "0x10000000000000000",
        "0x+1",
        "0x000000000000000g",
    ] {
        assert_eq!(
            invalid.parse::<Address>(),
            Err(ValidationError::AddressFormat)
        );
    }
    for valid in ["0xff", "0XFF", "0x00000000000000FF"] {
        assert_eq!(valid.parse::<Address>(), Ok(Address::new(255)));
    }
    Ok(())
}

#[test]
fn maps_preserve_permissions_and_reject_overlap() -> Result<(), ValidationError> {
    let read = Permissions {
        read: true,
        write: false,
        execute: false,
    };
    let write = Permissions {
        read: false,
        write: true,
        execute: false,
    };
    let range = AddressRange::new(Address::new(0x1000), 4096, 4096)?;
    let region = MemoryRegion::new(range, read, vec![42], 4096)?;
    assert_eq!(
        MemoryLayout::new(vec![region.clone(), region.clone()]),
        Err(ValidationError::Overlap)
    );
    let layout = MemoryLayout::new(vec![region])?;
    assert_eq!(layout.require(range, read)?.initial(), &[42]);
    assert_eq!(
        layout.require(range, write),
        Err(ValidationError::Permission)
    );
    let missing = AddressRange::new(Address::new(0x2000), 1, 1)?;
    assert_eq!(
        layout.require(missing, read),
        Err(ValidationError::Permission)
    );
    Ok(())
}

#[test]
fn faults_are_terminal_and_running_mutations_require_explicit_pause() -> Result<(), ValidationError>
{
    let running = ExecutionState::Ready.transition(ControlEvent::Start)?;
    assert_eq!(
        running.require_patchable(),
        Err(ValidationError::Transition)
    );
    assert_eq!(
        running.transition(ControlEvent::Reset),
        Err(ValidationError::Transition)
    );
    let paused = running.transition(ControlEvent::Pause(PauseReason::Requested))?;
    paused.require_patchable()?;
    let ended = paused.transition(ControlEvent::Terminate(Termination::GuestFault))?;
    assert_eq!(
        ended.transition(ControlEvent::Start),
        Err(ValidationError::Transition)
    );
    assert_eq!(
        ended.transition(ControlEvent::Reset)?,
        ExecutionState::Ready
    );
    assert_eq!(
        ExecutionState::Crashed.transition(ControlEvent::Reset),
        Err(ValidationError::Transition)
    );
    Ok(())
}
