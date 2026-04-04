//! Syscall dispatch: read, write, exit, file (open/close/lseek/unlink/fsync),
//! graphics + input. fd 0/1/2 = TTY; fd >= 3 = VFS.
//! Entry is SYSCALL only (arch::x86_64::msr::init_syscall_msrs).

mod debug;
mod gfx;
mod io;
mod ipc;
mod misc;
mod perf;
mod process;

use crate::syscall::user_ptr::{self, SysErr};
use libs::{
    KeyEvent, RwmMsg, MouseEvent, ProcInfo,
    SYS_CLOSE, SYS_DEBUG_DUMP_PTES, SYS_EXIT,
    SYS_FB_BLIT, SYS_FB_CLEAR, SYS_FB_FILL_RECT, SYS_FB_FLUSH,
    SYS_FSYNC, SYS_GETPID, SYS_GET_PROC_INFO, SYS_IPC_RECV, SYS_IPC_SEND,
    SYS_LIST_ROOT, SYS_LSEEK, SYS_OPEN, SYS_POLL_INPUT, SYS_POLL_MOUSE,
    SYS_READ, SYS_REBOOT, SYS_SCREEN_SIZE, SYS_SPAWN,
    SYS_CLAIM_COMPOSITOR, SYS_COMPOSITE_ALL, SYS_GET_COMPOSITOR_PID,
    SYS_SURFACE_ATTACH, SYS_SURFACE_COMMIT, SYS_SURFACE_CREATE, SYS_SURFACE_DESTROY,
    SYS_UNLINK, SYS_WAITPID, SYS_WRITE,
    // Debug / perf / scheduler
    SYS_HW_BP_SET, SYS_HW_BP_CLEAR, SYS_HW_BP_QUERY,
    SYS_PERF_OPEN, SYS_PERF_READ, SYS_PERF_CLOSE,
    SYS_SET_NICE,
};

static mut SYSCALL_HEARTBEAT: u64 = 0;

/// Framebuffer ownership: only the first writer (Director) should write; others log warning.
static mut FB_OWNER_PID: Option<u32> = None;
static mut FB_OTHER_WRITE_COUNT: u32 = 0;

