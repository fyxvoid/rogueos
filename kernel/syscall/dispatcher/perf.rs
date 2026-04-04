//! Perf-counter syscall handlers.
//!
//! These let ring-3 userland (AI/ML schedulers, profilers, pentest tools)
//! read hardware performance counters without ring-0 access.

use crate::syscall::user_ptr::SysErr;

// ---------------------------------------------------------------------------
// SYS_PERF_OPEN
//
// a1: event_id (u32) — see PerfEvent enum in arch/x86_64/perf.rs
//     0=cycles, 1=instructions, 2=L1d-access, 3=L1d-miss, 4=L2-access,
//     5=L2-miss, 6=branches, 7=branch-mispr, 8=icache-miss, 9=stall-cycles
//
// Returns: handle (u64, 0..5) on success, negative error on failure.
// ---------------------------------------------------------------------------

pub fn sys_perf_open(event_id: u64) -> Result<u64, SysErr> {
    let proc_idx = crate::process::current_index().ok_or(SysErr::INVAL)?;
    crate::arch::x86_64::perf::perf_open(event_id as u32, proc_idx)
        .map(|h| h as u64)
        .map_err(|_| SysErr::NOMEM) // NOMEM = all counters busy
}

// ---------------------------------------------------------------------------
// SYS_PERF_READ
//
// a1: handle (u64)
// a2: out_ptr (*mut u64) — userland pointer to write 64-bit count into
//
// Returns: 0 on success, negative error on failure.
// ---------------------------------------------------------------------------

pub fn sys_perf_read(handle: u64, out_ptr: *mut u64) -> Result<u64, SysErr> {
    if out_ptr.is_null() {
        return Err(SysErr::INVAL);
    }

    // Validate user pointer.
    let cr3 = crate::syscall::user_ptr::current_cr3()?;
    crate::syscall::user_ptr::validate_user_range(cr3, out_ptr as u64, 8, true)?;

    let count = crate::arch::x86_64::perf::perf_read(handle as u32)
        .map_err(|_| SysErr::INVAL)?;

    unsafe { core::ptr::write(out_ptr, count); }
    Ok(0)
}

// ---------------------------------------------------------------------------
// SYS_PERF_CLOSE
//
// a1: handle (u64)
//
// Returns: 0 on success, negative error on failure.
// ---------------------------------------------------------------------------

pub fn sys_perf_close(handle: u64) -> Result<u64, SysErr> {
    crate::arch::x86_64::perf::perf_close(handle as u32)
        .map(|_| 0u64)
        .map_err(|_| SysErr::INVAL)
}
