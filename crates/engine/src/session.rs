//! Synchronous machine ownership with bounded slices for a worker command loop.

use crate::machine::{Machine, MachineError, SLICE_DISPATCHES, Slice, SliceStop};
use oplab_core::{
    address::Address,
    diagnostic::ValidationError,
    execution::{ControlEvent, ExecutionState, Generation, GuestFault, PauseReason, Termination},
    policy::ExecutionPolicy,
    registers::IntegerRegisters,
    target::Target,
};

/// A loaded image, execution policy and machine owned by one thread.
///
/// Call [`Session::advance`] between command polls. Native handles never cross
/// the worker's thread or transport boundary.
pub struct Session {
    machine: Machine,
    policy: ExecutionPolicy,
    state: ExecutionState,
    generation: Generation,
    dispatches: u64,
    instructions: u64,
    fault: Option<GuestFault>,
    stepping: bool,
    bypass: Option<Address>,
}

impl Session {
    /// Load a static image with explicit completion and an instruction-start budget.
    ///
    /// # Errors
    ///
    /// Rejects invalid loading inputs, control policy, and native initialization failures.
    pub fn from_elf(
        image: &[u8],
        target: Target,
        completion: Address,
        instruction_budget: u64,
    ) -> Result<Self, MachineError> {
        let machine = Machine::from_elf(image, target)?;
        Self::new(machine, completion, instruction_budget)
    }

    /// Bind a newly loaded machine to explicit completion and instruction-start policy.
    ///
    /// # Errors
    ///
    /// Rejects invalid completion or budget. The supplied machine is dropped on failure.
    pub fn new(
        machine: Machine,
        completion: Address,
        instruction_budget: u64,
    ) -> Result<Self, MachineError> {
        let policy = ExecutionPolicy::new(
            machine.initial().target(),
            machine.initial().entry(),
            completion,
            instruction_budget,
        )?;
        Ok(Self {
            machine,
            policy,
            state: ExecutionState::Ready,
            generation: Generation::INITIAL,
            dispatches: 0,
            instructions: 0,
            fault: None,
            stepping: false,
            bypass: None,
        })
    }

    /// Current control state at the last ownership boundary.
    #[must_use]
    pub const fn state(&self) -> ExecutionState {
        self.state
    }

    /// Reset generation; advances only after successful machine replacement.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }

    /// Observed native dispatches, including REP iterations and dispatched faults.
    ///
    /// Fetch/decode failures can occur before an instruction is dispatched.
    #[must_use]
    pub const fn dispatches(&self) -> u64 {
        self.dispatches
    }

    /// Observed instructions started, counting a repeated string instruction once.
    ///
    /// A started instruction may fault before retiring.
    #[must_use]
    pub const fn instructions(&self) -> u64 {
        self.instructions
    }

    /// Most recent terminal guest fault, distinct from an unusable native machine.
    #[must_use]
    pub const fn fault(&self) -> Option<GuestFault> {
        self.fault
    }

    /// Enter Running without executing; subsequent [`Self::advance`] calls run slices.
    ///
    /// # Errors
    ///
    /// Rejects running, terminal, or crashed sessions.
    pub fn start(&mut self) -> Result<(), MachineError> {
        self.begin(false)
    }

    /// Begin one architectural step and execute its first slice.
    ///
    /// REP or a cooperative yield may leave it Running. Call [`Self::advance`] until
    /// the instruction completes or execution stops.
    ///
    /// # Errors
    ///
    /// Rejects illegal control states and propagates native failures as Crashed.
    pub fn step(&mut self) -> Result<(), MachineError> {
        self.begin(true)?;
        self.advance()
    }

    fn begin(&mut self, stepping: bool) -> Result<(), MachineError> {
        let next = self.state.transition(ControlEvent::Start)?;
        self.bypass = match self.state {
            ExecutionState::Paused(PauseReason::Breakpoint(address)) => Some(address),
            _ => None,
        };
        self.stepping = stepping;
        self.state = next;
        Ok(())
    }

    /// Execute at most one bounded slice, then return control for command polling.
    ///
    /// The time bound is cooperative; one native dispatch may overrun it. A native
    /// return or budget limit alone does not establish successful completion.
    ///
    /// # Errors
    ///
    /// Rejects non-running sessions. Native failures invalidate the session as Crashed.
    pub fn advance(&mut self) -> Result<(), MachineError> {
        if self.state != ExecutionState::Running {
            return Err(ValidationError::Transition.into());
        }
        let remaining = self.policy.instruction_budget() - self.instructions;
        let count = if self.stepping {
            1
        } else {
            remaining.clamp(1, SLICE_DISPATCHES)
        };
        let result = self
            .machine
            .run_slice(
                self.policy.completion(),
                count,
                remaining,
                self.bypass.take(),
            )
            .and_then(|slice| self.accept(slice));
        if result.is_err() {
            self.state = ExecutionState::Crashed;
        }
        result
    }