/// Dispatch SYSCALL. Args in rdi, rsi, rdx, r10, r8, r9; number in rax; return in rax.
#[no_mangle]
pub extern "C" fn syscall_dispatch(
    num: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
    _a6: u64,
) -> u64 {
    unsafe {
        SYSCALL_HEARTBEAT = SYSCALL_HEARTBEAT.wrapping_add(1);
        if SYSCALL_HEARTBEAT % 1000 == 0 {
            crate::arch::serial::write_str("[syscall] count=");
            crate::arch::serial::write_hex(SYSCALL_HEARTBEAT);
            crate::arch::serial::write_str(" (ring3 round-trip ok)\r\n");
        }
    }
    if let Some(d) = crate::process::current_descriptor() {
        if d.canary != crate::process::PROCESS_CANARY {
            crate::kernel::diagnostic::diagnostic_halt("process_descriptor_canary_corrupt");
        }
    }
    crate::process::check_current_kernel_stack_canary();
    match num {
        SYS_READ => user_ptr::result_to_rax(io::sys_read(a1 as u32, a2 as *mut u8, a3 as usize), |v| v as u64, |e| e.0),
        SYS_WRITE => user_ptr::result_to_rax(io::sys_write(a1 as u32, a2 as *const u8, a3 as usize), |v| v as u64, |e| e.0),
        SYS_OPEN => user_ptr::result_to_rax(io::sys_open(a1 as *const u8, a2 as usize, a3 as u32), |v| v as u64, |e| e.0),
        SYS_CLOSE => user_ptr::result_to_rax(io::sys_close(a1 as u32), |v| v as u64, |e| e.0),
        SYS_LSEEK => user_ptr::result_to_rax(io::sys_lseek(a1 as u32, a2 as i64, a3 as u32), |v| v as u64, |e| e.0),
        SYS_UNLINK => user_ptr::result_to_rax(io::sys_unlink(a1 as *const u8, a2 as usize), |v| v as u64, |e| e.0),
        SYS_FSYNC => user_ptr::result_to_rax(io::sys_fsync(a1 as u32), |v| v as u64, |e| e.0),
        SYS_LIST_ROOT => user_ptr::result_to_rax(io::sys_list_root(a1 as *mut u8, a2 as usize), |v| v as u64, |e| e.0),
        SYS_REBOOT => user_ptr::result_to_rax(misc::sys_reboot(a1 as u32), |v| v as u64, |e| e.0),
        SYS_EXIT => process::sys_exit(a1 as i32),
        SYS_POLL_INPUT => user_ptr::result_to_rax(gfx::sys_poll_input(a1 as *mut KeyEvent), |v| v as u64, |e| e.0),
        SYS_POLL_MOUSE => user_ptr::result_to_rax(gfx::sys_poll_mouse(a1 as *mut MouseEvent), |v| v as u64, |e| e.0),
        SYS_FB_CLEAR => {
            check_fb_owner();
            user_ptr::result_to_rax(gfx::sys_fb_clear(a1 as u32), |v| v as u64, |e| e.0)
        }
        SYS_FB_FILL_RECT => {
            check_fb_owner();
            user_ptr::result_to_rax(gfx::sys_fb_fill_rect(a1 as u32, a2 as u32, a3 as u32, a4 as u32, a5 as u32), |v| v as u64, |e| e.0)
        }
        SYS_FB_FLUSH => {
            check_fb_owner();
            user_ptr::result_to_rax(gfx::sys_fb_flush(), |v| v as u64, |e| e.0)
        }
        SYS_DEBUG_DUMP_PTES => user_ptr::result_to_rax(misc::sys_debug_dump_ptes(a1, a2, a3), |v| v as u64, |e| e.0),
        SYS_SPAWN => user_ptr::result_to_rax(process::sys_spawn(a1), |v| v as u64, |e| e.0),
        SYS_GET_PROC_INFO => user_ptr::result_to_rax(process::sys_get_proc_info(a1 as *mut ProcInfo, a2 as u32), |v| v as u64, |e| e.0),
        SYS_GETPID => user_ptr::result_to_rax(process::sys_getpid(), |v| v as u64, |e| e.0),
        SYS_WAITPID => user_ptr::result_to_rax(
            process::sys_waitpid(a1 as u32, a2 as *mut i32, a3 as u32),
            |v| v as u64,
            |e| e.0,
        ),
        // ── Surface protocol ─────────────────────────────────────────────
        SYS_SURFACE_CREATE => user_ptr::result_to_rax(
            gfx::sys_surface_create(), |v| v, |e| e.0),
        SYS_SURFACE_DESTROY => user_ptr::result_to_rax(
            gfx::sys_surface_destroy(a1 as u32), |v| v, |e| e.0),
        SYS_SURFACE_ATTACH => user_ptr::result_to_rax(
            gfx::sys_surface_attach(a1 as u32, a2 as *const u8, a3 as u32, a4 as u32, a5 as u32),
            |v| v, |e| e.0),
        SYS_SURFACE_COMMIT => user_ptr::result_to_rax(
            gfx::sys_surface_commit(a1 as u32, a2 as u32, a3 as u32), |v| v, |e| e.0),
        SYS_SCREEN_SIZE => user_ptr::result_to_rax(
            gfx::sys_screen_size(a1 as *mut u32, a2 as *mut u32), |v| v, |e| e.0),
        SYS_FB_BLIT => user_ptr::result_to_rax(
            gfx::sys_fb_blit(a1 as u32, a2 as u32, a3 as u32, a4 as u32, a5 as u32, _a6 as *const u8),
            |v| v, |e| e.0),
        // ── RDP compositor control ────────────────────────────────────────
        SYS_CLAIM_COMPOSITOR => user_ptr::result_to_rax(
            gfx::sys_claim_compositor(), |v| v, |e| e.0),
        SYS_COMPOSITE_ALL => user_ptr::result_to_rax(
            gfx::sys_composite_all(), |v| v, |e| e.0),
        SYS_GET_COMPOSITOR_PID => user_ptr::result_to_rax(
            gfx::sys_get_compositor_pid(), |v| v, |e| e.0),

        // ── IPC protocol ─────────────────────────────────────────────────
        SYS_IPC_SEND => user_ptr::result_to_rax(
            ipc::sys_ipc_send(a1 as u32, a2 as *const RwmMsg, a3 as u32),
            |v| v, |e| e.0),
        SYS_IPC_RECV => user_ptr::result_to_rax(
            ipc::sys_ipc_recv(a1 as *mut RwmMsg, a2 as u32),
            |v| v, |e| e.0),

        // ── Hardware breakpoints (pentester/debugger primitives) ──────────
        SYS_HW_BP_SET => user_ptr::result_to_rax(
            debug::sys_hw_bp_set(a1, a2, a3, a4), |v| v, |e| e.0),
        SYS_HW_BP_CLEAR => user_ptr::result_to_rax(
            debug::sys_hw_bp_clear(a1), |v| v, |e| e.0),
        SYS_HW_BP_QUERY => user_ptr::result_to_rax(
            debug::sys_hw_bp_query(a1 as *mut debug::HwBpInfo), |v| v, |e| e.0),

        // ── AMD PMU perf-counter telemetry ────────────────────────────────
        SYS_PERF_OPEN => user_ptr::result_to_rax(
            perf::sys_perf_open(a1), |v| v, |e| e.0),
        SYS_PERF_READ => user_ptr::result_to_rax(
            perf::sys_perf_read(a1, a2 as *mut u64), |v| v, |e| e.0),
        SYS_PERF_CLOSE => user_ptr::result_to_rax(
            perf::sys_perf_close(a1), |v| v, |e| e.0),

        // ── Scheduler control ─────────────────────────────────────────────
        SYS_SET_NICE => {
            let nice = a1 as i64;
            if nice < -20 || nice > 19 {
                SysErr::INVAL.0 as u64
            } else {
                if let Some(idx) = crate::process::current_index() {
                    crate::process::set_nice_for_current(idx, nice as i8);
                    if let Some(pcb) = crate::process::get_descriptor_mut(idx) {
                        pcb.nice = nice as i8;
                    }
                }
                0
            }
        }

        _ => SysErr::INVAL.0 as u64,
    }
}

