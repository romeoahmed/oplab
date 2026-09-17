//! Adapt typed decoder metadata without parsing display text or inferring guest state.

use super::DecodeError;
use super::{bridge, native};
use oplab_core::{
    address::{Address, AddressRange},
    protocol::{
        analysis::{
            ArchitectureAnalysis, DataAccess, FlagEffects, FlowControl, InstructionAnalysis,
            MemoryAccess, RegisterAccess,
        },
        scalar::HexAddress,
    },
    target::Target,
};

/// Analyze exactly one complete instruction at its real address.
///
/// Metadata describes potential effects, not retired effects or emulator support.
/// Register aliases and backend-specific limitations are preserved.
///
/// # Errors
///
/// Rejects empty, oversized, trailing, incomplete, misaligned or wrapping inputs,
/// unknown encodings and decoder initialization/detail failures.
pub fn analyze(
    target: Target,
    bytes: &[u8],
    base: Address,
) -> Result<InstructionAnalysis, DecodeError> {
    let maximum = match target {
        Target::X86_64 => 15,
        Target::Aarch64 => 4,
    };
    let length = u64::try_from(bytes.len()).map_err(|_| DecodeError::Range)?;
    AddressRange::new(base, length, maximum).map_err(|_| DecodeError::Range)?;
    if !base.get().is_multiple_of(target.instruction_alignment()) {
        return Err(DecodeError::Range);
    }
    let window = native(target, bytes, base, 1, true)?;
    if window.consumed != bytes.len() {
        return Err(DecodeError::Invalid(base));
    }
    let instruction = window
        .instructions
        .into_iter()
        .next()
        .ok_or(DecodeError::Invalid(base))?;
    let architecture = match target {
        Target::X86_64 => ArchitectureAnalysis::X86 {
            flow: flow(instruction.flow)?,
            isa: instruction.isa,
            flags: FlagEffects {
                read: instruction.flags.read,
                written: instruction.flags.written,
                cleared: instruction.flags.cleared,
                set: instruction.flags.set,
                undefined: instruction.flags.undefined,
            },
            registers_incomplete: instruction.registers_incomplete,
            privileged: instruction.privileged,
        },
        Target::Aarch64 => ArchitectureAnalysis::Aarch64 {
            groups: instruction.groups,
            updates_flags: instruction.updates_flags,
            writeback: instruction.writeback,
        },
    };
    Ok(InstructionAnalysis {
        registers: instruction
            .registers
            .into_iter()
            .map(|reg| {
                Ok(RegisterAccess {
                    name: reg.name,
                    access: access(reg.access)?,
                })
            })
            .collect::<Result<_, DecodeError>>()?,
        memory: instruction
            .memory
            .into_iter()
            .map(|memory| {
                Ok(MemoryAccess {
                    access: access(memory.access)?,
                    bytes: (memory.bytes != 0).then_some(memory.bytes),
                })
            })
            .collect::<Result<_, DecodeError>>()?,
        branch_target: instruction
            .has_branch
            .then(|| HexAddress::new(Address::new(instruction.branch))),
        architecture,
    })
}

fn access(value: bridge::Access) -> Result<DataAccess, DecodeError> {
    match value {
        bridge::Access::Unknown => Ok(DataAccess::Unknown),
        bridge::Access::Read => Ok(DataAccess::Read),
        bridge::Access::ConditionalRead => Ok(DataAccess::ConditionalRead),
        bridge::Access::Write => Ok(DataAccess::Write),
        bridge::Access::ConditionalWrite => Ok(DataAccess::ConditionalWrite),
        bridge::Access::ReadWrite => Ok(DataAccess::ReadWrite),
        bridge::Access::ReadConditionalWrite => Ok(DataAccess::ReadConditionalWrite),
        bridge::Access::ConditionalReadWrite => Ok(DataAccess::ConditionalReadWrite),
        _ => Err(DecodeError::Backend("invalid native metadata".into())),
    }
}

fn flow(value: bridge::Flow) -> Result<FlowControl, DecodeError> {
    match value {
        bridge::Flow::Next => Ok(FlowControl::Next),
        bridge::Flow::Branch => Ok(FlowControl::Branch),
        bridge::Flow::IndirectBranch => Ok(FlowControl::IndirectBranch),
        bridge::Flow::ConditionalBranch => Ok(FlowControl::ConditionalBranch),
        bridge::Flow::Call => Ok(FlowControl::Call),
        bridge::Flow::IndirectCall => Ok(FlowControl::IndirectCall),
        bridge::Flow::Return => Ok(FlowControl::Return),
        bridge::Flow::Interrupt => Ok(FlowControl::Interrupt),
        bridge::Flow::Transaction => Ok(FlowControl::Transaction),
        bridge::Flow::Exception => Ok(FlowControl::Exception),
        _ => Err(DecodeError::Backend("invalid native metadata".into())),
    }
}
