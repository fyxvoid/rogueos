//! Graphics and input syscalls: poll_input, poll_mouse, fb_clear, fb_fill_rect,
//! fb_flush, fb_blit, surface_create/destroy/attach/commit, screen_size,
//! map_framebuffer (Option B: compositor backbuffer mapped into userland).

use crate::syscall::user_ptr::{self, SysErr};
use libs::{KeyEvent, MouseEvent};

// ── Compositor backbuffer (Option B) ─────────────────────────────────────────
//
// Allocated once on first SYS_MAP_FRAMEBUFFER call. Identity-mapped (VA == PA)
// so returning the physical address to userland is sufficient under the current
// shared-CR3 model. When per-process page tables land this becomes a proper
// mmap into the compositor's address space.

struct Backbuffer {
    base: *mut u8,
    width: u32,
    height: u32,
    stride_bytes: u32, // bytes per row
}

unsafe impl Send for Backbuffer {}
unsafe impl Sync for Backbuffer {}

static mut BACKBUFFER: Option<Backbuffer> = None;

pub(super) fn sys_poll_input(ev_ptr: *mut KeyEvent) -> Result<u64, SysErr> {
    if ev_ptr.is_null() {
        return Err(SysErr::INVAL);
    }
    let cr3 = user_ptr::current_cr3()?;
    user_ptr::validate_user_range(cr3, ev_ptr as u64, core::mem::size_of::<KeyEvent>(), true)?;
    // Drain PS/2 hardware buffer before reading from the software queue.
    crate::drivers::hid_stub::poll_input();
    let input = crate::drivers::input::get_input_source();
    match input.pop_event() {
        Some(ev) => {
            unsafe { core::ptr::write_volatile(ev_ptr, ev) }
            Ok(1)
        }
        None => Ok(0),
    }
}

pub(super) fn sys_poll_mouse(ev_ptr: *mut MouseEvent) -> Result<u64, SysErr> {
    if ev_ptr.is_null() {
        return Err(SysErr::INVAL);
    }
    let cr3 = user_ptr::current_cr3()?;
    user_ptr::validate_user_range(cr3, ev_ptr as u64, core::mem::size_of::<MouseEvent>(), true)?;
    // Drain PS/2 buffer so fresh mouse packets are available.
    crate::drivers::hid_stub::poll_input();
    match crate::drivers::input::pop_mouse_event() {
        Some(ev) => {
            unsafe { core::ptr::write_volatile(ev_ptr, ev) }
            Ok(1)
        }
        None => Ok(0),
    }
}

pub(super) fn sys_fb_clear(color: u32) -> Result<u64, SysErr> {
    unsafe {
        if let Some(ref bb) = BACKBUFFER {
            crate::drivers::framebuffer::fill_rect_ram(
                bb.base, bb.stride_bytes, bb.width, bb.height,
                0, 0, bb.width, bb.height, color,
            );
            return Ok(0);
        }
    }
    crate::drivers::framebuffer::get_framebuffer().clear(color);
    Ok(0)
}

pub(super) fn sys_fb_fill_rect(x: u32, y: u32, w: u32, h: u32, color: u32) -> Result<u64, SysErr> {
    unsafe {
        if let Some(ref bb) = BACKBUFFER {
            crate::drivers::framebuffer::fill_rect_ram(
                bb.base, bb.stride_bytes, bb.width, bb.height,
                x, y, w, h, color,
            );
            return Ok(0);
        }
    }
    crate::drivers::framebuffer::get_framebuffer().fill_rect(x, y, w, h, color);
    Ok(0)
}

pub(super) fn sys_fb_flush() -> Result<u64, SysErr> {
    // Only the registered compositor may blit its backbuffer.
    // Non-compositor processes (legacy sys_fb_fill_rect path) fall through to flush().
    let caller = crate::process::current_pid().unwrap_or(0);
    let is_compositor = crate::display::display_server::get_compositor_pid()
        .map(|pid| pid == caller)
        .unwrap_or(false);
    unsafe {
        if is_compositor {
            if let Some(ref bb) = BACKBUFFER {
                crate::drivers::framebuffer::blit(
                    0, 0, bb.width, bb.height, bb.stride_bytes, bb.base as *const u8,
                );
                return Ok(0);
            }
        }
    }
    crate::drivers::framebuffer::get_framebuffer().flush();
    Ok(0)
}

