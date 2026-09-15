//! Interpret native returns conservatively: a clean return alone is not completion.

use super::{Machine, MachineError, hooks::Stop};
use oplab_core::{
    address::Address,
    execution::{Access, FaultKind, GuestFault},
};
use unicorn_engine::unicorn_const::uc_error;

/// Native work per ownership slice, independent of the instruction-start budget.
pub(crate) const SLICE_DISPATCHES: u64 = 1024;

#[derive(Debug, Clone, Copy)]
pub(crate) enum SliceStop {
    Yield,
    Completed,
    Budget,
    Breakpoint(Address),
    Fault(GuestFault),
    Environment,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Slice {
    pub dispatches: u64,
    pub instructions: u64,
    pub repeated_instruction_pending: bool,
    pub stop: SliceStop,
}

impl Machine {
    pub(crate) fn run_slice(
        &mut self,
        completion: Address,
        dispatches: u64,
        instruction_limit: u64,
        bypass: Option<Address>,
    ) -> Result<Slice, MachineError> {
        let count = usize::try_from(dispatches).map_err(|_| MachineError::Backend)?;
        if count == 0 || dispatches > SLICE_DISPATCHES {
            return Err(MachineError::Backend);
        }
        let before = self.native.pc_read().map_err(|_| MachineError::Backend)?;
        if before == completion.get() {
            return Ok(Slice {
                dispatches: 0,
                instructions: 0,
                repeated_instruction_pending: false,
                stop: SliceStop::Completed,
            });
        }
        self.configure_exits(before, completion)?;
        self.native
            .get_data_mut()
            .prepare(instruction_limit, bypass);
        // Yield from the code hook before admitting an instruction. Unicorn's
        // asynchronous timer can stop after a pre-execution hook but before effects,
        // making instruction accounting and exact single-step completion ambiguous.
        let result = self.native.emu_start(before, completion.get(), 0, count);
        self.native.get_data_mut().active = false;
        let pc = Address::new(self.native.pc_read().map_err(|_| MachineError::Backend)?);
        if self
            .native
            .get_data()
            .pending_repeat
            .is_some_and(|(address, _, _)| address != pc)
        {
            self.native.get_data_mut().pending_repeat = None;
        }
        let monitor = self.native.get_data();
        if monitor.failed
            || monitor.dispatches > dispatches
            || monitor.instructions > instruction_limit
        {
            return Err(MachineError::Backend);
        }
        // Infrastructure errors invalidate the owner even if a hook also requested
        // a stop. A breakpoint must not hide a native fault during translation.
        let native_fault = result.err().map(fault).transpose()?;
        let exhausted_before_fetch = monitor.dispatches == dispatches
            && monitor
                .last_instruction
                .is_some_and(|address| address != pc)
            && matches!(
                monitor.stop,
                None | Some(Stop::Fault {
                    kind: FaultKind::Unmapped(Access::Fetch) | FaultKind::Protection(Access::Fetch),
                    ..
                })
            )
            && matches!(
                native_fault,
                Some(FaultKind::Unmapped(Access::Fetch) | FaultKind::Protection(Access::Fetch))
            );
        // The native counter hook runs before the next decoded instruction. A
        // branch may have completed before translation of its next fetch fails.
        // Defer that fetch until the next requested slice; never defer data faults.
        let stop = if exhausted_before_fetch {
            SliceStop::Yield
        } else {
            stop_reason(monitor.stop, native_fault, pc).unwrap_or_else(|| {
                if pc == completion {
                    SliceStop::Completed
                } else if monitor.dispatches == dispatches || self.exits.contains(&pc.get()) {
                    SliceStop::Yield
                } else {
                    SliceStop::Environment
                } // An unmodelled native exit is not success.
            })
        };
        Ok(Slice {
            dispatches: monitor.dispatches,
            instructions: monitor.instructions,
            repeated_instruction_pending: monitor.pending_repeat.is_some(),
            stop,
        })
    }

    fn configure_exits(&mut self, before: u64, completion: Address) -> Result<(), MachineError> {
        // Translation can read ahead of the runtime counter hook. Native exits at
        // mapping ends prevent speculative fetches from defeating a valid last
        // instruction. Starting at such an end must still attempt the real fetch.
        let mut exits: Vec<_> = self
            .initial
            .memory()
            .regions()
            .iter()
            .filter(|region| region.permissions().execute)
            .filter_map(|region| u64::try_from(region.range().end()).ok())
            .filter(|address| *address != before)
            .collect();
        exits.push(completion.get());
        exits.sort_unstable();
        exits.dedup();
        if exits != self.exits {
            self.native
                .ctl_exits_enable()
                .map_err(|_| MachineError::Backend)?;
            self.native
                .ctl_set_exits(&exits)
                .map_err(|_| MachineError::Backend)?;
            // Exit checks are embedded in translated code; setting the native exit
            // tree does not invalidate already translated blocks in Unicorn 2.1.5.
            self.native
                .ctl_flush_tb()
                .map_err(|_| MachineError::Backend)?;
            self.exits = exits;
        }
        Ok(())
    }
}

const fn stop_reason(
    observed: Option<Stop>,
    native_fault: Option<FaultKind>,
    pc: Address,
) -> Option<SliceStop> {
    match (observed, native_fault) {
        (
            Some(Stop::Fault {
                kind,
                address,
                size,
            }),
            _,
        ) => Some(SliceStop::Fault(GuestFault {
            kind,
            pc,
            address,
            size,
        })),
        (_, Some(kind)) => Some(SliceStop::Fault(GuestFault {
            kind,
            pc,
            address: None,
            size: None,
        })),
        (Some(Stop::Breakpoint(address)), None) => Some(SliceStop::Breakpoint(address)),
        (Some(Stop::Budget), None) => Some(SliceStop::Budget),
        (Some(Stop::Yield), None) => Some(SliceStop::Yield),
        (Some(Stop::Environment), None) => Some(SliceStop::Environment),
        (None, None) => None,
    }
}

const fn fault(error: uc_error) -> Result<FaultKind, MachineError> {
    Ok(match error {
        uc_error::INSN_INVALID => FaultKind::InvalidInstruction,
        uc_error::EXCEPTION => FaultKind::Exception(None),
        uc_error::READ_UNMAPPED => FaultKind::Unmapped(Access::Read),
        uc_error::WRITE_UNMAPPED => FaultKind::Unmapped(Access::Write),
        uc_error::FETCH_UNMAPPED => FaultKind::Unmapped(Access::Fetch),
        uc_error::READ_PROT => FaultKind::Protection(Access::Read),
        uc_error::WRITE_PROT => FaultKind::Protection(Access::Write),
        uc_error::FETCH_PROT => FaultKind::Protection(Access::Fetch),
        uc_error::READ_UNALIGNED => FaultKind::Unaligned(Access::Read),
        uc_error::WRITE_UNALIGNED => FaultKind::Unaligned(Access::Write),
        uc_error::FETCH_UNALIGNED => FaultKind::Unaligned(Access::Fetch),
        _ => return Err(MachineError::Backend),
    })
}
