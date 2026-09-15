//! Adapt typed decoder metadata without parsing display text or inferring guest state.

use super::DecodeError;
use capstone::{RegId, arch::arm64::Arm64OperandType, prelude::*};
use iced_x86::{
    Decoder, DecoderOptions, Formatter, InstructionInfoFactory, IntelFormatter, OpAccess,
};
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
    match target {
        Target::X86_64 => x86(bytes, base),
        Target::Aarch64 => arm(bytes, base),
    }
}

fn x86(bytes: &[u8], base: Address) -> Result<InstructionAnalysis, DecodeError> {
    let mut decoder = Decoder::with_ip(64, bytes, base.get(), DecoderOptions::NONE);
    let instruction = decoder.decode();
    if instruction.is_invalid() || decoder.position() != bytes.len() {
        return Err(DecodeError::Invalid(base));
    }
    let mut factory = InstructionInfoFactory::new();
    let info = factory.info(&instruction);
    let mut formatter = IntelFormatter::new();
    let registers = info
        .used_registers()
        .iter()
        .filter_map(|register| {
            let access = access(register.access())?;
            Some(RegisterAccess {
                name: formatter.format_register(register.register()).to_owned(),
                access,
            })
        })
        .collect();
    let memory = info
        .used_memory()
        .iter()
        .filter_map(|memory| access(memory.access()).map(|access| (memory, access)))
        .map(|(memory, access)| {
            let size = memory.memory_size().size();
            Ok(MemoryAccess {
                access,
                bytes: if size == 0 {
                    None
                } else {
                    Some(u32::try_from(size).map_err(|_| DecodeError::Range)?)
                },
            })
        })
        .collect::<Result<_, DecodeError>>()?;
    let branch_target = matches!(
        instruction.op0_kind(),
        iced_x86::OpKind::NearBranch16
            | iced_x86::OpKind::NearBranch32
            | iced_x86::OpKind::NearBranch64
    )
    .then(|| HexAddress::new(Address::new(instruction.near_branch_target())));
    Ok(InstructionAnalysis {
        registers,
        memory,
        branch_target,
        architecture: ArchitectureAnalysis::X86 {
            flow: flow(instruction.flow_control()),
            cpuid: instruction
                .cpuid_features()
                .iter()
                .map(|feature| format!("{feature:?}"))
                .collect(),
            flags: FlagEffects {
                read: flags(instruction.rflags_read()),
                written: flags(instruction.rflags_written()),
                cleared: flags(instruction.rflags_cleared()),
                set: flags(instruction.rflags_set()),
                undefined: flags(instruction.rflags_undefined()),
            },
            registers_incomplete: instruction.is_save_restore_instruction(),
            privileged: instruction.is_privileged(),
        },
    })
}

const fn access(value: OpAccess) -> Option<DataAccess> {
    match value {
        OpAccess::Read => Some(DataAccess::Read),
        OpAccess::CondRead => Some(DataAccess::ConditionalRead),
        OpAccess::Write => Some(DataAccess::Write),
        OpAccess::CondWrite => Some(DataAccess::ConditionalWrite),
        OpAccess::ReadWrite => Some(DataAccess::ReadWrite),
        OpAccess::ReadCondWrite => Some(DataAccess::ReadConditionalWrite),
        // InstructionInfo omits these operand-only categories from used register/memory lists.
        OpAccess::None | OpAccess::NoMemAccess => None,
    }
}

const fn flow(value: iced_x86::FlowControl) -> FlowControl {
    match value {
        iced_x86::FlowControl::Next => FlowControl::Next,
        iced_x86::FlowControl::UnconditionalBranch => FlowControl::Branch,
        iced_x86::FlowControl::IndirectBranch => FlowControl::IndirectBranch,
        iced_x86::FlowControl::ConditionalBranch => FlowControl::ConditionalBranch,
        iced_x86::FlowControl::Call => FlowControl::Call,
        iced_x86::FlowControl::IndirectCall => FlowControl::IndirectCall,
        iced_x86::FlowControl::Return => FlowControl::Return,
        iced_x86::FlowControl::Interrupt => FlowControl::Interrupt,
        iced_x86::FlowControl::XbeginXabortXend => FlowControl::Transaction,
        iced_x86::FlowControl::Exception => FlowControl::Exception,
    }
}

