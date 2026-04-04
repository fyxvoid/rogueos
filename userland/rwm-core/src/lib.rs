//! # rwm-core
//!
//! Core types and traits for **roguewm** — a dwm-inspired tiling window manager.
//!
//! This crate contains no X11 or rendering code; it is a pure data-model and
//! trait library so that the rest of the workspace can depend on it without
//! pulling in platform specifics.
//!
//! Compiled as `no_std + alloc` so it can run on the RogueOS bare-metal userland.

#![no_std]
extern crate alloc;

pub mod client;
pub mod monitor;
pub mod layout;
pub mod event;
pub mod state;

// Re-exports for ergonomic use
pub use client::{Client, ClientId};
pub use monitor::{Monitor, MonitorId};
pub use layout::{Layout, LayoutId};
pub use event::{WmEvent, EventBus};
pub use state::WmState;

// ── Geometry ──────────────────────────────────────────────────────────

/// Axis-aligned rectangle used for window and monitor geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    /// Area of intersection with another rectangle.
    pub fn intersect_area(&self, other: &Rect) -> i64 {
        let x_overlap = 0i32.max(
            (self.x + self.w as i32).min(other.x + other.w as i32)
                - self.x.max(other.x),
        );
        let y_overlap = 0i32.max(
            (self.y + self.h as i32).min(other.y + other.h as i32)
                - self.y.max(other.y),
        );
        x_overlap as i64 * y_overlap as i64
    }

    /// Shrink by uniform insets (gaps).
    pub fn inset(&self, top: i32, right: i32, bottom: i32, left: i32) -> Rect {
        Rect {
            x: self.x + left,
            y: self.y + top,
            w: (self.w as i32 - left - right).max(1) as u32,
            h: (self.h as i32 - top - bottom).max(1) as u32,
        }
    }
}

// ── Tag Constants ─────────────────────────────────────────────────────

/// Maximum number of tags (dwm uses 9).
pub const TAG_COUNT: usize = 9;

/// Bitmask covering all tags: `0b1_1111_1111` = 0x1FF.
pub const TAGMASK: u32 = (1 << TAG_COUNT) - 1;

/// The hidden scratch-tag (bit 9, beyond visible tags).
pub const SCRATCHTAG: u32 = 1 << TAG_COUNT;

/// Check if a client with `client_tags` is visible on the given `view_tags`.
#[inline]
pub fn is_visible(client_tags: u32, view_tags: u32) -> bool {
    (client_tags & view_tags) != 0
}