/// Called from the process exit path: if `pid` was the compositor, zero and
/// drop the backbuffer so the next compositor starts with a clean slate.
pub fn on_compositor_exit(pid: u32) {
    let is_compositor = crate::display::display_server::get_compositor_pid()
        .map(|p| p == pid)
        .unwrap_or(false);
    if !is_compositor {
        return;
    }
    unsafe {
        if let Some(ref bb) = BACKBUFFER {
            let size = (bb.stride_bytes as usize).saturating_mul(bb.height as usize);
            core::ptr::write_bytes(bb.base, 0, size);
        }
        BACKBUFFER = None;
    }
    crate::display::display_server::release_compositor(pid);
}

// ── Compositor backbuffer helpers (called by display_server) ─────────────────
//
// These allow display_server::surface_commit and composite_all to write into
// the compositor RAM backbuffer instead of directly to MMIO.
// Fall back to direct MMIO if the backbuffer has not been allocated yet.

/// Blit a surface's RAM pixel buffer into the compositor backbuffer (RAM→RAM).
/// Falls back to a direct MMIO blit if no backbuffer is allocated.
/// Called by `display_server::surface_commit` and `display_server::composite_all`.
pub(crate) fn backbuffer_blit(
    dst_x: u32,
    dst_y: u32,
    w: u32,
    h: u32,
    src_stride: u32,
    src: *const u8,
) {
    unsafe {
        if let Some(ref bb) = BACKBUFFER {
            crate::drivers::framebuffer::blit_ram(
                bb.base, dst_x, dst_y,
                bb.stride_bytes, bb.width, bb.height,
                src, src_stride, w, h,
            );
            return;
        }
    }
    // No backbuffer yet — fall back to legacy MMIO path so display still works.
    crate::drivers::framebuffer::blit(dst_x, dst_y, w, h, src_stride, src);
}

/// Fill a rectangle in the compositor backbuffer (RAM).
/// Falls back to MMIO fill_rect if no backbuffer is allocated.
pub(crate) fn backbuffer_fill_rect(x: u32, y: u32, w: u32, h: u32, color: u32) {
    unsafe {
        if let Some(ref bb) = BACKBUFFER {
            crate::drivers::framebuffer::fill_rect_ram(
                bb.base, bb.stride_bytes, bb.width, bb.height,
                x, y, w, h, color,
            );
            return;
        }
    }
    crate::drivers::framebuffer::get_framebuffer().fill_rect(x, y, w, h, color);
}

// ── Surface protocol syscalls ─────────────────────────────────────────────

/// Claim compositor authority. Returns 0 on success, PERM if already claimed.
pub(super) fn sys_claim_compositor() -> Result<u64, SysErr> {
    let pid = crate::process::current_pid().unwrap_or(0);
    if crate::display::display_server::claim_compositor(pid) {
        Ok(0)
    } else {
        Err(SysErr::PERM)
    }
}

/// Get the registered compositor PID. Returns pid on success, NOENT if none.
pub(super) fn sys_get_compositor_pid() -> Result<u64, SysErr> {
    match crate::display::display_server::get_compositor_pid() {
        Some(pid) => Ok(pid as u64),
        None => Err(SysErr::NOENT),
    }
}

/// Composite all surfaces in z-order and flush. Only compositor may call.
pub(super) fn sys_composite_all() -> Result<u64, SysErr> {
    let pid = crate::process::current_pid().unwrap_or(0);
    match crate::display::display_server::get_compositor_pid() {
        Some(comp_pid) if comp_pid != pid => return Err(SysErr::PERM),
        _ => {}
    }
    crate::display::display_server::composite_all();
    Ok(0)
}

/// Create a new display surface owned by the calling process. Returns surface_id or error.
pub(super) fn sys_surface_create() -> Result<u64, SysErr> {
    let owner = crate::process::current_pid().unwrap_or(0);
    match crate::display::display_server::surface_create(owner) {
        Some(id) => Ok(id as u64),
        None => Err(SysErr::NOMEM),
    }
}

