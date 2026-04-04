//! RDP (Rogue Display Protocol) client library.
//!
//! Provides a simple surface-backed window that renders into its own pixel
//! buffer and notifies the compositor via IPC when the buffer is updated.
//!
//! # Typical usage
//!
//! ```no_run
//! let mut surface = RdpSurface::connect(b"my-app");
//! // surface.width() / surface.height() are set by the compositor.
//! loop {
//!     // Render into surface.buf_mut() ...
//!     surface.commit();
//!     if let Some(ev) = surface.poll_event() {
//!         match ev.kind { ... }
//!     }
//! }
//! ```

use libs::{IPC_NONBLOCK, RwmMsg};
use crate::{
    sys_get_compositor_pid, sys_ipc_recv, sys_ipc_send,
    sys_surface_attach, sys_surface_create, sys_surface_destroy,
};

// RDP RwmType byte constants (mirror libs::RwmType repr).
const RDP_CONNECT:    u8 = 0x50;
const RDP_GRANT:      u8 = 0x51;
const RDP_COMMIT:     u8 = 0x52;
const RDP_RESIZE:     u8 = 0x53;
const RDP_KEY:        u8 = 0x54;
const RDP_FOCUS:      u8 = 0x55;
const RDP_CLOSE:      u8 = 0x56;
const RDP_DISCONNECT: u8 = 0x57;

/// Kind of event received from the compositor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// Key pressed (key_code, pressed).
    Key { keycode: u32, pressed: bool },
    /// Focus gained (true) or lost (false).
    Focus(bool),
    /// Compositor requests resize to (width, height). Commit new buffer after resizing.
    Resize { width: u32, height: u32 },
    /// Compositor requests graceful window close.
    Close,
}

/// An event delivered by the compositor to this RDP window.
pub struct RdpEvent {
    pub kind: EventKind,
}

/// A RDP window — connects to the compositor, manages a pixel buffer, and
/// handles compositor events.
pub struct RdpSurface {
    compositor_pid: u32,
    surface_id:     u32,
    #[allow(dead_code)]
    x:              i32,
    #[allow(dead_code)]
    y:              i32,
    width:          u32,
    height:         u32,
    seq:            u16,
}

impl RdpSurface {
    /// Connect to the compositor and request a window with the given title.
    /// Blocks until the compositor sends a RdpGrant response.
    /// Returns `None` if no compositor is registered or surface creation fails.
    pub fn connect(title: &[u8]) -> Option<Self> {
        // Find compositor PID.
        let comp_r = sys_get_compositor_pid();
        if comp_r <= 0 {
            return None;
        }
        let compositor_pid = comp_r as u32;

        // Create our own surface (we are the owner).
        let sid_r = sys_surface_create();
        if sid_r <= 0 {
            return None;
        }
        let surface_id = sid_r as u32;

        // Build RdpConnect message.
        let mut msg = RwmMsg::ZERO;
        msg.msg_type = RDP_CONNECT;
        let kdp = unsafe { &mut msg.payload.rdp };
        kdp.surface_id = surface_id;
        kdp.flags = 0;
        let n = title.len().min(kdp.title.len() - 1);
        kdp.title[..n].copy_from_slice(&title[..n]);

        // Send to compositor.
        if sys_ipc_send(compositor_pid, &msg, 0) < 0 {
            let _ = sys_surface_destroy(surface_id);
            return None;
        }

        // Wait for RdpGrant.
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
            x:      g.x,
            y:      g.y,
            width:  g.width,
            height: g.height,
            seq:    1,
        })
    }

    /// Width assigned by the compositor.
    pub fn width(&self) -> u32 { self.width }
    /// Height assigned by the compositor.
    pub fn height(&self) -> u32 { self.height }

    /// Attach a rendered pixel buffer and notify the compositor.
    /// `buf` must be a `width * height * 4` byte 32bpp ARGB buffer.
    /// The compositor will blit this buffer on the next composite pass.
    pub fn commit(&mut self, buf: *const u8, stride: u32) {
        let _ = sys_surface_attach(self.surface_id, buf, self.width, self.height, stride);

        let mut msg = RwmMsg::ZERO;
        msg.msg_type = RDP_COMMIT;
        msg.seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        let kdp = unsafe { &mut msg.payload.rdp };
        kdp.surface_id = self.surface_id;
        let _ = sys_ipc_send(self.compositor_pid, &msg, 0);
    }

    /// Poll for an event from the compositor (non-blocking).
    /// Returns `None` immediately if no event is queued.
    pub fn poll_event(&self) -> Option<RdpEvent> {
        let mut msg = RwmMsg::ZERO;
        if sys_ipc_recv(&mut msg, IPC_NONBLOCK) < 0 {
            return None;
        }
        let kdp = unsafe { msg.payload.rdp };
        match msg.msg_type {
            RDP_KEY => Some(RdpEvent {
                kind: EventKind::Key {
                    keycode: kdp.key_code,
                    pressed: kdp.key_state != 0,
                },
            }),
            RDP_FOCUS => Some(RdpEvent {
                kind: EventKind::Focus(kdp.flags != 0),
            }),
            RDP_RESIZE => {
                Some(RdpEvent {
                    kind: EventKind::Resize {
                        width:  kdp.width,
                        height: kdp.height,
                    },
                })
            }
            RDP_CLOSE => Some(RdpEvent { kind: EventKind::Close }),
            _ => None,
        }
    }

    /// Disconnect from the compositor and destroy the surface.
    pub fn disconnect(self) {
        let mut msg = RwmMsg::ZERO;
        msg.msg_type = RDP_DISCONNECT;
        let kdp = unsafe { &mut msg.payload.rdp };
        kdp.surface_id = self.surface_id;
        let _ = sys_ipc_send(self.compositor_pid, &msg, 0);
        let _ = sys_surface_destroy(self.surface_id);
    }
}
