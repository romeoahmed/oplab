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
        let canonical = format!("0x{value:016x}");
        prop_assert_eq!(&address.to_string(), &canonical);
        prop_assert_eq!(canonical.parse::<Address>(), Ok(address));
    }

    #[test]
    fn range_validation_matches_wide_arithmetic(
        start in any::<u64>(), length in any::<u64>(), limit in any::<u64>(), probe in any::<u64>(),
    ) {
        let expected_end = u128::from(start) + u128::from(length);
        let range = AddressRange::new(Address::new(start), length, limit);
        if length == 0 || length > limit {
            prop_assert_eq!(range, Err(ValidationError::Length));
        } else if expected_end <= 1_u128 << 64 {
            let range = range?;
            prop_assert_eq!(range.end(), expected_end);
            prop_assert!(range.contains(Address::new(start)));
            prop_assert!(range.contains(Address::new(u64::try_from(expected_end - 1)?)));
            prop_assert_eq!(range.contains(Address::new(probe)), probe >= start && u128::from(probe) < expected_end);
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

const fn permissions(bits: u8) -> Permissions {
    Permissions {
        read: bits & 4 != 0,
        write: bits & 2 != 0,
        execute: bits & 1 != 0,
    }
}

proptest! {
    #[test]
    fn mappings_admit_only_complete_ranges_with_every_requested_permission(
        page in 0_u64..1024,
        granted in 0_u8..8,
        requested in 0_u8..8,
        offset in 0_u64..8192,
        length in 1_u64..8192,
    ) {
        let start = page * 4096;
        let range = AddressRange::new(Address::new(start), 4096, 4096)?;
        let region = MemoryRegion::new(range, permissions(granted), vec![42], 4096)?;
        prop_assert_eq!(MemoryLayout::new(vec![region.clone(), region.clone()]), Err(ValidationError::Overlap));
        let adjacent = MemoryRegion::new(AddressRange::new(Address::new(start + 4096), 4096, 4096)?, permissions(granted), Vec::new(), 4096)?;
        let layout = MemoryLayout::new(vec![adjacent, region])?;
        let query = AddressRange::new(Address::new(start + offset), length, 8192)?;
        // Access must fit one mapping: adjacency does not authorize a crossing.
        let fits = offset / 4096 == (offset + length - 1) / 4096 && offset + length <= 8192;
        prop_assert_eq!(layout.require(query, permissions(requested)).is_ok(), fits && requested & granted == requested);
    }
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