fn flags(mask: u32) -> Vec<String> {
    use iced_x86::RflagsBits as F;
    [
        (F::OF, "OF"),
        (F::SF, "SF"),
        (F::ZF, "ZF"),
        (F::AF, "AF"),
        (F::CF, "CF"),
        (F::PF, "PF"),
        (F::DF, "DF"),
        (F::IF, "IF"),
        (F::AC, "AC"),
        (F::UIF, "UIF"),
        (F::C0, "C0"),
        (F::C1, "C1"),
        (F::C2, "C2"),
        (F::C3, "C3"),
    ]
    .into_iter()
    .filter(|(bit, _)| mask & bit != 0)
    .map(|(_, name)| name.to_owned())
    .collect()
}

fn arm(bytes: &[u8], base: Address) -> Result<InstructionAnalysis, DecodeError> {
    if bytes.len() != 4 {
        return Err(DecodeError::Invalid(base));
    }
    let decoder = Capstone::new()
        .arm64()
        .mode(arch::arm64::ArchMode::Arm)
        .detail(true)
        .build()
        .map_err(|error| DecodeError::Backend(error.to_string()))?;
    let instructions = decoder
        .disasm_count(bytes, base.get(), 1)
        .map_err(|error| DecodeError::Backend(error.to_string()))?;
    let instruction = instructions
        .as_ref()
        .first()
        .ok_or(DecodeError::Invalid(base))?;
    let detail = decoder
        .insn_detail(instruction)
        .map_err(|error| DecodeError::Backend(error.to_string()))?;
    let architecture = detail.arch_detail();
    let arm = architecture.arm64().ok_or(DecodeError::Invalid(base))?;
    let mut registers = detail
        .regs_read()
        .iter()
        .map(|&register| {
            arm_register(
                &decoder,
                register,
                if detail.regs_write().contains(&register) {
                    DataAccess::ReadWrite
                } else {
                    DataAccess::Read
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    for &register in detail
        .regs_write()
        .iter()
        .filter(|register| !detail.regs_read().contains(register))
    {
        registers.push(arm_register(&decoder, register, DataAccess::Write)?);
    }
    // ARM64 operand access flags can conflate memory access with address writeback
    // (STR X0,[X1,#8]! reports ReadWrite). Do not present them as data effects.
    let memory = arm
        .operands()
        .filter(|operand| matches!(operand.op_type, Arm64OperandType::Mem(_)))
        .map(|_| MemoryAccess {
            access: DataAccess::Unknown,
            bytes: None,
        })
        .collect();
    let relative = detail
        .groups()
        .iter()
        .any(|group| u32::from(group.0) == arch::arm64::Arm64InsnGroup::ARM64_GRP_BRANCH_RELATIVE);
    let branch_target = if relative {
        arm.operands()
            .filter_map(|operand| match operand.op_type {
                // Capstone stores a full-width target in a signed immediate slot.
                Arm64OperandType::Imm(value) => {
                    Some(HexAddress::new(Address::new(value.cast_unsigned())))
                }
                _ => None,
            })
            .last()
    } else {
        None
    };
    Ok(InstructionAnalysis {
        registers,
        memory,
        branch_target,
        architecture: ArchitectureAnalysis::Aarch64 {
            groups: detail
                .groups()
                .iter()
                .filter_map(|&group| decoder.group_name(group))
                .collect(),
            updates_flags: arm.update_flags(),
            writeback: arm.writeback(),
        },
    })
}

fn arm_register(
    decoder: &Capstone,
    register: RegId,
    access: DataAccess,
) -> Result<RegisterAccess, DecodeError> {
    let name = decoder
        .reg_name(register)
        .ok_or_else(|| DecodeError::Backend("unnamed register".into()))?;
    Ok(RegisterAccess { name, access })
}
