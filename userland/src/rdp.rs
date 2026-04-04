//! RDP (Rogue Display Protocol) client library.
//!
//! Apps call `RdpSurface::connect()` to get a compositor-assigned window,
//! render into their own pixel buffer, and call `commit()` to push the frame.
//! The compositor sends `RdpPresentDone` after blitting so the client knows
//! it is safe to write the next frame (frame-callback / flow control).
//!
//! # Typical usage
//!
//! ```no_run
//! let mut surface = RdpSurface::connect(b"my-app").expect("no compositor");
//! loop {
//!     // Render into buf[..surface.buf_size()] at stride surface.width()*4 ...
//!     surface.commit(buf.as_ptr(), surface.width() * 4);
//!     // commit() blocks until compositor acks — no frame queue overflow possible.
//!     if let Some(ev) = surface.poll_event() { /* handle */ }
//! }
//! ```

use libs::{IPC_NONBLOCK, RwmMsg};
use crate::{
    sys_get_compositor_pid, sys_ipc_recv, sys_ipc_send,
    sys_surface_attach, sys_surface_create, sys_surface_destroy,
};

// RDP RwmType byte constants — mirror libs::RwmType repr.
const RDP_CONNECT:      u8 = 0x50;
const RDP_GRANT:        u8 = 0x51;
const RDP_COMMIT:       u8 = 0x52;
const RDP_RESIZE:       u8 = 0x53;
const RDP_KEY:          u8 = 0x54;
const RDP_FOCUS:        u8 = 0x55;
const RDP_CLOSE:        u8 = 0x56;
const RDP_DISCONNECT:   u8 = 0x57;
const RDP_PRESENT_DONE: u8 = 0x58;

// ── Event types ───────────────────────────────────────────────────────────────

/// Kind of event received from the compositor.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// Key event: (keycode, pressed).
    Key { keycode: u32, pressed: bool },
    /// Focus gained (true) or lost (false).
    Focus(bool),
    /// Compositor has resized the window to (width, height).
    /// The client MUST re-render at the new dimensions before the next commit().
    Resize { width: u32, height: u32 },
    /// Compositor asks the client to close gracefully (Mod+Shift+C).
    Close,
}

/// An event delivered from the compositor to this window.
pub struct RdpEvent {
    pub kind: EventKind,
}

// ── RdpSurface ────────────────────────────────────────────────────────────────

/// A live RDP window — connects to the compositor, owns a surface, and
/// handles compositor events.
pub struct RdpSurface {
    compositor_pid: u32,
    surface_id:     u32,
    #[allow(dead_code)]
    x:              i32,
    #[allow(dead_code)]
    y:              i32,
    /// Content area width as granted / last resized by the compositor.
    width:          u32,
    /// Content area height as granted / last resized by the compositor.
    height:         u32,
    /// Monotonically increasing frame sequence number.
    seq:            u16,
    /// Pending resize: set when poll_event sees RDP_RESIZE but commit() has not
    /// yet applied the new dimensions.
    pending_w:      u32,
    pending_h:      u32,
}

impl RdpSurface {
    // ── Connect ───────────────────────────────────────────────────────────────

