//! Syscall wrappers for kernel ABI (SYSCALL). rax = syscall number,
//! rdi,rsi,rdx,r10,r8,r9 = args (4th in r10; rcx overwritten by SYSCALL), return in rax. See libs for numbering.

#![no_std]
#![cfg_attr(not(test), deny(warnings))]

pub mod backend_kernel;
/// RDP (Rogue Display Protocol) client library for graphical applications.
pub mod rdp;

use libs::{
    KeyEvent, MouseEvent, RwmMsg, ProcInfo,
    SYS_CLAIM_COMPOSITOR, SYS_CLOSE, SYS_COMPOSITE_ALL, SYS_EXIT,
    SYS_FB_BLIT, SYS_FB_CLEAR, SYS_FB_FILL_RECT, SYS_FB_FLUSH,
    SYS_FSYNC, SYS_GETPID, SYS_GET_COMPOSITOR_PID, SYS_GET_PROC_INFO,
    SYS_IPC_RECV, SYS_IPC_SEND,
    SYS_LIST_ROOT, SYS_LSEEK, SYS_OPEN, SYS_POLL_INPUT, SYS_POLL_MOUSE,
    SYS_READ, SYS_REBOOT,
    SYS_SCREEN_SIZE, SYS_SPAWN,
    SYS_SURFACE_ATTACH, SYS_SURFACE_COMMIT, SYS_SURFACE_CREATE, SYS_SURFACE_DESTROY,
    SYS_SURFACE_SET_Z, SYS_SHM_CREATE, SYS_SHM_DESTROY, SYS_MAP_FRAMEBUFFER,
    SYS_UNLINK, SYS_WAITPID, SYS_WRITE,
    SYS_GETTIME,
    SYS_CAP_GRANT, SYS_CAP_REVOKE, SYS_CAP_QUERY,
    SYS_JOURNAL_WRITE, SYS_JOURNAL_READ,
    SYS_IFLOW_GET, SYS_IFLOW_TAINT, SYS_IFLOW_DECLASSIFY, SYS_IFLOW_ENDORSE,
};

#[cfg(not(test))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// Read from fd into buf, return bytes read or negative errno.
#[inline(always)]
pub fn sys_read(fd: u32, buf: *mut u8, len: usize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_READ,
            in("rdi") fd as u64,
            in("rsi") buf as u64,
            in("rdx") len as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Write from buf to fd, return bytes written or negative errno.
#[inline(always)]
pub fn sys_write(fd: u32, buf: *const u8, len: usize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_WRITE,
            in("rdi") fd as u64,
            in("rsi") buf as u64,
            in("rdx") len as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Exit process with status. Never returns.
#[inline(always)]
pub fn sys_exit(status: i32) -> ! {
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_EXIT,
            in("rdi") status as u64,
            options(nostack, noreturn)
        );
    }
}

/// Poll for a single keyboard event.
///
/// On success, returns:
/// - `> 0` if an event was written to `ev`
/// - `0` if no event is currently available
/// - `< 0` negative errno on error
#[inline(always)]
pub fn sys_poll_input(ev: &mut KeyEvent) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_POLL_INPUT,
            in("rdi") ev as *mut KeyEvent as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Poll for a single mouse event.
///
/// Returns `> 0` if `ev` was filled, `0` if no event, `< 0` on error.
#[inline(always)]
pub fn sys_poll_mouse(ev: &mut MouseEvent) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_POLL_MOUSE,
            in("rdi") ev as *mut MouseEvent as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Read the real-time clock. Returns packed u64:
