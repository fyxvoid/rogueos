//! Syscall handlers for information flow control:
//! SYS_IFLOW_GET, SYS_IFLOW_TAINT, SYS_IFLOW_DECLASSIFY, SYS_IFLOW_ENDORSE.

use crate::iflow;
use crate::syscall::user_ptr::SysErr;

pub fn sys_iflow_get(pid: u32, out_sec: *mut u64, out_int: *mut u64) -> Result<u64, SysErr> {
    iflow::sys_iflow_get(pid, out_sec, out_int)
}

pub fn sys_iflow_taint(add_secrecy: u64, remove_integrity: u64) -> Result<u64, SysErr> {
    iflow::sys_iflow_taint(add_secrecy, remove_integrity)
}

pub fn sys_iflow_declassify(remove_secrecy: u64) -> Result<u64, SysErr> {
    iflow::sys_iflow_declassify(remove_secrecy)
}

pub fn sys_iflow_endorse(target_pid: u32, add_integrity: u64) -> Result<u64, SysErr> {
    iflow::sys_iflow_endorse(target_pid, add_integrity)
}
