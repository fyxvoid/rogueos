//! Window manager: window list, focus, and shortcut-to-action mapping.
//! No user-defined actions; config drives key bindings.

#![no_std]

use userland_core::{Config, ShortcutAction};
use userland_compositor::WindowRect;

/// Single window: position, size, colors.
pub struct Window {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub fill_color: u32,
    pub border_color: u32,
    pub border_px: u32,
}

impl Window {
    pub const fn new(x: u32, y: u32, w: u32, h: u32, fill: u32, border: u32, border_px: u32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            fill_color: fill,
            border_color: border,
            border_px,
        }
    }
}

/// WM state: windows and focus index.
pub struct Wm {
    pub windows: &'static [Window],
    pub focused: usize,
}

impl Wm {
    pub const fn new(windows: &'static [Window]) -> Self {
        Self {
            windows,
            focused: 0,
        }
    }

    pub fn focus_left(&mut self) {
        if self.focused > 0 {
            self.focused -= 1;
        }
    }

    pub fn focus_right(&mut self) {
        if self.focused + 1 < self.windows.len() {
            self.focused += 1;
        }
    }

    /// Fill `buf` with window rects (active window uses active_fill/active_border). Returns count.
    pub fn window_rects(
        &self,
        active_fill: u32,
        active_border: u32,
        buf: &mut [WindowRect],
    ) -> usize {
        let n = self.windows.len().min(buf.len());
        for (i, w) in self.windows.iter().take(n).enumerate() {
            let active = i == self.focused;
            buf[i] = WindowRect {
                x: w.x,
                y: w.y,
                w: w.w,
                h: w.h,
                fill_color: if active { active_fill } else { w.fill_color },
                border_color: if active { active_border } else { w.border_color },
                border_px: w.border_px,
            };
        }
        n
    }
}

/// Map keycode to ShortcutAction using config. Returns None if no shortcut.
pub fn key_to_action(config: &Config, keycode: u8) -> Option<ShortcutAction> {
    let actions = [
        ShortcutAction::IncreaseTransparency,
        ShortcutAction::DecreaseTransparency,
        ShortcutAction::IncreaseCornerRadius,
        ShortcutAction::DecreaseCornerRadius,
        ShortcutAction::Screenshot,
        ShortcutAction::Lock,
        ShortcutAction::ClipboardPaste,
        ShortcutAction::FocusLeft,
        ShortcutAction::FocusRight,
        ShortcutAction::Confirm,
        ShortcutAction::Exit,
    ];
    for &action in &actions {
        if config.key_for(action) == keycode {
            return Some(action);
        }
    }
    None
}
