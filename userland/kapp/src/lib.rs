//! `kapp` — Kingdom OS application SDK
//!
//! Provides the three building blocks every Kingdom app needs:
//!
//! * [`App`]    — process identity + IPC state
//! * [`Window`] — pixel buffer + WM registration
//! * [`Event`]  — typed event enum fed by the WM
//!
//! # Quick start
//!
//! ```rust,ignore
//! #[no_mangle]
//! pub extern "C" fn kmain() {
//!     let mut app = App::new("My App");
//!     let mut win = Window::new(&mut app, 800, 600);
//!
//!     loop {
//!         match app.poll_event() {
//!             Event::Key(code, pressed) => { /* handle key */ }
//!             Event::Resize(w, h)       => win.resize(w, h),
//!             Event::Close              => break,
//!             Event::None               => {}
//!             _ => {}
//!         }
//!
//!         // Draw into win.pixels_mut() …
//!         win.commit(&mut app);
//!     }
//! }
//! ```

#![no_std]
extern crate alloc;

use alloc::vec::Vec;
use libs::{
    IPC_NONBLOCK, KwmMsg, KwmType,
    PayloadGeometry, PayloadRegister, PayloadSetTitle, PayloadSurfaceCommit,
    SYSERR_AGAIN,
};
use userland::{
    sys_getpid, sys_ipc_recv, sys_ipc_send,
    sys_surface_attach, sys_surface_commit, sys_surface_create,
};

// ── App ──────────────────────────────────────────────────────────────────────

/// Process identity and IPC session with the WM.
pub struct App {
    pub pid:    u32,
    pub wm_pid: u32,
    seq:        u16,
}

impl App {
    /// Initialise the app, discover the WM, and register.
    ///
    /// `wm_pid` is passed explicitly because Kingdom OS does not yet have a
    /// name-service; the WM is always the first user process (pid 1 in the
    /// typical boot sequence).  Pass 0 to skip registration (standalone mode).
    pub fn new(title: &str, wm_pid: u32) -> Self {
        let pid = sys_getpid();
        let mut app = App { pid, wm_pid, seq: 0 };
        if wm_pid != 0 {
            app.register(title);
        }
        app
    }

    /// Send `KWM_REGISTER` to the WM.
    pub fn register(&mut self, title: &str) {
        let mut payload = PayloadRegister {
            title: [0u8; 48],
            flags: 0,
            _pad:  [0u8; 4],
        };
        let bytes = title.as_bytes();
        let n = bytes.len().min(47);
        payload.title[..n].copy_from_slice(&bytes[..n]);

        let msg = KwmMsg {
            msg_type:   KwmType::Register as u8,
            flags:      0,
            seq:        self.next_seq(),
            sender_pid: self.pid,
            payload:    libs::KwmPayload { register: payload },
        };
        let _ = sys_ipc_send(self.wm_pid, &msg, 0);
    }

    /// Set window title after initial registration.
    pub fn set_title(&mut self, title: &str) {
        let mut payload = PayloadSetTitle { title: [0u8; 56] };
        let bytes = title.as_bytes();
        let n = bytes.len().min(55);
        payload.title[..n].copy_from_slice(&bytes[..n]);

        let msg = KwmMsg {
            msg_type:   KwmType::SetTitle as u8,
            flags:      0,
            seq:        self.next_seq(),
            sender_pid: self.pid,
            payload:    libs::KwmPayload { set_title: payload },
        };
        let _ = sys_ipc_send(self.wm_pid, &msg, 0);
    }

    /// Send `KWM_UNREGISTER` and detach from the WM.
    pub fn unregister(&mut self) {
        let msg = KwmMsg {
            msg_type:   KwmType::Unregister as u8,
            flags:      0,
            seq:        self.next_seq(),
            sender_pid: self.pid,
            payload:    libs::KwmPayload { raw: libs::PayloadRaw { data: [0u8; 56] } },
        };
        let _ = sys_ipc_send(self.wm_pid, &msg, 0);
    }

    /// Non-blocking poll: return the next [`Event`] or [`Event::None`].
    pub fn poll_event(&mut self) -> Event {
        let mut msg = KwmMsg::ZERO;
        let ret = sys_ipc_recv(&mut msg, IPC_NONBLOCK);
        if ret == SYSERR_AGAIN as isize || ret < 0 {
            return Event::None;
        }
        decode_event(&msg)
    }

    #[inline]
    fn next_seq(&mut self) -> u16 {
        let s = self.seq;
        self.seq = self.seq.wrapping_add(1);
        s
    }
}

// ── Window ───────────────────────────────────────────────────────────────────

/// An on-screen pixel buffer managed by the WM.
pub struct Window {
    pub surface_id: u32,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pixels: Vec<u32>,
}

