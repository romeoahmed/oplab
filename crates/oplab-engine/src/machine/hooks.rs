//! Bounded observations at native hooks; callbacks never unwind or perform host I/O.

use iced_x86::{Decoder, DecoderOptions, Mnemonic};
use oplab_core::{
    address::Address,
    execution::{Access, FaultKind},
    target::Target,
};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
use unicorn_engine::{
    Unicorn,
    unicorn_const::{HookType, MemType, X86Insn, uc_error},
};

#[derive(Debug, Clone, Copy)]
pub(super) enum Stop {
    Yield,
    Breakpoint(Address),
    Budget,
    Environment,
    Fault {
        kind: FaultKind,
        address: Option<Address>,
        size: Option<u64>,
    },
}

pub(super) struct Monitor {
    target: Target,
    pub active: bool,
    pub dispatches: u64,
    pub instructions: u64,
    pub last_instruction: Option<Address>,
    instruction_limit: u64,
    pub pending_repeat: Option<(Address, [u8; 15], u32)>,
    deadline: Option<Instant>,
    pub stop: Option<Stop>,
    pub failed: bool,
    pub bypass: Option<Address>,
    pub breakpoints: BTreeSet<Address>,
}

impl Monitor {
    pub(super) const fn new(target: Target) -> Self {
        Self {
            target,
            active: false,
            dispatches: 0,
            instructions: 0,
            last_instruction: None,
            instruction_limit: 0,
            pending_repeat: None,
            deadline: None,
            stop: None,
            failed: false,
            bypass: None,
            breakpoints: BTreeSet::new(),
        }
    }

    pub(super) fn prepare(&mut self, instruction_limit: u64, bypass: Option<Address>) {
        self.active = true;
        self.dispatches = 0;
        self.instructions = 0;
        self.last_instruction = None;
        self.instruction_limit = instruction_limit;
        self.stop = None;
        self.failed = false;
        // A time slice may yield before the bypassed instruction is admitted.
        self.bypass = bypass.or(self.bypass);
        self.deadline = Some(Instant::now() + Duration::from_millis(2));
    }
}

pub(super) fn install(native: &mut Unicorn<'_, Monitor>) -> Result<(), uc_error> {
    // begin > end is Unicorn's documented whole-address-space hook convention.
    native.add_code_hook(1, 0, instruction)?;
    native.add_mem_hook(HookType::MEM_INVALID, 1, 0, memory_fault)?;
    native.add_intr_hook(interrupt)?;
    if native.get_data().target == Target::X86_64 {
        for instruction in [X86Insn::SYSCALL, X86Insn::SYSENTER] {
            native
                .add_insn_sys_hook(instruction, 1, 0, |native| stop(native, Stop::Environment))?;
        }
        native.add_insn_in_hook(|native, _, _| {
            stop(native, Stop::Environment);
            0
        })?;
        native.add_insn_out_hook(|native, _, _, _| stop(native, Stop::Environment))?;
    }
    Ok(())
}

fn instruction(native: &mut Unicorn<'_, Monitor>, address: u64, size: u32) {
    let monitor = native.get_data_mut();
    if !monitor.active {
        return;
    }
    if let Some(reason) = monitor.stop {
        stop(native, reason);
        return;
    }
    if monitor
        .deadline
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        stop(native, Stop::Yield);
        return;
    }
    let address = Address::new(address);
    let mut bytes = [0_u8; 15];
    let Some(instruction_bytes) = usize::try_from(size)
        .ok()
        .and_then(|size| bytes.get_mut(..size))
    else {
        return; // Invalid native instruction sizes are handled by emu_start's result.
    };
    if native.mem_read(address.get(), instruction_bytes).is_err() {
        native.get_data_mut().failed = true;
        stop(native, Stop::Environment);
        return;
    }
    let (repeat, waits) = match native.get_data().target {
        Target::X86_64 => {
            let instruction = Decoder::new(64, instruction_bytes, DecoderOptions::NONE).decode();
            (
                instruction.is_string_instruction()
                    && (instruction.has_rep_prefix() || instruction.has_repne_prefix()),
                instruction.mnemonic() == Mnemonic::Hlt,
            )
        }
        Target::Aarch64 => (
            false,
            matches!(instruction_bytes, [0x5f | 0x7f, 0x20, 0x03, 0xd5]),
        ),
    };
    let identity = (address, bytes, size);
    let monitor = native.get_data_mut();
    let continuation = repeat && monitor.pending_repeat == Some(identity);
    if !continuation && monitor.instructions == monitor.instruction_limit {
        stop(native, Stop::Budget);
        return;
    }
    let bypass = monitor.bypass.take() == Some(address);
    if !bypass && !continuation && monitor.breakpoints.contains(&address) {
        stop(native, Stop::Breakpoint(address));
        return;
    }
    monitor.dispatches += 1; // Each slice is bounded to at most 1,024 dispatches.
    monitor.last_instruction = Some(address);
    monitor.instructions += u64::from(!continuation);
    monitor.pending_repeat = repeat.then_some(identity);
    if waits {
        stop(native, Stop::Environment);
    }
}

fn memory_fault(
    native: &mut Unicorn<'_, Monitor>,
    access: MemType,
    address: u64,
    size: usize,
    _: i64,
) -> bool {
    let kind = match access {
        MemType::READ_UNMAPPED => FaultKind::Unmapped(Access::Read),
        MemType::WRITE_UNMAPPED => FaultKind::Unmapped(Access::Write),
        MemType::FETCH_UNMAPPED => FaultKind::Unmapped(Access::Fetch),
        MemType::READ_PROT => FaultKind::Protection(Access::Read),
        MemType::WRITE_PROT => FaultKind::Protection(Access::Write),
        MemType::FETCH_PROT => FaultKind::Protection(Access::Fetch),
        _ => return false,
    };
    if native.get_data().active {
        native
            .get_data_mut()
            .stop
            .get_or_insert_with(|| Stop::Fault {
                kind,
                address: Some(Address::new(address)),
                size: Some(size as u64),
            });
    }
    false // Never repair a failed guest access or widen its permissions.
}

fn interrupt(native: &mut Unicorn<'_, Monitor>, vector: u32) {
    let environment = match native.get_data().target {
        Target::X86_64 => vector >= 32,
        Target::Aarch64 => matches!(vector, 2 | 11 | 13), // SVC, HVC, SMC in Unicorn's Arm model.
    };
    let reason = if environment {
        Stop::Environment
    } else {
        Stop::Fault {
            kind: FaultKind::Exception(Some(vector)),
            address: None,
            size: None,
        }
    };
    stop(native, reason);
}

fn stop(native: &mut Unicorn<'_, Monitor>, reason: Stop) {
    if native.get_data().active {
        native.get_data_mut().stop.get_or_insert(reason);
        if native.emu_stop().is_err() {
            native.get_data_mut().failed = true;
        }
    }
}