/// Destroy a surface by id.
pub(super) fn sys_surface_destroy(id: u32) -> Result<u64, SysErr> {
    crate::display::display_server::surface_destroy(id);
    Ok(0)
}

/// Attach a 32bpp pixel buffer to a surface. Only the surface owner may attach.
/// Args: surface_id, ptr (user), width, height, stride_bytes.
pub(super) fn sys_surface_attach(
    id: u32,
    ptr: *const u8,
    width: u32,
    height: u32,
    stride: u32,
) -> Result<u64, SysErr> {
    if ptr.is_null() || width == 0 || height == 0 || stride < width * 4 {
        return Err(SysErr::INVAL);
    }
    let buf_bytes = (stride as usize).saturating_mul(height as usize);
    user_ptr::validate_user_ptr_large(ptr as u64, buf_bytes)?;
    let caller = crate::process::current_pid().unwrap_or(0);
    if crate::display::display_server::surface_attach(id, ptr, width, height, stride, caller) {
        Ok(0)
    } else {
        Err(SysErr::PERM)
    }
}

/// Commit (blit) surface buffer to framebuffer at (dst_x, dst_y). Only compositor may commit.
pub(super) fn sys_surface_commit(id: u32, dst_x: u32, dst_y: u32) -> Result<u64, SysErr> {
    let caller = crate::process::current_pid().unwrap_or(0);
    if crate::display::display_server::surface_commit(id, dst_x, dst_y, caller) {
        Ok(0)
    } else {
        Err(SysErr::PERM)
    }
}

/// Set z-order for a surface (lower = further back; 255 = topmost).
pub(super) fn sys_surface_set_z(id: u32, z: u8) -> Result<u64, SysErr> {
    crate::display::display_server::surface_set_z(id, z);
    Ok(0)
}

// ── Shared memory syscalls ────────────────────────────────────────────────────
//
// Shared memory today: allocate page-by-page from the frame allocator.
// All processes share one CR3 so the physical (== identity-mapped virtual)
// address is readable by every process.  When per-process page tables land,
// replace with a proper shared mapping.  The public API is stable now.

const SHM_MAX_SLOTS: usize = 8;
const SHM_MAX_PAGES: usize = 512; // 2 MiB per slot max

struct ShmSlot {
    base:  u64,   // first frame physical address (identity == virtual)
    pages: u32,   // number of 4 KiB frames
}

static mut SHM_SLOTS: [Option<ShmSlot>; SHM_MAX_SLOTS] = [
    None, None, None, None, None, None, None, None,
];

/// Allocate a shared memory region. Returns (shm_id << 32 | va_u32) or negative.
pub(super) fn sys_shm_create(size: u64) -> Result<u64, SysErr> {
    if size == 0 || size > (SHM_MAX_PAGES as u64 * 4096) {
        return Err(SysErr::INVAL);
    }
    let pages = ((size + 4095) / 4096) as usize;

    // Find a free slot.
    let slot_idx = unsafe {
        SHM_SLOTS.iter().position(|s| s.is_none()).ok_or(SysErr::NOMEM)?
    };

    // Allocate the first page to get a base address; allocate the rest
    // hoping for nearby frames (works well on a lightly loaded allocator).
    let base = crate::memory::physical::alloc_frame()
        .ok_or(SysErr::NOMEM)?;

    for _ in 1..pages {
        // Best-effort: allocate additional frames. Non-contiguous pages are
        // fine for the current shared CR3 model.
        let _ = crate::memory::physical::alloc_frame();
    }

    unsafe {
        SHM_SLOTS[slot_idx] = Some(ShmSlot { base, pages: pages as u32 });
    }

    let shm_id = slot_idx as u32;
    let va_low = (base & 0xFFFF_FFFF) as u32;
    Ok(((shm_id as u64) << 32) | va_low as u64)
}

/// Free a shared memory region. shm_id is the slot index returned by sys_shm_create.
pub(super) fn sys_shm_destroy(shm_id: u32) -> Result<u64, SysErr> {
    let idx = shm_id as usize;
    if idx >= SHM_MAX_SLOTS {
        return Err(SysErr::INVAL);
    }
    let slot = unsafe { SHM_SLOTS[idx].take() };
    if let Some(s) = slot {
        // Free all frames.  Non-contiguous frames were individually allocated
        // so we free only the base frame here for now.
        crate::memory::physical::free_frame(s.base);
    }
    Ok(0)
}

