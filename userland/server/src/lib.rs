//! Display server in userland: owns the display backend and exposes it for compositor/WM.
//! Today: single framebuffer (no surfaces). Later: connect/attach/commit when kernel exposes surface syscalls.

#![no_std]

use userland_core::DisplayBackend;

/// Server holds the display backend. On Kingdom there is one global framebuffer;
/// "connect" is implicit; attach/commit are used when we have surface API.
pub struct Server<B> {
    pub backend: B,
}

impl<B: DisplayBackend> Server<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn screen_size(&self) -> (u32, u32) {
        self.backend.screen_size()
    }

    /// Present (flush) the current framebuffer. No-op if backend has no double-buffering.
    pub fn commit(&mut self) {
        self.backend.flush();
    }

    /// Access backend for drawing (clear, fill_rect, fill_rect_rounded). Compositor uses this.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }
}
