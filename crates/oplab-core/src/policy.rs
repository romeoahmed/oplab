//! Validated immutable execution inputs.

use crate::{address::Address, diagnostic::ValidationError, target::Target};

/// Explicit snippet completion and a bounded instruction-start budget.
/// REP iterations belong to one instruction; native dispatches are counted separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionPolicy {
    completion: Address,
    instruction_budget: u64,
}

impl ExecutionPolicy {
    /// Validate aligned, distinct entry/completion addresses and a positive budget.
    ///
    /// # Errors
    /// Rejects incompatible instruction alignment and budgets outside 1..=100,000,000.
    pub fn new(
        target: Target,
        entry: Address,
        completion: Address,
        instruction_budget: u64,
    ) -> Result<Self, ValidationError> {
        let alignment = target.instruction_alignment();
        if !entry.get().is_multiple_of(alignment)
            || !completion.get().is_multiple_of(alignment)
            || entry == completion
        {
            return Err(ValidationError::Target);
        }
        if instruction_budget == 0 || instruction_budget > 100_000_000 {
            return Err(ValidationError::Length);
        }
        Ok(Self {
            completion,
            instruction_budget,
        })
    }

    /// Stop successfully before fetching this address.
    #[must_use]
    pub const fn completion(self) -> Address {
        self.completion
    }

    /// Maximum instructions started, including instructions that later fault.
    #[must_use]
    pub const fn instruction_budget(self) -> u64 {
        self.instruction_budget
    }
}