/// Copy path from user into static BUF; return name-only slice. len 0 = null-terminated. Validates range first.
pub(crate) fn copy_path_from_user(ptr: *const u8, len: usize) -> Result<&'static [u8], SysErr> {
    const PATH_MAX: usize = 256;
    static mut BUF: [u8; PATH_MAX] = [0; PATH_MAX];
    let cr3 = user_ptr::current_cr3()?;
    let copy_len = if len == 0 { PATH_MAX } else { core::cmp::min(len, PATH_MAX) };
    user_ptr::validate_user_range(cr3, ptr as u64, copy_len, false)?;
    if ptr.is_null() {
        return Err(SysErr::INVAL);
    }
    let n = unsafe {
        if len == 0 {
            let mut i = 0usize;
            while i < PATH_MAX {
                let b = *ptr.add(i);
                BUF[i] = b;
                if b == 0 {
                    break;
                }
                i += 1;
            }
            i
        } else {
            let n = core::cmp::min(len, PATH_MAX);
            core::ptr::copy_nonoverlapping(ptr, BUF.as_mut_ptr(), n);
            n
        }
    };
    if n == 0 || n >= PATH_MAX {
        return Err(SysErr::INVAL);
    }
    let (start, name_len) = unsafe {
        let mut end = n;
        while end > 0 && (BUF[end - 1] == b'/' || BUF[end - 1] == 0) {
            end -= 1;
        }
        let start = (0..end).rev().find(|&i| BUF[i] == b'/').map(|p| p + 1).unwrap_or(0);
        let nl = end.saturating_sub(start);
        (start, nl)
    };
    if name_len == 0 || name_len > 31 {
        return Err(SysErr::INVAL);
    }
    unsafe { Ok(core::slice::from_raw_parts(BUF.as_ptr().add(start), name_len)) }
}

/// If more than one subsystem (pid) writes to framebuffer, log warning.
fn check_fb_owner() {
    let pid = crate::process::current_pid().unwrap_or(0);
    unsafe {
        match FB_OWNER_PID {
            None => FB_OWNER_PID = Some(pid),
            Some(owner) if owner != pid => {
                FB_OTHER_WRITE_COUNT = FB_OTHER_WRITE_COUNT.saturating_add(1);
                crate::arch::serial::write_str("[FB] warning: pid ");
                crate::arch::serial::write_hex(pid as u64);
                crate::arch::serial::write_str(" wrote; owner is ");
                crate::arch::serial::write_hex(owner as u64);
                crate::arch::serial::write_str(" (count=");
                crate::arch::serial::write_hex(FB_OTHER_WRITE_COUNT as u64);
                crate::arch::serial::write_str(")\r\n");
            }
            _ => {}
        }
    }
}
