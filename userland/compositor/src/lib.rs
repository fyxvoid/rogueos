//! Compositor: draws background and windows with config-driven transparency and rounded corners.
//! Runtime state (current alpha, corner radius) is clamped to config and updated by shortcuts.

#![no_std]

use userland_core::{Config, CornerRadius, DisplayBackend, Transparency};

/// Compositor state: current transparency and corner radius (clamped to config).
pub struct Compositor {
    pub transparency: Transparency,
    pub corner_radius: CornerRadius,
}

impl Compositor {
    pub fn new(config: &Config) -> Self {
        Self {
            transparency: config.clamp_transparency(config.transparency.default),
            corner_radius: config.clamp_corner_radius(config.corner_radius.default),
        }
    }

    /// Increase transparency (more opaque); clamp to config max.
    pub fn increase_transparency(&mut self, config: &Config) {
        let next = self.transparency.saturating_add(16).min(255);
        self.transparency = config.clamp_transparency(next);
    }

    /// Decrease transparency (more transparent); clamp to config min.
    pub fn decrease_transparency(&mut self, config: &Config) {
        let next = self.transparency.saturating_sub(16);
        self.transparency = config.clamp_transparency(next);
    }

    /// Increase corner radius; clamp to config max.
    pub fn increase_corner_radius(&mut self, config: &Config) {
        let next = self.corner_radius.saturating_add(2);
        self.corner_radius = config.clamp_corner_radius(next);
    }

    /// Decrease corner radius; clamp to config min.
    pub fn decrease_corner_radius(&mut self, config: &Config) {
        let next = self.corner_radius.saturating_sub(2);
        self.corner_radius = config.clamp_corner_radius(next);
    }
}

/// A window/surface to composite: position, size, and colors (no per-window alpha/corner yet; use global).
#[derive(Clone, Copy)]
pub struct WindowRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub fill_color: u32,
    pub border_color: u32,
    pub border_px: u32,
}

/// Alpha-blend a single ARGB pixel `src` over `dst` using `alpha` (0=transparent, 255=opaque).
/// Ignores the alpha channel already in `src`; uses the supplied `alpha` argument.
/// Formula: out = (src * alpha + dst * (255 - alpha)) / 255  per channel.
#[inline]
pub fn blend_pixel(dst: u32, src: u32, alpha: u8) -> u32 {
    if alpha == 255 {
        return (src & 0x00FF_FFFF) | 0xFF00_0000;
    }
    if alpha == 0 {
        return dst;
    }
    let a = alpha as u32;
    let ia = 255 - a;
    let r = ((src >> 16 & 0xFF) * a + (dst >> 16 & 0xFF) * ia) / 255;
    let g = ((src >>  8 & 0xFF) * a + (dst >>  8 & 0xFF) * ia) / 255;
    let b = ((src       & 0xFF) * a + (dst       & 0xFF) * ia) / 255;
    0xFF00_0000 | (r << 16) | (g << 8) | b
}

/// Alpha-blend a surface pixel buffer onto a destination framebuffer slice.
///
/// `dst`    — flat ARGB pixel slice for the output frame (stride = screen_w).
/// `src`    — surface pixel data (32bpp ARGB, row-major).
/// `dst_x`, `dst_y` — top-left placement in destination.
/// `src_w`, `src_h`, `src_stride` — surface dimensions (stride in pixels).
/// `screen_w`, `screen_h`         — destination bounds.
/// `alpha`  — global alpha for this surface (compositor.transparency).
pub fn blend_surface_into(
    dst: &mut [u32],
    src: &[u32],
    dst_x: i32,
    dst_y: i32,
    src_w: u32,
    src_h: u32,
    src_stride: u32,
    screen_w: u32,
    screen_h: u32,
    alpha: u8,
) {
    for row in 0..src_h {
        let dy = dst_y + row as i32;
        if dy < 0 || dy >= screen_h as i32 {
            continue;
        }
        for col in 0..src_w {
            let dx = dst_x + col as i32;
            if dx < 0 || dx >= screen_w as i32 {
                continue;
            }
            let si = (row * src_stride + col) as usize;
            let di = (dy as u32 * screen_w + dx as u32) as usize;
            if si < src.len() && di < dst.len() {
                dst[di] = blend_pixel(dst[di], src[si], alpha);
            }
        }
    }
}

/// Draw full scene: clear to background, then draw each window with rounded corners.
/// `compositor.transparency` is now used for alpha blending (255 = fully opaque).
pub fn composite<B: DisplayBackend>(
    backend: &mut B,
    bg_color: u32,
    compositor: &Compositor,
    windows: &[WindowRect],
) {
    backend.clear(bg_color);
    let r = compositor.corner_radius;
    let alpha = compositor.transparency; // 255 = fully opaque
    for w in windows {
        if w.w == 0 || w.h == 0 {
            continue;
        }
        // For now: use fill_rect_rounded with alpha-tinted colour.
        // When surface pixel buffers are available, blend_surface_into() is used instead.
        let fill = apply_alpha_to_color(w.fill_color, alpha);
        backend.fill_rect_rounded(w.x, w.y, w.w, w.h, r, fill);

        if w.border_px > 0 && w.w > 2 * w.border_px && w.h > 2 * w.border_px {
            let (x, y, ww, hh, b) = (w.x, w.y, w.w, w.h, w.border_px);
            backend.fill_rect(x, y, ww, b, w.border_color);
            backend.fill_rect(x, y + hh - b, ww, b, w.border_color);
            backend.fill_rect(x, y, b, hh, w.border_color);
            backend.fill_rect(x + ww - b, y, b, hh, w.border_color);
        }
    }
    backend.flush();
}

/// Apply global alpha to an ARGB colour by scaling the RGB channels.
/// alpha=255 → unchanged, alpha=128 → 50% darkened toward background.
#[inline]
fn apply_alpha_to_color(color: u32, alpha: u8) -> u32 {
    if alpha == 255 {
        return color;
    }
    let a = alpha as u32;
    let r = (color >> 16 & 0xFF) * a / 255;
    let g = (color >>  8 & 0xFF) * a / 255;
    let b = (color       & 0xFF) * a / 255;
    0xFF00_0000 | (r << 16) | (g << 8) | b
}

#[cfg(test)]
mod tests {
    use super::*;
    use userland_core::HeadlessBackend;

    #[test]
    fn composite_runs_without_panic() {
        let config = Config::default();
        let compositor = Compositor::new(&config);
        let mut backend = HeadlessBackend;
        let windows = [
            WindowRect {
                x: 10,
                y: 10,
                w: 100,
                h: 80,
                fill_color: 0xFF_80_80_80,
                border_color: 0xFF_40_40_40,
                border_px: 2,
            },
        ];
        composite(&mut backend, 0xFF_20_20_20, &compositor, &windows);
    }

    #[test]
    fn compositor_shortcuts_clamp() {
        let config = Config::default();
        let mut comp = Compositor::new(&config);
        comp.decrease_corner_radius(&config);
        comp.decrease_corner_radius(&config);
        assert!(comp.corner_radius >= config.corner_radius.min);
        comp.increase_corner_radius(&config);
        comp.increase_corner_radius(&config);
        assert!(comp.corner_radius <= config.corner_radius.max);
    }
}
