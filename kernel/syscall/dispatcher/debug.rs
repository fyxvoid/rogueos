//! Hardware breakpoint and perf-counter syscall handlers.
//!
//! These are the pentester/developer primitives that make this kernel unique.
//! A ring-3 debugger, fuzzer, or security tool can set hardware breakpoints on
//! any address in its own address space without ptrace, without CAP_SYS_PTRACE,
//! and without modifying the target binary.

use crate::arch::x86_64::debug_regs::{BpCondition, BpLen, HwBpState, MAX_HW_BREAKPOINTS};
use crate::syscall::user_ptr::SysErr;

// ---------------------------------------------------------------------------
// SYS_HW_BP_SET — set a hardware breakpoint
//
// a1: slot index (0–3)
// a2: virtual address to watch
// a3: condition (0=exec, 1=write, 2=io_rw, 3=rw)
// a4: length    (0=1B, 1=2B, 2=8B, 3=4B)
// Returns 0 on success, negative SysErr on failure.
// ---------------------------------------------------------------------------

pub fn sys_hw_bp_set(slot: u64, addr: u64, cond: u64, len: u64) -> Result<u64, SysErr> {
    let slot = slot as usize;
    if slot >= MAX_HW_BREAKPOINTS {
        return Err(SysErr::INVAL);
    }

    // Validate: addr must be in user virtual address space (< 0x0000_8000_0000_0000).
    if addr >= 0x0000_8000_0000_0000 {
        return Err(SysErr::INVAL);
    }

    let condition = match cond {
        0 => BpCondition::Execute,
        1 => BpCondition::Write,
        2 => BpCondition::IoRw,
        3 => BpCondition::ReadWrite,
        _ => return Err(SysErr::INVAL),
    };

    let length = match len {
        0 => BpLen::Byte,
        1 => BpLen::Word,
        2 => BpLen::Qword,
        3 => BpLen::Dword,
        _ => return Err(SysErr::INVAL),
    };

    // Execution BPs must use Byte length per AMD manual.
    let length = if condition == BpCondition::Execute { BpLen::Byte } else { length };

    let cur_idx = crate::process::current_index().ok_or(SysErr::INVAL)?;
    let pcb = crate::process::get_descriptor_mut(cur_idx).ok_or(SysErr::INVAL)?;
    pcb.hw_bp.set(slot, addr, condition, length);

    // Immediately write to hardware registers (current process context).
    unsafe { crate::arch::x86_64::debug_regs::write_dr(&pcb.hw_bp); }

    Ok(0)
}

// ---------------------------------------------------------------------------
// SYS_HW_BP_CLEAR — clear a hardware breakpoint slot
//
// a1: slot index (0–3), or 0xFF = clear all
// Returns 0 on success.
// ---------------------------------------------------------------------------

pub fn sys_hw_bp_clear(slot: u64) -> Result<u64, SysErr> {
    let cur_idx = crate::process::current_index().ok_or(SysErr::INVAL)?;
    let pcb = crate::process::get_descriptor_mut(cur_idx).ok_or(SysErr::INVAL)?;

    if slot == 0xFF {
        // Clear all slots.
        for i in 0..MAX_HW_BREAKPOINTS {
            pcb.hw_bp.clear(i);
        }
    } else {
        let s = slot as usize;
        if s >= MAX_HW_BREAKPOINTS {
            return Err(SysErr::INVAL);
        }
        pcb.hw_bp.clear(s);
    }

    unsafe { crate::arch::x86_64::debug_regs::write_dr(&pcb.hw_bp); }
    Ok(0)
}

// ---------------------------------------------------------------------------
// SYS_HW_BP_QUERY — read hardware breakpoint state
//
// a1: out_ptr *mut HwBpInfo (user pointer, 64 bytes)
//
// HwBpInfo layout (repr C, 64 bytes):
//   [0..32]  dr0..dr3 (4×u64)
//   [32..40] dr6      (u64)
//   [40..48] dr7      (u64)
//   [48..56] enabled  (u8 × 4, 1=enabled) + padding
//
// Returns 0 on success, negative on error.
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct HwBpInfo {
    pub dr: [u64; 4],
    pub dr6: u64,
    pub dr7: u64,
    pub enabled: [u8; 4],
    pub _pad: [u8; 4],
}

pub fn sys_hw_bp_query(out_ptr: *mut HwBpInfo) -> Result<u64, SysErr> {
    if out_ptr.is_null() {
        return Err(SysErr::INVAL);
    }

    let cur_idx = crate::process::current_index().ok_or(SysErr::INVAL)?;

    // Refresh DR6 from hardware before reading (it latches on #DB).
    let pcb = crate::process::get_descriptor_mut(cur_idx).ok_or(SysErr::INVAL)?;
    unsafe { crate::arch::x86_64::debug_regs::read_dr(&mut pcb.hw_bp); }

    let hw = &pcb.hw_bp;

    // Validate user pointer.
    let cr3 = crate::syscall::user_ptr::current_cr3()?;
    crate::syscall::user_ptr::validate_user_range(
        cr3,
        out_ptr as u64,
        core::mem::size_of::<HwBpInfo>(),
        true,
    )?;

    let info = HwBpInfo {
        dr: [hw.dr0, hw.dr1, hw.dr2, hw.dr3],
        dr6: hw.dr6,
        dr7: hw.dr7,
        enabled: [
            hw.is_enabled(0) as u8,
            hw.is_enabled(1) as u8,
            hw.is_enabled(2) as u8,
            hw.is_enabled(3) as u8,
        ],
        _pad: [0; 4],
    };

    unsafe { core::ptr::write(out_ptr, info); }
    Ok(0)
}