    fn accept(&mut self, slice: Slice) -> Result<(), MachineError> {
        self.dispatches = self
            .dispatches
            .checked_add(slice.dispatches)
            .ok_or(ValidationError::CounterExhausted)?;
        self.instructions += slice.instructions;
        match slice.stop {
            SliceStop::Completed => self.terminate(Termination::Completed),
            SliceStop::Budget => self.terminate(Termination::Budget),
            SliceStop::Breakpoint(address) => self.pause_with(PauseReason::Breakpoint(address)),
            SliceStop::Fault(fault) => {
                self.fault = Some(fault);
                self.terminate(Termination::GuestFault)
            }
            SliceStop::Environment => self.terminate(Termination::UnsupportedEnvironment),
            SliceStop::Yield
                if self.instructions == self.policy.instruction_budget()
                    && !slice.repeated_instruction_pending =>
            {
                self.terminate(Termination::Budget)
            }
            SliceStop::Yield
                if self.stepping
                    && slice.dispatches != 0
                    && !slice.repeated_instruction_pending =>
            {
                self.pause_with(PauseReason::Step)
            }
            SliceStop::Yield => Ok(()),
        }
    }

    /// Cooperatively pause between native slices, retaining current machine effects.
    ///
    /// # Errors
    ///
    /// Rejects any session that is not Running.
    pub fn pause(&mut self) -> Result<(), MachineError> {
        self.pause_with(PauseReason::Requested)
    }

    fn pause_with(&mut self, reason: PauseReason) -> Result<(), MachineError> {
        self.state = self.state.transition(ControlEvent::Pause(reason))?;
        Ok(())
    }

    /// Cancel at an ownership boundary. Cancellation is a terminal outcome.
    ///
    /// # Errors
    ///
    /// Rejects terminal and crashed sessions.
    pub fn cancel(&mut self) -> Result<(), MachineError> {
        self.terminate(Termination::Cancelled)
    }

    fn terminate(&mut self, reason: Termination) -> Result<(), MachineError> {
        self.state = self.state.transition(ControlEvent::Terminate(reason))?;
        Ok(())
    }

    /// Restore the original memory and CPU conditions, retaining address breakpoints.
    ///
    /// Failure preserves the previous machine and generation.
    ///
    /// # Errors
    ///
    /// Rejects running/crashed sessions, exhausted generations, or native allocation failures.
    pub fn reset(&mut self) -> Result<(), MachineError> {
        let next = self.state.transition(ControlEvent::Reset)?;
        let generation = self.generation.next()?;
        self.machine.reset()?;
        self.state = next;
        self.generation = generation;
        self.dispatches = 0;
        self.instructions = 0;
        self.fault = None;
        self.stepping = false;
        self.bypass = None;
        Ok(())
    }

    /// Add/remove a bounded address breakpoint. It triggers only when that address
    /// is reached as an instruction start; arbitrary byte addresses are not decoded here.
    ///
    /// # Errors
    ///
    /// Rejects running, terminal or crashed sessions, target misalignment and excess breakpoints.
    pub fn set_breakpoint(&mut self, address: Address, enabled: bool) -> Result<(), MachineError> {
        self.state.require_patchable()?;
        self.machine.set_breakpoint(address, enabled)
    }

    /// Sorted address breakpoints, retained across reset and cleared by a new load.
    pub fn breakpoints(&self) -> impl Iterator<Item = Address> + '_ {
        self.machine.breakpoints()
    }

    /// Write a canonical GPR or stack pointer between execution slices.
    ///
    /// Counters, PC, flags, breakpoints and the retained reset image are unchanged.
    /// An interrupted REP instruction remains one instruction when resumed.
    ///
    /// # Errors
    ///
    /// Rejects states other than ready or paused, and noncanonical names, before mutation.
    /// A native failure invalidates the session; uncertain writes are never replayed.
    pub fn write_register(&mut self, name: &str, value: u64) -> Result<(), MachineError> {
        self.state.require_patchable()?;
        let result = self.machine.write_register(name, value);
        self.accept_write(result)
    }

    /// Patch 1–65,536 bytes in one mapped region without widening guest permissions.
    ///
    /// Success invalidates overlapping executable translations. Reset restores
    /// original bytes; patching does not modify the source or assembled artifact.
    ///
    /// # Errors
    ///
    /// Rejects states other than ready or paused, and invalid ranges, before mutation.
    /// Native write/cache failures invalidate the session, with no rollback guarantee.
    pub fn write_memory(&mut self, address: Address, bytes: &[u8]) -> Result<(), MachineError> {
        self.state.require_patchable()?;
        let result = self.machine.write_memory(address, bytes);
        self.accept_write(result)
    }

    const fn accept_write(&mut self, result: Result<(), MachineError>) -> Result<(), MachineError> {
        if matches!(result, Err(MachineError::Backend)) {
            self.state = ExecutionState::Crashed;
        }
        result
    }

    /// Observe canonical integer registers between native slices.
    ///
    /// # Errors
    ///
    /// Rejects crashed sessions and native observation failures.
    pub fn read_registers(&self) -> Result<IntegerRegisters, MachineError> {
        if self.state == ExecutionState::Crashed {
            return Err(MachineError::Backend);
        }
        self.machine.read_registers()
    }

    /// Observe bounded memory between native slices, including terminal fault state.
    ///
    /// # Errors
    ///
    /// Rejects crashed sessions, invalid ranges and native observation failures.
    pub fn read_memory(&self, address: Address, length: u64) -> Result<Vec<u8>, MachineError> {
        if self.state == ExecutionState::Crashed {
            return Err(MachineError::Backend);
        }
        self.machine.read_memory(address, length)
    }
}
