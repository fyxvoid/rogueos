//! Window client management — port of dwm's `struct Client`.
//!
//! Uses a [`SlotMap`] generational arena instead of C linked lists for O(1)
//! insert / remove / lookup with safe, stable IDs.

use alloc::string::String;
use slotmap::new_key_type;
use crate::Rect;

new_key_type! {
    /// Stable handle to a managed client window.
    pub struct ClientId;
}

/// A managed X11 window — the Rust equivalent of dwm's `Client` struct.
#[derive(Debug, Clone)]
pub struct Client {
    /// X11 window ID.
    pub win: u32,
    /// Window title (`_NET_WM_NAME` or `WM_NAME`).
    pub name: String,
    /// Tag bitmask — which tags this client belongs to.
    pub tags: u32,
    /// Current geometry (position + size).
    pub geom: Rect,
    /// Saved geometry for fullscreen restore.
    pub old_geom: Rect,
    /// Border width in pixels.
    pub border_width: u32,
    /// Saved border width (before fullscreen).
    pub old_border_width: u32,

    // ── Size hints (from WM_NORMAL_HINTS) ───────────────────────────
    pub base_w: i32,
    pub base_h: i32,
    pub inc_w: i32,
    pub inc_h: i32,
    pub max_w: i32,
    pub max_h: i32,
    pub min_w: i32,
    pub min_h: i32,
    pub min_aspect: f32,
    pub max_aspect: f32,
    pub hints_valid: bool,

    // ── State flags ─────────────────────────────────────────────────
    pub is_floating: bool,
    pub is_fullscreen: bool,
    pub is_urgent: bool,
    pub is_fixed: bool,
    pub never_focus: bool,
    pub old_state_floating: bool,

    /// Per-client size factor (cfact patch, default 1.0).
    pub cfact: f32,

    /// Owning monitor id (index into monitors vec).
    pub mon_id: usize,
}

impl Client {
    /// Create a new client with sensible defaults.
    pub fn new(win: u32, geom: Rect, mon_id: usize) -> Self {
        Self {
            win,
            name: String::new(),
            tags: 0,
            geom,
            old_geom: geom,
            border_width: 0,
            old_border_width: 0,
            base_w: 0,
            base_h: 0,
            inc_w: 0,
            inc_h: 0,
            max_w: 0,
            max_h: 0,
            min_w: 0,
            min_h: 0,
            min_aspect: 0.0,
            max_aspect: 0.0,
            hints_valid: false,
            is_floating: false,
            is_fullscreen: false,
            is_urgent: false,
            is_fixed: false,
            never_focus: false,
            old_state_floating: false,
            cfact: 1.0,
            mon_id,
        }
    }

    /// Total width including borders.
    pub fn full_width(&self) -> u32 {
        self.geom.w + 2 * self.border_width
    }

    /// Total height including borders.
    pub fn full_height(&self) -> u32 {
        self.geom.h + 2 * self.border_width
    }
}
