//! Kernel display backends: KernelBackend (sys_fb_* syscalls) and
//! BackbufferBackend (claim compositor → map backbuffer → direct writes → single flush).

use userland_core::DisplayBackend;
use crate::{
    sys_fb_clear, sys_fb_fill_rect, sys_fb_flush,
    sys_claim_compositor, sys_map_framebuffer, sys_screen_size,
};

/// Legacy kernel backend: draws via per-rect sys_fb_* syscalls.
/// Queries real screen dimensions from the kernel at construction.
pub struct KernelBackend {
    width:  u32,
    height: u32,
}

impl KernelBackend {
    pub fn new() -> Self {
        let mut w = 0u32;
        let mut h = 0u32;
        let _ = sys_screen_size(&mut w, &mut h);
        // Fall back to a sane default if the syscall returns nothing (e.g. before fb init).
        if w == 0 || h == 0 {
            w = 1920;
            h = 1080;
        }
        Self { width: w, height: h }
    }
}

impl DisplayBackend for KernelBackend {
    fn screen_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn clear(&mut self, color: u32) {
        let _ = sys_fb_clear(color);
    }

    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        let _ = sys_fb_fill_rect(x, y, w, h, color);
    }

    fn flush(&mut self) {
        let _ = sys_fb_flush();
    }
}

/// Backbuffer backend: claims compositor authority, maps the kernel backbuffer
/// into userland, writes entire frames directly, then flushes with one syscall.
/// This is the Option-B model: all drawing happens in userspace memory; a single
/// sys_fb_flush blits the complete frame to the GOP hardware framebuffer.
pub struct BackbufferBackend {
    ptr: *mut u32,
    width: u32,
    height: u32,
    stride_px: usize,
}

unsafe impl Send for BackbufferBackend {}

impl BackbufferBackend {
    /// Claim compositor authority and map the backbuffer.
    /// Returns `None` if the compositor is already held by another process.
    pub fn claim() -> Option<Self> {
        if sys_claim_compositor() < 0 {
            return None;
        }
        let mut fb_ptr: u64 = 0;
        let mut fb_w: u32 = 0;
        let mut fb_h: u32 = 0;
        let mut fb_stride: u32 = 0;
        if sys_map_framebuffer(&mut fb_ptr, &mut fb_w, &mut fb_h, &mut fb_stride) < 0
            || fb_ptr == 0 || fb_w == 0 || fb_h == 0
        {
            return None;
        }
        Some(Self {
            ptr: fb_ptr as *mut u32,
            width: fb_w,
            height: fb_h,
            stride_px: (fb_stride / 4) as usize,
        })
    }
}

impl DisplayBackend for BackbufferBackend {
    fn screen_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn clear(&mut self, color: u32) {
        for row in 0..self.height as usize {
            let row_base = row * self.stride_px;
            for col in 0..self.width as usize {
                unsafe { core::ptr::write_volatile(self.ptr.add(row_base + col), color); }
            }
        }
    }

    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        let x_end = (x + w).min(self.width) as usize;
        let y_end = (y + h).min(self.height) as usize;
        let xs = x as usize;
        let ys = y as usize;
        for row in ys..y_end {
            let row_base = row * self.stride_px;
            for col in xs..x_end {
                unsafe { core::ptr::write_volatile(self.ptr.add(row_base + col), color); }
            }
        }
    }

    fn flush(&mut self) {
        let _ = sys_fb_flush();
    }
}
