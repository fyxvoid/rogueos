//! Process syscalls: exit, spawn, get_proc_info, getpid, waitpid.

use crate::syscall::user_ptr::{self, SysErr};
use libs::ProcInfo;

pub(super) fn sys_exit(status: i32) -> ! {
    crate::process::exit_current_and_schedule(Some(status));
}

pub(super) fn sys_spawn(program_id: u64) -> Result<u64, SysErr> {
    match crate::process::spawn_by_program_id(program_id as u32) {
        Some(pid) => Ok(pid as u64),
        None => Err(SysErr::NOENT),
    }
}

pub(super) fn sys_get_proc_info(buf: *mut ProcInfo, capacity: u32) -> Result<u64, SysErr> {
    if buf.is_null() || capacity == 0 {
        return Err(SysErr::INVAL);
    }
    let cap = capacity as usize;
    let cr3 = user_ptr::current_cr3()?;
    let size = cap * core::mem::size_of::<ProcInfo>();
    user_ptr::validate_user_range(cr3, buf as u64, size, true)?;
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, cap) };
    Ok(crate::process::get_proc_info_snapshot(slice) as u64)
}

pub(super) fn sys_getpid() -> Result<u64, SysErr> {
    crate::process::current_pid().map(|p| p as u64).ok_or(SysErr::INVAL)
}

pub(super) fn sys_waitpid(pid: u32, status_ptr: *mut i32, options: u32) -> Result<u64, SysErr> {
    // WNOHANG (0x01): if no dead process available, return SYSERR_AGAIN instead of SYSERR_INVAL.
    let wnohang = (options & libs::WNOHANG) != 0;
    if let Some((reaped_pid, status)) = crate::process::reap_dead(pid) {
        if !status_ptr.is_null() {
            let cr3 = user_ptr::current_cr3()?;
            user_ptr::validate_user_range(cr3, status_ptr as u64, 4, true)?;
            unsafe { core::ptr::write(status_ptr, status.unwrap_or(-1)); }
        }
        Ok(reaped_pid as u64)
    } else if wnohang {
        Err(SysErr::AGAIN)
    } else {
        Err(SysErr::INVAL)
    }
}
