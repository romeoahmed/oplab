//! Session boundaries precede fetch; only the native owner executes guest instructions.
use super::{Machine, MachineError, RunGoal};
use oplab_core::{address::Address, execution::GuestFault};
use oplab_runtime::Outcome;
use std::time::{Duration, Instant};
pub(crate) const SLICE_DISPATCHES: u64 = 1024;
#[derive(Debug, Clone, Copy)]
pub(crate) enum SliceStop {
    Yield,
    Completed,
    Budget,
    Breakpoint(Address),
    Target(Address),
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
        goal: &RunGoal,
        instructions: u64,
    ) -> Result<Slice, MachineError> {
        if dispatches == 0 || dispatches > SLICE_DISPATCHES {
            return Err(MachineError::Backend);
        }
        self.bypass = bypass.or(self.bypass);
        let deadline = Instant::now() + Duration::from_millis(2);
        let mut result = Slice {
            dispatches: 0,
            instructions: 0,
            repeated_instruction_pending: false,
            stop: SliceStop::Yield,
        };
        loop {
            let pc = self.pc()?;
            if pc == completion {
                result.stop = SliceStop::Completed;
                break;
            }
            if result.dispatches == dispatches
                || (result.dispatches != 0 && Instant::now() >= deadline)
            {
                break;
            }
            let continuation = self.pending_repeat == Some(pc);
            if !continuation && result.instructions == instruction_limit {
                result.stop = SliceStop::Budget;
                break;
            }
            if !continuation && self.bypass != Some(pc) && self.breakpoints.contains(&pc) {
                result.stop = SliceStop::Breakpoint(pc);
                break;
            }
            if !continuation && self.reached(goal, pc)? {
                result.stop = SliceStop::Target(pc);
                break;
            }
            let step = self
                .native
                .execute(pc, dispatches - result.dispatches)
                .map_err(|_| MachineError::Backend)?;
            if step.started {
                self.bypass = None;
            }
            result.dispatches += step.dispatches;
            result.instructions += u64::from(step.started && !continuation);
            if step.started && !continuation {
                self.trace.record(instructions + result.instructions, pc);
            }
            self.pending_repeat = step.repeated.then_some(pc);
            match step.outcome {
                Outcome::Finished => {}
                Outcome::Yield => break,
                Outcome::Environment => {
                    result.stop = SliceStop::Environment;
                    break;
                }
                Outcome::Fault(fault) => {
                    result.stop = SliceStop::Fault(fault);
                    break;
                }
            }
        }
        result.repeated_instruction_pending = self.pending_repeat.is_some();
        Ok(result)
    }
}
