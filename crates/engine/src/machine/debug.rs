//! Temporary execution goals and opt-in bounded instruction history.

use super::{Machine, MachineError};
use oplab_core::{
    address::Address,
    diagnostic::ValidationError,
    protocol::analysis::{ArchitectureAnalysis, FlowControl},
    target::Target,
};
use oplab_runtime::Register;
use oplab_toolchain::decode;
use std::collections::{BTreeSet, VecDeque};

const TRACE_CAPACITY: usize = 512;

pub(crate) enum RunGoal {
    Continue,
    Step,
    Until(BTreeSet<Address>),
    Return { address: Address, stack: u64 },
}

/// An instruction admitted for execution, possibly faulting before retirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceEntry {
    /// One-based session instruction-start counter, counting REP only once.
    pub instruction: u64,
    /// Native PC when the instruction started.
    pub pc: Address,
}

/// Oldest-first suffix of at most 512 starts; recording is disabled by default.
#[derive(Default)]
pub struct Trace {
    enabled: bool,
    entries: VecDeque<TraceEntry>,
    discarded: u64,
}

impl Trace {
    /// Whether new starts are captured. Disabled intervals leave gaps in instruction counters.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Entries evicted since clear/reset, excluding intervals when recording was disabled.
    #[must_use]
    pub const fn discarded(&self) -> u64 {
        self.discarded
    }

    /// Retained instruction starts in chronological order.
    #[must_use]
    pub fn entries(&self) -> impl ExactSizeIterator<Item = &TraceEntry> {
        self.entries.iter()
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.discarded = 0;
    }

    pub(super) fn record(&mut self, instruction: u64, pc: Address) {
        if !self.enabled {
            return;
        }
        if self.entries.len() == TRACE_CAPACITY {
            self.entries.pop_front();
            self.discarded += 1;
        }
        self.entries.push_back(TraceEntry { instruction, pc });
    }
}

impl Machine {
    pub(crate) const fn trace(&self) -> &Trace {
        &self.trace
    }

    pub(crate) const fn record_trace(&mut self, enabled: bool) {
        self.trace.enabled = enabled;
    }

    pub(crate) fn clear_trace(&mut self) {
        self.trace.clear();
    }

    pub(crate) fn run_goal(&self, addresses: &[Address]) -> Result<RunGoal, MachineError> {
        if addresses.is_empty() || addresses.len() > 256 {
            return Err(ValidationError::Length.into());
        }
        if addresses.iter().any(|address| {
            !address
                .get()
                .is_multiple_of(self.initial.target().instruction_alignment())
        }) {
            return Err(ValidationError::Alignment.into());
        }
        Ok(RunGoal::Until(addresses.iter().copied().collect()))
    }

    pub(crate) fn step_over_goal(&self) -> Result<RunGoal, MachineError> {
        let pc = self.pc()?;
        let target = self.initial.target();
        // Read live bytes one mapping at a time; a short window can still contain a full instruction.
        let maximum = if target == Target::X86_64 { 15 } else { 4 };
        let mut bytes = Vec::with_capacity(maximum);
        while bytes.len() < maximum {
            let Ok(address) =
                pc.checked_add(u64::try_from(bytes.len()).map_err(|_| ValidationError::Length)?)
            else {
                break;
            };
            let Some(range) = self
                .initial
                .memory()
                .regions()
                .iter()
                .map(oplab_core::memory::MemoryRegion::range)
                .find(|range| range.contains(address))
            else {
                break;
            };
            let available = range.length() - (address.get() - range.start().get());
            let remaining =
                u64::try_from(maximum - bytes.len()).map_err(|_| ValidationError::Length)?;
            bytes.extend(self.read_memory(address, available.min(remaining))?);
        }
        let instruction = decode::decode(target, &bytes, pc, 1)
            .map_err(|error| decode_error(&error))?
            .into_iter()
            .next()
            .ok_or(ValidationError::Target)?;
        let analysis = decode::analyze(target, &instruction.bytes, pc)
            .map_err(|error| decode_error(&error))?;
        let call = match analysis.architecture {
            ArchitectureAnalysis::X86 { flow, .. } => {
                matches!(flow, FlowControl::Call | FlowControl::IndirectCall)
            }
            ArchitectureAnalysis::Aarch64 { groups, .. } => {
                groups.iter().any(|group| group == "call")
            }
        };
        if !call {
            return Ok(RunGoal::Step);
        }
        Ok(RunGoal::Return {
            address: pc.checked_add(
                u64::try_from(instruction.bytes.len()).map_err(|_| ValidationError::Length)?,
            )?,
            stack: self.stack_pointer()?,
        })
    }

    fn stack_pointer(&self) -> Result<u64, MachineError> {
        let index = match self.initial.target() {
            Target::X86_64 => 4,
            Target::Aarch64 => 31,
        };
        self.native
            .register(Register::Gpr(index))
            .map(u64::from_le_bytes)
            .map_err(|_| MachineError::Backend)
    }

    pub(super) fn reached(&self, goal: &RunGoal, pc: Address) -> Result<bool, MachineError> {
        match goal {
            RunGoal::Until(addresses) => Ok(addresses.contains(&pc)),
            RunGoal::Return { address, stack } => {
                Ok(pc == *address && self.stack_pointer()? == *stack)
            }
            RunGoal::Continue | RunGoal::Step => Ok(false),
        }
    }
}

fn decode_error(error: &decode::DecodeError) -> MachineError {
    match error {
        decode::DecodeError::Backend(_) => MachineError::Backend,
        decode::DecodeError::Range | decode::DecodeError::Invalid(_) => {
            ValidationError::Target.into()
        }
    }
}
