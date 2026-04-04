//! Process table, current process, kernel stack allocation.

use super::process::{ProcessDescriptor, Pid, MAX_PROCESSES};

// Use a const initializer instead of a literal so MAX_PROCESSES can change freely.
static mut PROCESS_TABLE: [Option<ProcessDescriptor>; MAX_PROCESSES] =
    [const { None }; MAX_PROCESSES];
static mut NEXT_PID: Pid = 1;
static mut CURRENT_PID: Option<Pid> = None;
static mut CURRENT_INDEX: Option<usize> = None;

const KERNEL_STACK_SIZE: usize = 32 * 1024;
static mut KERNEL_STACKS: [[u8; KERNEL_STACK_SIZE]; MAX_PROCESSES] =
    [[0; KERNEL_STACK_SIZE]; MAX_PROCESSES];
static mut KERNEL_STACK_USED: [bool; MAX_PROCESSES] = [false; MAX_PROCESSES];
const KSTACK_CANARY: u64 = 0xBADC0FFE_EE0DDF00;

/// Allocate a process slot and next pid. Returns (table_index, pid) or None.
pub(crate) fn allocate_process_slot() -> Option<(usize, Pid)> {
    unsafe {
        for (i, slot) in PROCESS_TABLE.iter().enumerate() {
            if slot.is_none() {
                let pid = NEXT_PID;
                NEXT_PID = NEXT_PID.wrapping_add(1);
                return Some((i, pid));
            }
        }
    }
    None
}

/// Store descriptor at index. Caller must have obtained index from allocate_process_slot.
pub(crate) fn put_descriptor(idx: usize, desc: ProcessDescriptor) {
    if idx >= MAX_PROCESSES {
        return;
    }
    unsafe {
        PROCESS_TABLE[idx] = Some(desc);
    }
}

/// Release slot: clear table entry and free kernel stack.
pub(crate) fn release_slot(idx: usize) {
    if idx >= MAX_PROCESSES {
        return;
    }
    unsafe {
        KERNEL_STACK_USED[idx] = false;
        PROCESS_TABLE[idx] = None;
    }
}

/// Reap one dead process. pid: 0 or u32::MAX = any dead; else reap that pid if dead.
/// Returns (reaped_pid, exit_status) and frees the slot, or None if no matching dead process.
pub fn reap_dead(pid: u32) -> Option<(Pid, Option<i32>)> {
    unsafe {
        let any = pid == 0 || pid == u32::MAX;
        for i in 0..MAX_PROCESSES {
            if let Some(ref p) = PROCESS_TABLE[i] {
                if p.state != super::process::ProcessState::Dead {
                    continue;
                }
                if !any && p.pid != pid {
                    continue;
                }
                let reaped_pid = p.pid;
                let status = p.exit_status;
                release_slot(i);
                return Some((reaped_pid, status));
            }
        }
    }
    None
}

/// Set the current running process (index). None when no process is running.
pub(crate) fn set_current(idx: Option<usize>) {
    unsafe {
        CURRENT_INDEX = idx;
        CURRENT_PID = idx.and_then(|i| PROCESS_TABLE[i].as_ref().map(|p| p.pid));
    }
}

/// Allocate kernel stack for the given table index. Returns stack top address or None.
pub(crate) fn alloc_kernel_stack(idx: usize) -> Option<u64> {
    if idx >= MAX_PROCESSES {
        return None;
    }
    unsafe {
        if KERNEL_STACK_USED[idx] {
            return None;
        }
        KERNEL_STACK_USED[idx] = true;
        let canary_ptr = KERNEL_STACKS[idx].as_mut_ptr() as *mut u64;
        core::ptr::write_volatile(canary_ptr, KSTACK_CANARY);
        let top = KERNEL_STACKS[idx].as_ptr().add(KERNEL_STACK_SIZE) as u64;
        Some(top)
    }
}

#[inline]
pub fn current_pid() -> Option<Pid> {
    unsafe { CURRENT_PID }
}

#[inline]
pub fn current_pid_for_fault() -> u64 {
    current_pid().unwrap_or(0) as u64
}

#[inline]
pub fn current_index() -> Option<usize> {
    unsafe { CURRENT_INDEX }
}

pub fn get_descriptor(index: usize) -> Option<&'static ProcessDescriptor> {
    if index >= MAX_PROCESSES {
        return None;
    }
    unsafe { PROCESS_TABLE[index].as_ref() }
}

pub fn get_descriptor_mut(index: usize) -> Option<&'static mut ProcessDescriptor> {
    if index >= MAX_PROCESSES {
        return None;
    }
    unsafe { PROCESS_TABLE[index].as_mut() }
}

pub fn current_descriptor() -> Option<&'static ProcessDescriptor> {
    current_index().and_then(get_descriptor)
}

/// Find the table index of the process with the given pid, or None.
pub fn index_of_pid(pid: Pid) -> Option<usize> {
    unsafe {
        for i in 0..MAX_PROCESSES {
            if let Some(ref p) = PROCESS_TABLE[i] {
                if p.pid == pid {
                    return Some(i);
                }
            }
        }
    }
    None
}

/// Wake any process blocked in sys_waitpid waiting for `dead_pid` (or any child if u32::MAX).
/// Sets the waiter Runnable and re-enqueues it. Called from exit_current_and_schedule.
pub fn wake_waiters_for(dead_pid: Pid) {
    unsafe {
        for i in 0..MAX_PROCESSES {
            if let Some(ref mut p) = PROCESS_TABLE[i] {
                if p.state != super::process::ProcessState::Blocked {
                    continue;
                }
                let matches = match p.waiting_for {
                    None => false,
                    Some(w) => w == dead_pid || w == u32::MAX,
                };
                if matches {
                    p.state = super::process::ProcessState::Runnable;
                    p.waiting_for = None;
                    super::scheduler::enqueue_runqueue(i);
                }
            }
        }
    }
}

/// Check canary at bottom of current process kernel stack. Halt on corruption.
pub(crate) fn check_kernel_stack_canary() {
    let idx = unsafe { CURRENT_INDEX };
    let Some(i) = idx else { return };
    if i >= MAX_PROCESSES {
        return;
    }
    unsafe {
        let canary_ptr = KERNEL_STACKS[i].as_ptr() as *const u64;
        let c = core::ptr::read_volatile(canary_ptr);
        if c != KSTACK_CANARY {
            crate::kernel::diagnostic::diagnostic_halt("kernel_stack_canary_corrupt");
        }
    }
}

pub(crate) fn dump_table_serial() {
    unsafe {
        crate::arch::x86_64::serial::write_fmt(format_args!(
            "[DIAG][PROC] current_pid={:?} current_index={:?}\r\n",
            CURRENT_PID,
            CURRENT_INDEX
        ));
        crate::arch::x86_64::serial::write_str("[DIAG][PROC] process table:\r\n");
        for i in 0..MAX_PROCESSES {
            if let Some(ref p) = PROCESS_TABLE[i] {
                crate::arch::x86_64::serial::write_fmt(format_args!(
                    "  idx={} pid={} state={:?} prio={} cr3=",
                    i, p.pid, p.state, p.priority
                ));
                crate::arch::x86_64::serial::write_hex(p.cr3);
                crate::arch::x86_64::serial::write_str(" kstack=");
                crate::arch::x86_64::serial::write_hex(p.kernel_stack_top);
                crate::arch::x86_64::serial::write_str("\r\n");
            }
        }
    }
}
