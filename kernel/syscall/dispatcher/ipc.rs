//! Syscall handlers for SYS_IPC_SEND and SYS_IPC_RECV.

use crate::process;
use crate::syscall::user_ptr::{self, SysErr};
use libs::{IPC_NONBLOCK, RwmMsg, SYSERR_AGAIN};

/// SYS_IPC_SEND — send a RwmMsg to target_pid.
///
/// Copies the message from user space, stamps sender_pid with the calling
/// process's actual PID (ignoring whatever the app wrote there), then enqueues
/// it in the target's ring buffer.
///
/// Returns `Ok(0)` on success.
/// Returns `Err(NOENT)` if target_pid is not live.
/// Returns `Err(NOMEM)` if the target's queue is full.
pub fn sys_ipc_send(
    target_pid: u32,
    msg_ptr: *const RwmMsg,
    _flags: u32,
) -> Result<u64, SysErr> {
    // Validate the user pointer.
    let cr3 = user_ptr::current_cr3()?;
    user_ptr::validate_user_range(
        cr3,
        msg_ptr as u64,
        core::mem::size_of::<RwmMsg>(),
        false,
    )?;

    // Copy the message out of user space.
    let mut msg: RwmMsg = unsafe { core::ptr::read(msg_ptr) };

    // Stamp sender_pid — the kernel is authoritative here.
    msg.sender_pid = process::current_pid().unwrap_or(0);

    // Find the target process slot.
    let target_idx = process::index_of_pid(target_pid).ok_or(SysErr::NOENT)?;

    // Enqueue; fail if queue is full.
    if !process::ipc_enqueue(target_idx, msg) {
        return Err(SysErr::NOMEM);
    }

    Ok(0)
}

/// SYS_IPC_RECV — dequeue the next RwmMsg for the calling process.
///
/// If the queue is empty and `IPC_NONBLOCK` is set in flags, returns
/// `SYSERR_AGAIN`.  (Blocking is not yet implemented; behaves like non-blocking.)
///
/// Returns `Ok(0)` and writes to `*out_ptr` on success.
pub fn sys_ipc_recv(out_ptr: *mut RwmMsg, flags: u32) -> Result<u64, SysErr> {
    // Validate the output pointer.
    let cr3 = user_ptr::current_cr3()?;
    user_ptr::validate_user_range(
        cr3,
        out_ptr as u64,
        core::mem::size_of::<RwmMsg>(),
        true,
    )?;

    let idx = process::current_index().ok_or(SysErr::INVAL)?;

    match process::ipc_dequeue(idx) {
        Some(msg) => {
            unsafe { core::ptr::write(out_ptr, msg) };
            Ok(0)
        }
        None => {
            if flags & IPC_NONBLOCK != 0 {
                Err(SysErr(SYSERR_AGAIN))
            } else {
                // Blocking recv: for now behave like non-blocking (no scheduler sleep yet).
                // TODO: set process state to Blocked and reschedule.
                Err(SysErr(SYSERR_AGAIN))
            }
        }
    }
}
