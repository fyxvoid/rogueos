//! Process syscalls: exit, spawn, get_proc_info, getpid, waitpid.

use crate::process::ProcessState;
use crate::syscall::user_ptr::{self, SysErr};
use libs::{ProcInfo, WNOHANG};

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
    let wnohang = (options & WNOHANG) != 0;

    // Try to reap immediately.
    if let Some((reaped_pid, status)) = crate::process::reap_dead(pid) {
        if !status_ptr.is_null() {
            let cr3 = user_ptr::current_cr3()?;
            user_ptr::validate_user_range(cr3, status_ptr as u64, 4, true)?;
            unsafe { core::ptr::write(status_ptr, status.unwrap_or(-1)); }
        }
        return Ok(reaped_pid as u64);
    }

    if wnohang {
        return Err(SysErr::AGAIN);
    }

    // Blocking waitpid: save user context into PCB so enter_user (IRETQ) can resume the
    // caller. We set trap_frame.rip two bytes BEFORE the user return address so that when
    // the process is rescheduled it re-executes the SYSCALL instruction and retries waitpid,
    // this time finding the dead child and returning normally.
    let current_idx = crate::process::current_index().ok_or(SysErr::INVAL)?;
    {
        let user_rip = crate::arch::x86_64::syscall_entry::get_user_rip();
        let user_rflags = crate::arch::x86_64::syscall_entry::get_user_rflags();
        let user_rsp = crate::arch::x86_64::syscall_entry::get_user_rsp();
        let pcb = crate::process::get_descriptor_mut(current_idx).ok_or(SysErr::INVAL)?;
        // Restart semantics: re-execute SYSCALL (2 bytes: 0x0F 0x05) on resume.
        pcb.trap_frame.rip = user_rip.saturating_sub(2);
        pcb.trap_frame.rflags = user_rflags | 0x200; // ensure IF set
        pcb.trap_frame.rsp = user_rsp;
        pcb.trap_frame.cs = (crate::arch::x86_64::gdt::USER_CS | 3) as u64;
        pcb.trap_frame.ss = (crate::arch::x86_64::gdt::USER_SS | 3) as u64;
        pcb.state = ProcessState::Blocked;
        pcb.waiting_for = Some(pid);
    }
    crate::process::scheduler::remove_from_runqueue(current_idx);
    crate::process::pid::set_current(None);

    // Run next runnable process.
    if let Some(next_idx) = crate::process::scheduler::dequeue_runqueue() {
        crate::process::run_first_process(next_idx);
    }
    // No other processes: spin until something wakes us (should not happen in normal use).
    loop { unsafe { core::arch::asm!("hlt"); } }
}