impl Window {
    /// Create a surface, allocate a pixel buffer, and attach it to the surface.
    pub fn new(app: &mut App, w: u32, h: u32) -> Self {
        let surface_id = {
            let id = sys_surface_create();
            if id < 0 { 0 } else { id as u32 }
        };

        let count = (w * h) as usize;
        let mut pixels: Vec<u32> = Vec::with_capacity(count);
        // Safety: u32 is POD; uninitialized memory is fine for a pixel buffer.
        unsafe { pixels.set_len(count) };
        pixels.iter_mut().for_each(|p| *p = 0);

        if surface_id != 0 {
            let _ = sys_surface_attach(
                surface_id,
                pixels.as_ptr() as *const u8,
                w, h,
                w * 4,
            );
        }

        let win = Window { surface_id, x: 0, y: 0, w, h, pixels };

        // Tell the WM about this surface.
        if app.wm_pid != 0 && surface_id != 0 {
            win.notify_commit(app);
        }

        win
    }

    /// Mutable slice of ARGB pixels (row-major, no padding).
    #[inline]
    pub fn pixels_mut(&mut self) -> &mut [u32] {
        &mut self.pixels
    }

    /// Pixel slice as raw bytes (e.g. for blit helpers).
    #[inline]
    pub fn pixels_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self.pixels.as_ptr() as *const u8,
                self.pixels.len() * 4,
            )
        }
    }

    /// Update geometry when the WM sends a [`PayloadGeometry`] event.
    pub fn apply_geometry(&mut self, geo: &PayloadGeometry) {
        self.x = geo.x;
        self.y = geo.y;
        if geo.w != self.w || geo.h != self.h {
            self.resize(geo.w, geo.h);
        }
    }

    /// Resize the pixel buffer and re-attach to the surface.
    pub fn resize(&mut self, w: u32, h: u32) {
        self.w = w;
        self.h = h;
        let count = (w * h) as usize;
        self.pixels.resize(count, 0);
        if self.surface_id != 0 {
            let _ = sys_surface_attach(
                self.surface_id,
                self.pixels.as_ptr() as *const u8,
                w, h,
                w * 4,
            );
        }
    }

    /// Commit (blit) the pixel buffer to the framebuffer and notify the WM.
    pub fn commit(&mut self, app: &mut App) {
        if self.surface_id != 0 {
            let _ = sys_surface_commit(self.surface_id, self.x as u32, self.y as u32);
        }
        if app.wm_pid != 0 {
            self.notify_commit(app);
        }
    }

    fn notify_commit(&self, app: &mut App) {
        let payload = PayloadSurfaceCommit {
            surface_id: self.surface_id,
            x:          self.x,
            y:          self.y,
            w:          self.w,
            h:          self.h,
            _pad:       [0u8; 36],
        };
        let msg = KwmMsg {
            msg_type:   KwmType::SurfaceCommit as u8,
            flags:      0,
            seq:        app.next_seq(),
            sender_pid: app.pid,
            payload:    libs::KwmPayload { surface_commit: payload },
        };
        let _ = sys_ipc_send(app.wm_pid, &msg, 0);
    }
}

// ── Event ────────────────────────────────────────────────────────────────────

/// Events delivered to an application from the WM.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// Key press or release: (keycode, pressed).
    Key(u8, bool),
    /// Mouse move + button state: (abs_x, abs_y, dx, dy, buttons).
    Mouse(i32, i32, i16, i16, u8),
    /// Window was resized to (w, h).
    Resize(u32, u32),
    /// Focus gained (true) or lost (false).
    Focus(bool),
    /// WM assigned geometry: (x, y, w, h).
    Geometry(i32, i32, u32, u32),
    /// WM requests the window to close.
    Close,
    /// No event available (non-blocking poll returned empty).
    None,
}

fn decode_event(msg: &KwmMsg) -> Event {
    match msg.msg_type {
        t if t == KwmType::EventKey as u8 => {
            let p = unsafe { msg.payload.event_key };
            Event::Key(p.keycode, p.pressed != 0)
        }
        t if t == KwmType::EventMouse as u8 => {
            let p = unsafe { msg.payload.event_mouse };
            Event::Mouse(p.abs_x, p.abs_y, p.dx, p.dy, p.buttons)
        }
        t if t == KwmType::EventResize as u8 => {
            let p = unsafe { msg.payload.event_resize };
            Event::Resize(p.w, p.h)
        }
        t if t == KwmType::EventFocus as u8 => {
            let p = unsafe { msg.payload.event_focus };
            Event::Focus(p.focused != 0)
        }
        t if t == KwmType::Geometry as u8 => {
            let p = unsafe { msg.payload.geometry };
            Event::Geometry(p.x, p.y, p.w, p.h)
        }
        t if t == KwmType::Unregister as u8 => Event::Close,
        _ => Event::None,
    }
}
