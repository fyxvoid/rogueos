//! Simple linear framebuffer abstraction for the kernel.
//!
//! The UEFI bootloader queries GOP, fills [`libs::BootInfo`] at the fixed
//! physical address, and the kernel maps that framebuffer into its page
//! tables for use by the window manager via syscalls.

use core::ptr;

use libs::BootInfo;

use crate::memory::paging;

/// Preferred resolution per plan (Section 7). Other sizes are accepted but logged.
pub const FB_WIDTH: u32 = 1920;
pub const FB_HEIGHT: u32 = 1080;

/// Global framebuffer description, filled during early graphics init.
pub struct FrameBufferInfo {
    pub base_virt: *mut u8,
    pub size: usize,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bpp: u32,
}

static mut FB_INFO: Option<FrameBufferInfo> = None;

/// Initialize graphics from the `BootInfo` provided at kernel entry.
/// Accepts any GOP mode; logs if it differs from the preferred 1920x1080.
pub fn init_from_boot_info(boot_info: &BootInfo) -> bool {
    if boot_info.fb_base == 0 || boot_info.fb_size == 0 {
        return false;
    }
    if boot_info.fb_width != FB_WIDTH || boot_info.fb_height != FB_HEIGHT {
        crate::arch::serial::write_str("[GFX] non-preferred mode ");
        crate::arch::serial::write_hex(boot_info.fb_width as u64);
        crate::arch::serial::write_str("x");
        crate::arch::serial::write_hex(boot_info.fb_height as u64);
        crate::arch::serial::write_str(" (preferred ");
        crate::arch::serial::write_hex(FB_WIDTH as u64);
        crate::arch::serial::write_str("x");
        crate::arch::serial::write_hex(FB_HEIGHT as u64);
        crate::arch::serial::write_str(")\r\n");
    }

    let fb_phys = boot_info.fb_base;
    let fb_size = boot_info.fb_size as usize;

    // Identity-map framebuffer region into the kernel's address space.
    let page_size = 4096u64;
    let start = fb_phys & !(page_size - 1);
    let end = (fb_phys + fb_size as u64 + page_size - 1) & !(page_size - 1);

    let mut pa = start;
    while pa < end {
        let _va = pa; // identity map physical to virtual
        let flags = paging::EntryFlags::kernel_rw().as_u64();
        if !paging::map_page_identity(pa, flags) {
            return false;
        }
        pa += page_size;
    }

    let base_virt = fb_phys as *mut u8;
    unsafe {
        FB_INFO = Some(FrameBufferInfo {
            base_virt,
            size: fb_size,
            width: boot_info.fb_width,
            height: boot_info.fb_height,
            stride: boot_info.fb_stride,
            bpp: boot_info.fb_bpp,
        });
    }

    true
}

fn info() -> Option<&'static FrameBufferInfo> {
    unsafe { FB_INFO.as_ref() }
}

/// Returns (width, height, stride_bytes) of the active framebuffer, or None if not initialised.
pub fn dimensions() -> Option<(u32, u32, u32)> {
    info().map(|i| (i.width, i.height, i.stride * 4))
}

/// Dump framebuffer state to serial (for diagnostic_halt / Section 11).
pub fn dump_state_serial() {
    if let Some(info) = info() {
        crate::arch::serial::write_str("[DIAG] framebuffer base=");
        crate::arch::serial::write_hex(info.base_virt as u64);
        crate::arch::serial::write_str(" size=");
        crate::arch::serial::write_hex(info.size as u64);
        crate::arch::serial::write_str(" width=");
        crate::arch::serial::write_hex(info.width as u64);
        crate::arch::serial::write_str(" height=");
        crate::arch::serial::write_hex(info.height as u64);
        crate::arch::serial::write_str(" stride=");
        crate::arch::serial::write_hex(info.stride as u64);
        crate::arch::serial::write_str(" bpp=");
        crate::arch::serial::write_hex(info.bpp as u64);
        crate::arch::serial::write_str("\r\n");
    } else {
        crate::arch::serial::write_str("[DIAG] framebuffer not initialized\r\n");
    }
}

/// Clear the entire framebuffer to a solid ARGB color.
pub fn clear(color: u32) {
    if let Some(info) = info() {
        if info.base_virt.is_null() || info.bpp != 32 || info.size == 0 {
            return;
        }
        if info.stride == 0 || info.width == 0 || info.height == 0 {
            return;
        }
        let pixels = (info.size / 4) as usize;
        unsafe {
            let buf = info.base_virt as *mut u32;
            for i in 0..pixels {
                ptr::write_volatile(buf.add(i), color);
            }
        }
    }
}

/// Fill a rectangle in the framebuffer with a solid ARGB color.
pub fn fill_rect(x: u32, y: u32, w: u32, h: u32, color: u32) {
    let info = match info() {
        Some(i) => i,
        None => return,
    };
    if info.base_virt.is_null() || info.bpp != 32 || w == 0 || h == 0 {
        return;
    }
    if info.stride == 0 || info.width == 0 || info.height == 0 {
        return;
    }

    let max_x = info.width;
    let max_y = info.height;

    let x0 = x;
    let y0 = y;
    let mut w0 = w;
    let mut h0 = h;

    if x0 >= max_x || y0 >= max_y {
        return;
    }

    if x0 + w0 > max_x {
        w0 = max_x - x0;
    }
    if y0 + h0 > max_y {
        h0 = max_y - y0;
    }

    let stride = info.stride as usize;
    unsafe {
        let buf = info.base_virt as *mut u32;
        // Build one filled row in a stack buffer, then memcpy each row.
        let mut row_buf = [0u32; 1920];
        let rw = w0 as usize;
        for p in row_buf[..rw].iter_mut() { *p = color; }
        for row in 0..h0 as usize {
            let dst_row = buf.add((y0 as usize + row) * stride + x0 as usize);
            ptr::copy_nonoverlapping(row_buf.as_ptr(), dst_row, rw);
        }
    }
}