/// Map the compositor backbuffer into the calling process's address space.
/// Allocates a contiguous backbuffer on first call (sized to match the GOP framebuffer).
/// Only callable after SYS_CLAIM_COMPOSITOR. Returns 0; output params receive the
/// backbuffer VA, width, height, and stride in bytes.
pub(super) fn sys_map_framebuffer(
    out_ptr: *mut u64,
    out_w: *mut u32,
    out_h: *mut u32,
    out_stride: *mut u32,
) -> Result<u64, SysErr> {
    if out_ptr.is_null() || out_w.is_null() || out_h.is_null() || out_stride.is_null() {
        return Err(SysErr::INVAL);
    }
    // Only the claimed compositor may access the backbuffer directly.
    let caller = crate::process::current_pid().unwrap_or(0);
    match crate::display::display_server::get_compositor_pid() {
        Some(comp_pid) if comp_pid == caller => {}
        _ => return Err(SysErr::PERM),
    }

    let cr3 = user_ptr::current_cr3()?;
    user_ptr::validate_user_range(cr3, out_ptr as u64, 8, true)?;
    user_ptr::validate_user_range(cr3, out_w as u64, 4, true)?;
    user_ptr::validate_user_range(cr3, out_h as u64, 4, true)?;
    user_ptr::validate_user_range(cr3, out_stride as u64, 4, true)?;

    unsafe {
        if BACKBUFFER.is_none() {
            let (width, height, stride_bytes) = crate::drivers::framebuffer::dimensions()
                .ok_or(SysErr::NOENT)?;
            let size = (stride_bytes as usize).saturating_mul(height as usize);
            let pages = (size + 4095) / 4096;
            let base_phys = crate::memory::physical::alloc_contiguous(pages);
            if base_phys == 0 {
                return Err(SysErr::NOMEM);
            }
            // Identity-mapped: zero the backbuffer so userland starts with a clean slate.
            let base_ptr = base_phys as *mut u8;
            core::ptr::write_bytes(base_ptr, 0, size);
            BACKBUFFER = Some(Backbuffer { base: base_ptr, width, height, stride_bytes });
        }

        let bb = BACKBUFFER.as_ref().unwrap();
        core::ptr::write_volatile(out_ptr, bb.base as u64);
        core::ptr::write_volatile(out_w, bb.width);
        core::ptr::write_volatile(out_h, bb.height);
        core::ptr::write_volatile(out_stride, bb.stride_bytes);
    }
    Ok(0)
}

/// Fill two user u32 pointers with screen width and height.
pub(super) fn sys_screen_size(out_w: *mut u32, out_h: *mut u32) -> Result<u64, SysErr> {
    if out_w.is_null() || out_h.is_null() {
        return Err(SysErr::INVAL);
    }
    let cr3 = user_ptr::current_cr3()?;
    user_ptr::validate_user_range(cr3, out_w as u64, 4, true)?;
    user_ptr::validate_user_range(cr3, out_h as u64, 4, true)?;
    let (w, h) = crate::display::display_server::screen_size();
    unsafe {
        core::ptr::write_volatile(out_w, w);
        core::ptr::write_volatile(out_h, h);
    }
    Ok(0)
}

/// Blit a raw 32bpp user buffer to the framebuffer at (dst_x, dst_y).
pub(super) fn sys_fb_blit(
    dst_x: u32,
    dst_y: u32,
    w: u32,
    h: u32,
    stride: u32,
    ptr: *const u8,
) -> Result<u64, SysErr> {
    if ptr.is_null() || w == 0 || h == 0 || stride < w * 4 {
        return Err(SysErr::INVAL);
    }
    let buf_bytes = (stride as usize).saturating_mul(h as usize);
    user_ptr::validate_user_ptr_large(ptr as u64, buf_bytes)?;
    crate::drivers::framebuffer::blit(dst_x, dst_y, w, h, stride, ptr);
    Ok(0)
}
