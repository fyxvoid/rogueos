//! Monitor (physical screen) management — port of dwm's `struct Monitor`.

use alloc::string::String;
use alloc::vec::Vec;
use crate::client::ClientId;
use crate::layout::LayoutId;
use crate::Rect;

/// Index-based monitor handle.
pub type MonitorId = usize;

/// A physical display — the Rust equivalent of dwm's `Monitor` struct.
#[derive(Debug, Clone)]
pub struct Monitor {
    /// Monitor index.
    pub id: MonitorId,
    /// Full monitor geometry.
    pub geom: Rect,
    /// Usable window area (monitor minus bar).
    pub window_area: Rect,

    // ── Tag state ───────────────────────────────────────────────────
    /// Two tagsets for toggling (current & previous), just like dwm.
    pub tagset: [u32; 2],
    /// Index into `tagset` — 0 or 1.
    pub sel_tags: usize,

    // ── Layout ──────────────────────────────────────────────────────
    /// Current and previous layout (for toggling).
    pub layout: [LayoutId; 2],
    /// Index into `layout` — 0 or 1.
    pub sel_layout: usize,
    /// Layout symbol string (e.g. "[]=", "[M]").
    pub layout_symbol: String,

    // ── Master/stack parameters ─────────────────────────────────────
    /// Master area factor (0.05 .. 0.95).
    pub mfact: f32,
    /// Number of windows in the master area.
    pub nmaster: u32,

    // ── Gaps (vanitygaps) ───────────────────────────────────────────
    pub gap_inner_h: i32,
    pub gap_inner_v: i32,
    pub gap_outer_h: i32,
    pub gap_outer_v: i32,

    // ── Bar ─────────────────────────────────────────────────────────
    /// Bar X11 window ID.
    pub bar_win: u32,
    /// Bar Y position.
    pub bar_y: i32,
    pub show_bar: bool,
    pub top_bar: bool,

    // ── Client tracking ─────────────────────────────────────────────
    /// Clients on this monitor, in creation order.
    pub clients: Vec<ClientId>,
    /// Focus-order stack (most recent first).
    pub stack: Vec<ClientId>,
    /// Currently focused client.
    pub focused: Option<ClientId>,
}

impl Monitor {
    pub fn new(id: MonitorId, geom: Rect) -> Self {
        Self {
            id,
            geom,
            window_area: geom,
            tagset: [1, 1], // start viewing tag 1
            sel_tags: 0,
            layout: [LayoutId(0), LayoutId(0)],
            sel_layout: 0,
            layout_symbol: String::from("[]="),
            mfact: 0.55,
            nmaster: 1,
            gap_inner_h: 4,
            gap_inner_v: 4,
            gap_outer_h: 4,
            gap_outer_v: 4,
            bar_win: 0,
            bar_y: 0,
            show_bar: true,
            top_bar: true,
            clients: Vec::new(),
            stack: Vec::new(),
            focused: None,
        }
    }

    /// Current active tagset bitmask.
    pub fn current_tags(&self) -> u32 {
        self.tagset[self.sel_tags]
    }

    /// Current active layout id.
    pub fn current_layout(&self) -> LayoutId {
        self.layout[self.sel_layout]
    }

    /// Update window area based on bar visibility.
    pub fn update_bar_pos(&mut self, bar_height: u32) {
        if self.show_bar {
            self.window_area = Rect {
                x: self.geom.x,
                y: if self.top_bar {
                    self.geom.y + bar_height as i32
                } else {
                    self.geom.y
                },
                w: self.geom.w,
                h: self.geom.h - bar_height,
            };
            self.bar_y = if self.top_bar {
                self.geom.y
            } else {
                self.geom.y + self.window_area.h as i32
            };
        } else {
            self.window_area = self.geom;
            self.bar_y = -(bar_height as i32); // offscreen
        }
    }

    /// Promote a client to the top of the focus stack.
    pub fn raise_in_stack(&mut self, cid: ClientId) {
        self.stack.retain(|&id| id != cid);
        self.stack.insert(0, cid);
    }
}