/// Copy from a 32bpp source buffer into the framebuffer at (dst_x, dst_y).
/// src_stride is in bytes. Clips to framebuffer bounds.
pub fn blit(dst_x: u32, dst_y: u32, w: u32, h: u32, src_stride: u32, src_ptr: *const u8) {
    let info = match info() {
        Some(i) => i,
        None => return,
    };
    if info.base_virt.is_null() || info.bpp != 32 || src_ptr.is_null() {
        return;
    }
    let max_x = info.width;
    let max_y = info.height;
    let dst_stride = info.stride as usize;
    let w = w.min(max_x.saturating_sub(dst_x));
    let h = h.min(max_y.saturating_sub(dst_y));
    if w == 0 || h == 0 {
        return;
    }
    unsafe {
        let dst = info.base_virt as *mut u32;
        let src = src_ptr as *const u32;
        let src_stride_u32 = (src_stride as usize) / 4;
        for row in 0..h as usize {
            let src_row = src.add(row * src_stride_u32);
            let dst_row = dst.add((dst_y as usize + row) * dst_stride + dst_x as usize);
            ptr::copy_nonoverlapping(src_row, dst_row, w as usize);
        }
    }
}

/// Fast RAM-to-RAM pixel blit. Uses `copy_nonoverlapping` per row so the
/// compiler can vectorize (SSE/AVX). Do NOT use for MMIO destinations —
/// use `blit()` there which uses `write_volatile` to prevent elision.
///
/// Clips to `(dst_w, dst_h)` bounds. `dst_stride` and `src_stride` are in bytes.
pub fn blit_ram(
    dst: *mut u8,
    dst_x: u32,
    dst_y: u32,
    dst_stride: u32,
    dst_w: u32,
    dst_h: u32,
    src: *const u8,
    src_stride: u32,
    w: u32,
    h: u32,
) {
    if dst.is_null() || src.is_null() || w == 0 || h == 0 { return; }
    if dst_x >= dst_w || dst_y >= dst_h { return; }
    let w = w.min(dst_w - dst_x);
    let h = h.min(dst_h - dst_y);
    let row_bytes = w as usize * 4; // 32bpp
    unsafe {
        for row in 0..h as usize {
            let dst_row = dst.add(
                (dst_y as usize + row) * dst_stride as usize + dst_x as usize * 4,
            );
            let src_row = src.add(row * src_stride as usize);
            core::ptr::copy_nonoverlapping(src_row, dst_row, row_bytes);
        }
    }
}

/// Fill a rectangle in a RAM buffer with a solid 32bpp color.
/// Uses plain (non-volatile) stores so the compiler can emit SIMD fills.
/// Do NOT use for MMIO destinations — use `fill_rect()` there.
pub fn fill_rect_ram(
    dst: *mut u8,
    dst_stride: u32,
    dst_w: u32,
    dst_h: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: u32,
) {
    if dst.is_null() || w == 0 || h == 0 { return; }
    if x >= dst_w || y >= dst_h { return; }
    let w = w.min(dst_w - x);
    let h = h.min(dst_h - y);
    unsafe {
        for row in 0..h as usize {
            let row_ptr = dst.add(
                (y as usize + row) * dst_stride as usize + x as usize * 4,
            ) as *mut u32;
            for col in 0..w as usize {
                *row_ptr.add(col) = color;
            }
        }
    }
}

/// Flush any pending drawing to hardware (stub — drawing is direct to MMIO).
pub fn flush() {
    // Compositor path flushes via sys_fb_flush → blit(BACKBUFFER → MMIO).
}

/// Draw a test pattern to verify framebuffer is writable and visible.
/// Red background, gradient band, and a second color band. Call after init_from_boot_info.
pub fn draw_test_pattern() {
    let info = match info() {
        Some(i) => i,
        None => return,
    };
    if info.base_virt.is_null() || info.bpp != 32 || info.width == 0 || info.height == 0 {
        return;
    }
    // Solid red (ARGB)
    clear(0xFF_00_00_FF);
    // Gradient band (top 80 pixels): red -> blue
    let band_h = 80u32.min(info.height);
    for row in 0..band_h {
        let t = row as u32 * 255 / band_h.max(1);
        let color = 0xFF_00_00_00 | (255 - t) << 16 | t; // R->B
        fill_rect(0, row, info.width, 1, color);
    }
    // Second band: green rectangle in center
    let cx = info.width / 2;
    let cy = info.height / 2;
    let rw = 200u32.min(info.width.saturating_sub(40));
    let rh = 120u32.min(info.height.saturating_sub(40));
    let rx = cx.saturating_sub(rw / 2);
    let ry = cy.saturating_sub(rh / 2);
    fill_rect(rx, ry, rw, rh, 0xFF_00_FF_00);
    flush();
}

// --- Driver trait implementation ---

/// GOP framebuffer as the concrete Framebuffer implementation.
pub struct GopFramebuffer;

impl crate::drivers::traits::Framebuffer for GopFramebuffer {
    fn clear(&self, color: u32) {
        clear(color);
    }
    fn fill_rect(&self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        fill_rect(x, y, w, h, color);
    }
    fn flush(&self) {
        flush();
    }
}

static GOP_FRAMEBUFFER: GopFramebuffer = GopFramebuffer;

/// Return the kernel's framebuffer implementation (trait object). Syscall layer uses this.
pub fn get_framebuffer() -> &'static dyn crate::drivers::traits::Framebuffer {
    &GOP_FRAMEBUFFER
}

