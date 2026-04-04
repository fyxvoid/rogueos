//! Kernel display backend: implements DisplayBackend using sys_fb_* syscalls.
//! Used by the unified session binary on RogueOS (x86_64-unknown-none).

use userland_core::DisplayBackend;
use crate::{sys_fb_clear, sys_fb_fill_rect, sys_fb_flush};

/// Fixed screen size matching kernel framebuffer (BootInfo / GOP).
const SCREEN_W: u32 = 1280;
const SCREEN_H: u32 = 800;

/// Kernel backend: draws via sys_fb_* syscalls.
pub struct KernelBackend;

impl KernelBackend {
    pub const fn new() -> Self {
        Self
    }
}

impl DisplayBackend for KernelBackend {
    fn screen_size(&self) -> (u32, u32) {
        (SCREEN_W, SCREEN_H)
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
