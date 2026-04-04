//! Display backend trait: same code runs on RogueOS (sys_fb_*) or host (X11).
//! Input is handled by main loop (sys_poll_input or X11); backend only draws.

/// Display backend: clear, fill_rect, flush, screen size.
/// fill_rect_rounded draws a rectangle with rounded corners (radius r); r=0 means sharp.
pub trait DisplayBackend {
    fn screen_size(&self) -> (u32, u32);
    fn clear(&mut self, color: u32);
    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32);
    /// Draw rectangle with rounded corners. r=0 is equivalent to fill_rect.
    fn fill_rect_rounded(&mut self, x: u32, y: u32, w: u32, h: u32, r: u32, color: u32) {
        if r == 0 || w <= 2 * r || h <= 2 * r {
            self.fill_rect(x, y, w, h, color);
            return;
        }
        // Default: draw as multiple rects (center + 4 corners approximated by scanlines).
        for dy in 0..h {
            let (start, len) = rounded_rect_scan(r, w, h, dy);
            if len > 0 {
                self.fill_rect(x + start, y + dy, len, 1, color);
            }
        }
    }
    fn flush(&mut self);
}

/// For a rounded rect of size (w, h) with corner radius r, at scanline dy (0..h),
/// return (x_offset, length) of the span to draw. No_std compatible.
#[inline]
fn rounded_rect_scan(r: u32, w: u32, h: u32, dy: u32) -> (u32, u32) {
    if r == 0 {
        return (0, w);
    }
    if dy >= h {
        return (0, 0);
    }
    let r2 = (r as u64) * (r as u64);
    // Top band: dy in 0..r. Top-left quarter circle center (r,r): (x-r)^2 + (dy-r)^2 <= r^2 => x >= r - sqrt(r^2 - (r-dy)^2).
    let y_in_top = dy < r;
    let y_in_bottom = dy >= h.saturating_sub(r);
    let (start, len) = if y_in_top {
        let d = r - dy; // 0..=r
        let d2 = (d as u64) * (d as u64);
        let diff = r2.saturating_sub(d2);
        let x_off = isqrt_u64(diff) as u32;
        let left = r.saturating_sub(x_off);
        let right = (w - r).saturating_add(x_off);
        (left, right.saturating_sub(left))
    } else if y_in_bottom {
        // Bottom band: dy in (h-r)..h. Bottom-left circle center (r, h-r); vertical offset d = dy - (h - r).
        let d = dy - (h - r);
        let d2 = (d as u64) * (d as u64);
        let diff = r2.saturating_sub(d2);
        let x_off = isqrt_u64(diff) as u32;
        let left = r.saturating_sub(x_off);
        let right = (w - r).saturating_add(x_off);
        (left, right.saturating_sub(left))
    } else {
        (0, w)
    };
    (start, len)
}

/// Integer square root (no_std).
#[inline]
fn isqrt_u64(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Headless backend for tests: no-op draw, fixed size.
pub struct HeadlessBackend;

impl DisplayBackend for HeadlessBackend {
    fn screen_size(&self) -> (u32, u32) {
        (1280, 800)
    }
    fn clear(&mut self, _color: u32) {}
    fn fill_rect(&mut self, _x: u32, _y: u32, _w: u32, _h: u32, _color: u32) {}
    fn flush(&mut self) {}
}