/// bits[55:40]=year, [39:32]=month, [31:24]=day, [23:16]=hour, [15:8]=minute, [7:0]=second.
/// On error (should not happen), returns a negative isize cast to u64.
#[inline(always)]
pub fn sys_gettime() -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_GETTIME,
            in("rdi") 0u64, // no output pointer; return value in rax
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Clear the entire framebuffer to `color` (X8R8G8B8).
#[inline(always)]
pub fn sys_fb_clear(color: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_FB_CLEAR,
            in("rdi") color as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Fill a rectangle on the framebuffer.
#[inline(always)]
pub fn sys_fb_fill_rect(x: u32, y: u32, w: u32, h: u32, color: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_FB_FILL_RECT,
            in("rdi") x as u64,
            in("rsi") y as u64,
            in("rdx") w as u64,
            in("r10") h as u64,
            in("r8") color as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Flush any pending drawing to the hardware framebuffer.
#[inline(always)]
pub fn sys_fb_flush() -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_FB_FLUSH,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Open file by path (null-terminated or path_len). Flags: O_RDONLY etc. Returns fd or negative errno.
#[inline(always)]
pub fn sys_open(path: *const u8, path_len: usize, flags: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_OPEN,
            in("rdi") path as u64,
            in("rsi") path_len as u64,
            in("rdx") flags as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Close fd. Returns 0 or negative errno.
#[inline(always)]
pub fn sys_close(fd: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_CLOSE,
            in("rdi") fd as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Seek on fd. Whence: SEEK_SET, SEEK_CUR, SEEK_END. Returns new offset or negative errno.
#[inline(always)]
pub fn sys_lseek(fd: u32, offset: i64, whence: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_LSEEK,
            in("rdi") fd as u64,
            in("rsi") offset as u64,
            in("rdx") whence as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Unlink file by path. Returns 0 or negative errno.
#[inline(always)]
pub fn sys_unlink(path: *const u8, path_len: usize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_UNLINK,
            in("rdi") path as u64,
            in("rsi") path_len as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Sync file to disk. Returns 0 or negative errno.
#[inline(always)]
pub fn sys_fsync(fd: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_FSYNC,
            in("rdi") fd as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Spawn process by program_id with full capability inheritance (backwards-compat).
#[inline(always)]
pub fn sys_spawn(program_id: u32) -> isize {
    sys_spawn_capped(program_id, 0) // 0 = inherit all parent caps
}

/// Spawn a process and restrict its capabilities to `cap_mask & parent_caps`.
/// Use `libs::cap::*` constants to build the mask.
pub fn sys_spawn_capped(program_id: u32, cap_mask: u64) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_SPAWN,
            in("rdi") program_id as u64,
            in("rsi") cap_mask,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Grant capability bits to a process. Requires CAP_GRANT.
pub fn sys_cap_grant(target_pid: u32, cap_bits: u64) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_CAP_GRANT,
            in("rdi") target_pid as u64,
            in("rsi") cap_bits,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Revoke capability bits from a process. Requires CAP_GRANT.
pub fn sys_cap_revoke(target_pid: u32, cap_bits: u64) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_CAP_REVOKE,
            in("rdi") target_pid as u64,
            in("rsi") cap_bits,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Query own capability bitmask. Returns the bitmask (cast to isize; treat as u64).
pub fn sys_cap_query() -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_CAP_QUERY,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Write bytes to the Cogman restart journal. Requires CAP_JOURNAL.
pub fn sys_journal_write(data: &[u8]) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_JOURNAL_WRITE,
            in("rdi") data.as_ptr() as u64,
            in("rsi") data.len() as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Read the Cogman restart journal into `buf`. Returns bytes read. Requires CAP_JOURNAL.
pub fn sys_journal_read(buf: &mut [u8]) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_JOURNAL_READ,
            in("rdi") buf.as_mut_ptr() as u64,
            in("rsi") buf.len() as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Query the IFC label of a process. Returns 0 on success or negative errno.
#[inline(always)]
pub fn sys_iflow_get(pid: u32, out_secrecy: &mut u64, out_integrity: &mut u64) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_IFLOW_GET,
            in("rdi") pid as u64,
            in("rsi") out_secrecy as *mut u64 as u64,
            in("rdx") out_integrity as *mut u64 as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Raise own secrecy tags / lower own integrity tags. Always permitted.
#[inline(always)]
pub fn sys_iflow_taint(add_secrecy: u64, remove_integrity: u64) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_IFLOW_TAINT,
            in("rdi") add_secrecy,
            in("rsi") remove_integrity,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Lower own secrecy (declassify). Requires CAP_DECLASSIFY.
#[inline(always)]
pub fn sys_iflow_declassify(remove_secrecy: u64) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_IFLOW_DECLASSIFY,
            in("rdi") remove_secrecy,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Raise integrity of a target process (endorse). Requires CAP_ENDORSE.
#[inline(always)]
pub fn sys_iflow_endorse(target_pid: u32, add_integrity: u64) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_IFLOW_ENDORSE,
            in("rdi") target_pid as u64,
            in("rsi") add_integrity,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Get process table snapshot. Returns count filled or negative errno.
#[inline(always)]
pub fn sys_get_proc_info(buf: *mut ProcInfo, capacity: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_GET_PROC_INFO,
            in("rdi") buf as u64,
            in("rsi") capacity as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// List root directory into buf. Returns bytes written or negative errno.
#[inline(always)]
pub fn sys_list_root(buf: *mut u8, capacity: usize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_LIST_ROOT,
            in("rdi") buf as u64,
            in("rsi") capacity as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Get current process ID. Returns pid or negative errno.
#[inline(always)]
pub fn sys_getpid() -> u32 {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_GETPID,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    if ret < 0 {
        0
    } else {
        ret as u32
    }
}

/// Reap a dead process. pid: 0 or u32::MAX = any; status_ptr: optional pointer to write exit status; options: 0.
/// Returns reaped pid or negative errno.
#[inline(always)]
pub fn sys_waitpid(pid: u32, status_ptr: *mut i32, options: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_WAITPID,
            in("rdi") pid as u64,
            in("rsi") status_ptr as u64,
            in("rdx") options as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

// ── Surface protocol syscalls ─────────────────────────────────────────────

/// Create a new display surface.  Returns the surface ID (> 0) or negative errno.
#[inline(always)]
pub fn sys_surface_create() -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_SURFACE_CREATE,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Destroy a surface by ID.
#[inline(always)]
pub fn sys_surface_destroy(id: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_SURFACE_DESTROY,
            in("rdi") id as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Attach a 32bpp ARGB pixel buffer to a surface.
/// `stride` is in bytes (must be >= width * 4).
#[inline(always)]
pub fn sys_surface_attach(id: u32, ptr: *const u8, width: u32, height: u32, stride: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_SURFACE_ATTACH,
            in("rdi") id as u64,
            in("rsi") ptr as u64,
            in("rdx") width as u64,
            in("r10") height as u64,
            in("r8")  stride as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Commit (blit) a surface to the framebuffer at `(dst_x, dst_y)`.
#[inline(always)]
pub fn sys_surface_commit(id: u32, dst_x: u32, dst_y: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_SURFACE_COMMIT,
            in("rdi") id as u64,
            in("rsi") dst_x as u64,
            in("rdx") dst_y as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Query the screen size.  Returns 0 on success; `w` and `h` are filled.
#[inline(always)]
pub fn sys_screen_size(w: &mut u32, h: &mut u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_SCREEN_SIZE,
            in("rdi") w as *mut u32 as u64,
            in("rsi") h as *mut u32 as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Blit a raw 32bpp buffer to the framebuffer at `(dst_x, dst_y)`.
#[inline(always)]
pub fn sys_fb_blit(dst_x: u32, dst_y: u32, w: u32, h: u32, stride: u32, ptr: *const u8) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_FB_BLIT,
            in("rdi") dst_x as u64,
            in("rsi") dst_y as u64,
            in("rdx") w as u64,
            in("r10") h as u64,
            in("r8")  stride as u64,
            in("r9")  ptr as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

// ── IPC syscalls ─────────────────────────────────────────────────────────

/// Send a RwmMsg to `target_pid`.
/// `flags`: 0 for blocking (blocks until queue has space), or IPC_NONBLOCK.
/// Returns 0 on success, negative errno on error.
#[inline(always)]
pub fn sys_ipc_send(target_pid: u32, msg: &RwmMsg, flags: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_IPC_SEND,
            in("rdi") target_pid as u64,
            in("rsi") msg as *const RwmMsg as u64,
            in("rdx") flags as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Receive the next RwmMsg for the calling process.
/// `flags`: 0 = block until a message arrives, IPC_NONBLOCK = return SYSERR_AGAIN immediately.
/// Returns 0 on success (msg is filled), negative errno otherwise.
#[inline(always)]
pub fn sys_ipc_recv(out: &mut RwmMsg, flags: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_IPC_RECV,
            in("rdi") out as *mut RwmMsg as u64,
            in("rsi") flags as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Claim compositor authority (RDP). Only the first caller succeeds.
/// Returns 0 on success, negative errno if already claimed.
#[inline(always)]
pub fn sys_claim_compositor() -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_CLAIM_COMPOSITOR,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Composite all surfaces in z-order and flush to hardware.
/// Only the registered compositor may call. Returns 0 or negative errno.
#[inline(always)]
pub fn sys_composite_all() -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_COMPOSITE_ALL,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Set the z-order for a surface (lower = further back; 255 = topmost).
#[inline(always)]
pub fn sys_surface_set_z(id: u32, z: u8) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_SURFACE_SET_Z,
            in("rdi") id as u64,
            in("rsi") z as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Create a shared memory region of `size` bytes.
/// Returns packed (shm_id << 32 | va_u32) as isize, or negative errno.
/// Decode: `shm_id = (ret as u64 >> 32) as u32`, `ptr = (ret as u64 & 0xFFFF_FFFF) as *mut u8`.
#[inline(always)]
pub fn sys_shm_create(size: usize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_SHM_CREATE,
            in("rdi") size as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Destroy a shared memory region by shm_id.
#[inline(always)]
pub fn sys_shm_destroy(shm_id: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_SHM_DESTROY,
            in("rdi") shm_id as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Map the compositor backbuffer into the calling process's address space (Option B).
/// Caller must have previously called sys_claim_compositor(). On success writes the
/// backbuffer VA, width, height, and stride (bytes) into the four output slots and
/// returns 0. Returns negative errno on failure.
#[inline(always)]
pub fn sys_map_framebuffer(
    out_ptr: &mut u64,
    out_w: &mut u32,
    out_h: &mut u32,
    out_stride: &mut u32,
) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_MAP_FRAMEBUFFER,
            in("rdi") out_ptr as *mut u64 as u64,
            in("rsi") out_w   as *mut u32 as u64,
            in("rdx") out_h   as *mut u32 as u64,
            in("r10") out_stride as *mut u32 as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Get the PID of the registered RDP compositor. Returns pid or negative errno.
#[inline(always)]
pub fn sys_get_compositor_pid() -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_GET_COMPOSITOR_PID,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Reboot/halt. mode: 0=halt, 1=reboot. Returns 0 or negative errno.
#[inline(always)]
pub fn sys_reboot(mode: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_REBOOT,
            in("rdi") mode as u64,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

// ── Global bump allocator — enables `extern crate alloc` in all userland binaries ──

extern crate alloc;

/// 1 MiB static heap — plenty for WM state (clients, monitors, layouts, strings).
/// Zero-initialised so the linker places it in .bss (no ELF size inflation).
const HEAP_SIZE: usize = 1024 * 1024;
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
static HEAP_NEXT: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

struct BumpAllocator;

unsafe impl core::alloc::GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let base = core::ptr::addr_of!(HEAP) as usize;
        let align = layout.align();
        let size = layout.size();
        loop {
            let cur = HEAP_NEXT.load(core::sync::atomic::Ordering::Relaxed);
            // Align up from base+cur.
            let aligned = (base + cur + align - 1) & !(align - 1);
            let offset = aligned - base;
            let next = offset + size;
            if next > HEAP_SIZE {
                return core::ptr::null_mut();
            }
            if HEAP_NEXT
                .compare_exchange(
                    cur,
                    next,
                    core::sync::atomic::Ordering::AcqRel,
                    core::sync::atomic::Ordering::Relaxed,
                )
                .is_ok()
            {
                return aligned as *mut u8;
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {
        // Bump allocator: no-op dealloc (WM state is long-lived).
    }
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;
