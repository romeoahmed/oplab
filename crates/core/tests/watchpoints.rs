//! Validation at address-space and payload boundaries, before native allocation.
use oplab_core::{
    address::Address,
    watchpoint::{WatchAccess, Watchpoint},
};
use proptest::prelude::*;

#[test]
fn single_last_byte_and_maximum_window_are_valid_but_wrap_and_empty_are_not() {
    assert!(Watchpoint::new(Address::new(u64::MAX), 1, WatchAccess::ReadWrite).is_ok());
    assert!(Watchpoint::new(Address::new(u64::MAX - 65535), 65536, WatchAccess::Write).is_ok());
    for (address, length) in [(0, 0), (0, 65537), (u64::MAX, 2)] {
        assert!(Watchpoint::new(Address::new(address), length, WatchAccess::Read).is_err());
    }
}

proptest! {
    #[test]
    fn ranges_accept_exactly_nonempty_bounded_nonwrapping_input(
        address in prop_oneof![any::<u64>(), (0_u64..65536).prop_map(|tail| u64::MAX - tail)],
        length in 0_u32..70000,
    ) {
        let valid = (1..=65536).contains(&length) && u128::from(address) + u128::from(length) <= 1_u128 << 64;
        for access in [WatchAccess::Read, WatchAccess::Write, WatchAccess::ReadWrite] {
            prop_assert_eq!(Watchpoint::new(Address::new(address), length, access).is_ok(), valid);
        }
    }
}