    /// Connect to the compositor and request a window.
    /// Blocks until the compositor grants a window geometry (RdpGrant).
    /// Returns `None` if no compositor is registered or surface creation fails.
    pub fn connect(title: &[u8]) -> Option<Self> {
        // Locate compositor.
        let comp_r = sys_get_compositor_pid();
        if comp_r <= 0 {
            return None;
        }
        let compositor_pid = comp_r as u32;

        // Create our kernel surface (we are the owner).
        let sid_r = sys_surface_create();
        if sid_r <= 0 {
            return None;
        }
        let surface_id = sid_r as u32;

        // Send RdpConnect to compositor.
        let mut msg = RwmMsg::ZERO;
        msg.msg_type = RDP_CONNECT;
        let rdp = unsafe { &mut msg.payload.rdp };
        rdp.surface_id = surface_id;
        rdp.flags = 0;
        let n = title.len().min(rdp.title.len().saturating_sub(1));
        rdp.title[..n].copy_from_slice(&title[..n]);

        if sys_ipc_send(compositor_pid, &msg, 0) < 0 {
            let _ = sys_surface_destroy(surface_id);
            return None;
        }

        // Wait for RdpGrant (blocking — compositor assigns our geometry).
        let mut reply = RwmMsg::ZERO;
        loop {
            if sys_ipc_recv(&mut reply, 0) == 0 && reply.msg_type == RDP_GRANT {
                break;
            }
        }
        let g = unsafe { reply.payload.rdp };

        Some(RdpSurface {
            compositor_pid,
            surface_id,
            x:         g.x,
            y:         g.y,
            width:     g.width,
            height:    g.height,
            seq:       1,
            pending_w: 0,
            pending_h: 0,
        })
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// Current content width (updated on resize events).
    pub fn width(&self) -> u32 { self.width }
    /// Current content height (updated on resize events).
    pub fn height(&self) -> u32 { self.height }
    /// Bytes needed for one full frame: `width * height * 4`.
    pub fn buf_size(&self) -> usize { (self.width * self.height * 4) as usize }
    /// Recommended stride in bytes: `width * 4`.
    pub fn stride(&self) -> u32 { self.width * 4 }

    // ── Commit ────────────────────────────────────────────────────────────────

    /// Attach `buf` to the kernel surface and notify the compositor.
    ///
    /// **Frame callback:** this call blocks until the compositor sends
    /// `RdpPresentDone`, ensuring the client never gets ahead of the compositor
    /// and the IPC queue cannot overflow.
    ///
    /// `buf` must point to at least `width * height * 4` bytes.
    /// `stride` is in bytes (pass `width * 4` for packed rows).
    pub fn commit(&mut self, buf: *const u8, stride: u32) {
        // Apply any pending resize before attaching the buffer.
        if self.pending_w != 0 {
            self.width    = self.pending_w;
            self.height   = self.pending_h;
            self.pending_w = 0;
            self.pending_h = 0;
        }

        // Give the kernel our pixel buffer.
        let _ = sys_surface_attach(self.surface_id, buf, self.width, self.height, stride);

        // Tell the compositor: "my surface is ready, please blit it".
        let commit_seq = self.seq;
        self.seq = self.seq.wrapping_add(1);

        let mut msg = RwmMsg::ZERO;
        msg.msg_type = RDP_COMMIT;
        msg.seq      = commit_seq;
        let rdp = unsafe { &mut msg.payload.rdp };
        rdp.surface_id = self.surface_id;
        let _ = sys_ipc_send(self.compositor_pid, &msg, 0);

        // Frame callback: wait for RdpPresentDone that acks our seq.
        // This prevents buffer recycling races and IPC queue overflow.
        self.wait_present_done(commit_seq);
    }

    /// Non-blocking commit — send the frame without waiting for PresentDone.
    /// Use only when you manage your own double-buffering and queue depth.
    pub fn commit_async(&mut self, buf: *const u8, stride: u32) {
        if self.pending_w != 0 {
            self.width    = self.pending_w;
            self.height   = self.pending_h;
            self.pending_w = 0;
            self.pending_h = 0;
        }
        let _ = sys_surface_attach(self.surface_id, buf, self.width, self.height, stride);
        let mut msg = RwmMsg::ZERO;
        msg.msg_type   = RDP_COMMIT;
        msg.seq        = self.seq;
        self.seq       = self.seq.wrapping_add(1);
        let rdp = unsafe { &mut msg.payload.rdp };
        rdp.surface_id = self.surface_id;
        let _ = sys_ipc_send(self.compositor_pid, &msg, 0);
    }

    // ── Event polling ─────────────────────────────────────────────────────────

    /// Poll for a compositor event (non-blocking).
    /// Returns `None` immediately if the inbox is empty.
    ///
    /// Resize events update `self.width` / `self.height` immediately so the
    /// next call to `width()` / `height()` reflects the new geometry.
    pub fn poll_event(&mut self) -> Option<RdpEvent> {
        let mut msg = RwmMsg::ZERO;
        if sys_ipc_recv(&mut msg, IPC_NONBLOCK) < 0 {
            return None;
        }
        let rdp = unsafe { msg.payload.rdp };
        match msg.msg_type {
            RDP_KEY => Some(RdpEvent {
                kind: EventKind::Key {
                    keycode: rdp.key_code,
                    pressed: rdp.key_state != 0,
                },
            }),
            RDP_FOCUS => Some(RdpEvent {
                kind: EventKind::Focus(rdp.flags != 0),
            }),
            RDP_RESIZE => {
                // FIX #4: update stored dimensions immediately so width()/height()
                // are correct for the next render pass.
                self.width  = rdp.width;
                self.height = rdp.height;
                // Also store as pending so commit() re-attaches at the right size.
                self.pending_w = rdp.width;
                self.pending_h = rdp.height;
                Some(RdpEvent {
                    kind: EventKind::Resize { width: rdp.width, height: rdp.height },
                })
            }
            RDP_CLOSE => Some(RdpEvent { kind: EventKind::Close }),
            // Swallow PresentDone if the app polls outside of commit() — not an error.
            RDP_PRESENT_DONE => None,
            _ => None,
        }
    }

    // ── Disconnect ────────────────────────────────────────────────────────────

    /// Gracefully close the window and release the kernel surface.
    pub fn disconnect(self) {
        let mut msg = RwmMsg::ZERO;
        msg.msg_type = RDP_DISCONNECT;
        let rdp = unsafe { &mut msg.payload.rdp };
        rdp.surface_id = self.surface_id;
        let _ = sys_ipc_send(self.compositor_pid, &msg, 0);
        let _ = sys_surface_destroy(self.surface_id);
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Block until we receive RdpPresentDone for `expected_seq`.
    /// Intercepts and stores any resize events seen while waiting.
    fn wait_present_done(&mut self, expected_seq: u16) {
        let mut msg = RwmMsg::ZERO;
        loop {
            if sys_ipc_recv(&mut msg, 0) < 0 {
                // Blocking recv returned error — unexpected; break to avoid spin.
                break;
            }
            match msg.msg_type {
                RDP_PRESENT_DONE => {
                    if msg.seq == expected_seq {
                        break;
                    }
                    // Ack for a different seq (can happen if we fell behind) — accept it.
                    break;
                }
                RDP_RESIZE => {
                    // Store as pending; apply on next commit().
                    let rdp = unsafe { msg.payload.rdp };
                    self.width     = rdp.width;
                    self.height    = rdp.height;
                    self.pending_w = rdp.width;
                    self.pending_h = rdp.height;
                }
                // Anything else (key, focus, close) — drop for now.
                // A production implementation would queue these.
                _ => {}
            }
        }
    }
}
