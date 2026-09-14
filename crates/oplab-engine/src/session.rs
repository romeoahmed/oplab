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

/// One bound image and its execution controls. The worker calls `advance` between
/// command polls; no emulator handle crosses a thread or transport boundary.
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
    /// Rejects invalid loading inputs, control policy, and native initialization failures.
    pub fn from_elf(
        image: &[u8],
        target: Target,
        completion: Address,
        instruction_budget: u64,
    ) -> Result<Self, MachineError> {
        let machine = Machine::from_elf(image, target)?;
        let policy = ExecutionPolicy::new(
            target,
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

    /// Generation of the initial image and conditions, advanced by reset.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }

    /// Observed native dispatches, including REP iterations and dispatched faults.
    /// Fetch/decode failures can occur before an instruction is dispatched.
    #[must_use]
    pub const fn dispatches(&self) -> u64 {
        self.dispatches
    }

    /// Observed instructions started, counting a repeated string instruction once.
    /// A faulting instruction is not thereby claimed to have retired successfully.
    #[must_use]
    pub const fn instructions(&self) -> u64 {
        self.instructions
    }

    /// Most recent terminal guest fault, distinct from loss of the native worker.
    #[must_use]
    pub const fn fault(&self) -> Option<GuestFault> {
        self.fault
    }

    /// Start running; subsequent `advance` calls execute bounded ownership slices.
    ///
    /// # Errors
    /// Rejects running, terminal, or crashed sessions.
    pub fn start(&mut self) -> Result<(), MachineError> {
        self.begin(false)
    }

    /// Begin one architectural step. REP or a cooperative yield may leave it Running;
    /// `advance` continues until the instruction finishes or execution stops.
    ///
    /// # Errors
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
    /// A native return, timeout, or budget limit is never inferred to mean completion.
    ///
    /// # Errors
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
    /// Rejects terminal and crashed sessions.
    pub fn cancel(&mut self) -> Result<(), MachineError> {
        self.terminate(Termination::Cancelled)
    }

    fn terminate(&mut self, reason: Termination) -> Result<(), MachineError> {
        self.state = self.state.transition(ControlEvent::Terminate(reason))?;
        Ok(())
    }

    /// Restore the original memory and CPU conditions, retaining address breakpoints.
    /// The generation advances only after replacement initialization succeeds.
    ///
    /// # Errors
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
    /// Rejects running/terminal sessions, target misalignment, and too many breakpoints.
    pub fn set_breakpoint(&mut self, address: Address, enabled: bool) -> Result<(), MachineError> {
        self.state.require_patchable()?;
        self.machine.set_breakpoint(address, enabled)
    }

    /// Observe canonical integer registers between native slices.
    ///
    /// # Errors
    /// Rejects a lost machine or failed native observation.
    pub fn read_registers(&self) -> Result<IntegerRegisters, MachineError> {
        if self.state == ExecutionState::Crashed {
            return Err(MachineError::Backend);
        }
        self.machine.read_registers()
    }

    /// Observe bounded memory between native slices, including terminal fault state.
    ///
    /// # Errors
    /// Rejects a lost machine, invalid ranges, and native observation failures.
    pub fn read_memory(&self, address: Address, length: u64) -> Result<Vec<u8>, MachineError> {
        if self.state == ExecutionState::Crashed {
            return Err(MachineError::Backend);
        }
        self.machine.read_memory(address, length)
    }
}
