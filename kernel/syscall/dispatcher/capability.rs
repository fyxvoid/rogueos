//! Capability syscall handlers: CAP_GRANT, CAP_REVOKE, CAP_QUERY, JOURNAL_WRITE, JOURNAL_READ.

use crate::capability::{self, CapSet};
use crate::syscall::user_ptr::{self, SysErr};
use libs::cap;

/// SYS_CAP_GRANT — grant capability bits to a target process.
/// Args: target_pid (u32), cap_bits (u64).
/// Requires CAP_GRANT. Parent cannot grant bits it does not itself hold.
pub(super) fn sys_cap_grant(target_pid_raw: u64, cap_bits: u64) -> Result<u64, SysErr> {
    capability::require(cap::GRANT, "cap_grant")?;
    let caller_caps = capability::current_caps();
    // Restrict to bits the granting process actually holds (no elevation).
    let safe_bits = caller_caps.bits & cap_bits;
    let target_pid = target_pid_raw as u32;
    for i in 0..crate::process::MAX_PROCESSES {
        if let Some(pcb) = crate::process::get_descriptor_mut(i) {
            if pcb.pid == target_pid {
                pcb.caps.grant(safe_bits);
                crate::arch::serial::write_str("[CAP] granted bits=");
                crate::arch::serial::write_hex(safe_bits);
                crate::arch::serial::write_str(" to pid=");
                crate::arch::serial::write_hex(target_pid as u64);
                crate::arch::serial::write_str("\r\n");
                return Ok(safe_bits);
            }
        }
    }
    Err(SysErr::NOENT)
}

/// SYS_CAP_REVOKE — remove capability bits from a target process.
/// Args: target_pid (u32), cap_bits (u64). Requires CAP_GRANT.
pub(super) fn sys_cap_revoke(target_pid_raw: u64, cap_bits: u64) -> Result<u64, SysErr> {
    capability::require(cap::GRANT, "cap_revoke")?;
    let target_pid = target_pid_raw as u32;
    for i in 0..crate::process::MAX_PROCESSES {
        if let Some(pcb) = crate::process::get_descriptor_mut(i) {
            if pcb.pid == target_pid {
                pcb.caps.revoke(cap_bits);
                crate::arch::serial::write_str("[CAP] revoked bits=");
                crate::arch::serial::write_hex(cap_bits);
                crate::arch::serial::write_str(" from pid=");
                crate::arch::serial::write_hex(target_pid as u64);
                crate::arch::serial::write_str("\r\n");
                return Ok(0);
            }
        }
    }
    Err(SysErr::NOENT)
}

/// SYS_CAP_QUERY — return the capability bitmask of the current process.
pub(super) fn sys_cap_query() -> Result<u64, SysErr> {
    Ok(capability::current_caps().bits)
}

/// SYS_JOURNAL_WRITE — overwrite Cogman restart journal with caller's data.
/// Args: ptr (*const u8), len (usize). Requires CAP_JOURNAL.
pub(super) fn sys_journal_write(ptr: u64, len: u64) -> Result<u64, SysErr> {
    capability::require(cap::JOURNAL, "journal_write")?;
    let len = len as usize;
    if len == 0 || len > crate::capability::journal::JOURNAL_SIZE {
        return Err(SysErr::INVAL);
    }
    let cr3 = user_ptr::current_cr3()?;
    user_ptr::validate_user_range(cr3, ptr, len, false)?;
    let pid = crate::process::current_pid().unwrap_or(0);
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    let written = crate::capability::journal::write(bytes, pid);
    if written == 0 {
        Err(SysErr::INVAL)
    } else {
        Ok(written as u64)
    }
}

/// SYS_JOURNAL_READ — copy current journal into caller buffer.
/// Args: ptr (*mut u8), cap (usize). Returns bytes copied. Requires CAP_JOURNAL.
pub(super) fn sys_journal_read(ptr: u64, cap: u64) -> Result<u64, SysErr> {
    capability::require(cap::JOURNAL, "journal_read")?;
    let cap = cap as usize;
    if cap == 0 {
        return Ok(0);
    }
    let cr3 = user_ptr::current_cr3()?;
    user_ptr::validate_user_range(cr3, ptr, cap, true)?;
    let buf = unsafe { core::slice::from_raw_parts_mut(ptr as *mut u8, cap) };
    Ok(crate::capability::journal::read(buf) as u64)
}
