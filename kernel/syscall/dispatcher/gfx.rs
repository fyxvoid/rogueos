//! Graphics and input syscalls: poll_input, poll_mouse, fb_clear, fb_fill_rect,
//! fb_flush, fb_blit, surface_create/destroy/attach/commit, screen_size.

use crate::syscall::user_ptr::{self, SysErr};
use libs::{KeyEvent, MouseEvent};

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
    match crate::drivers::input::pop_mouse_event() {
        Some(ev) => {
            unsafe { core::ptr::write_volatile(ev_ptr, ev) }
            Ok(1)
        }
        None => Ok(0),
    }
}

pub(super) fn sys_fb_clear(color: u32) -> Result<u64, SysErr> {
    crate::drivers::framebuffer::get_framebuffer().clear(color);
    Ok(0)
}

pub(super) fn sys_fb_fill_rect(x: u32, y: u32, w: u32, h: u32, color: u32) -> Result<u64, SysErr> {
    crate::drivers::framebuffer::get_framebuffer().fill_rect(x, y, w, h, color);
    Ok(0)
}

pub(super) fn sys_fb_flush() -> Result<u64, SysErr> {
    crate::drivers::framebuffer::get_framebuffer().flush();
    Ok(0)
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
