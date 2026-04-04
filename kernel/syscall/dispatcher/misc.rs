//! Misc syscalls: reboot, debug_dump_ptes.

use crate::syscall::user_ptr::{self, SysErr};

pub(super) fn sys_reboot(mode: u32) -> Result<u64, SysErr> {
    let _ = crate::fs::flush_volume_header();
    match mode {
        0 => loop {
            crate::arch::halt();
        },
        1 => crate::arch::x86_64::reboot(),
        _ => Err(SysErr::INVAL),
    }
}

/// Debug: dump PTEs for va_start..va_end. Uses current process CR3 only (user-passed cr3 ignored).
pub(super) fn sys_debug_dump_ptes(_cr3: u64, va_start: u64, va_end: u64) -> Result<u64, SysErr> {
    let cr3 = user_ptr::current_cr3()?;
    crate::memory::paging::dump_ptes_range_serial(cr3, va_start, va_end);
    Ok(0)
}
