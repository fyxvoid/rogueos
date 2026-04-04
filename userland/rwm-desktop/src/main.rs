//! RogueWM — native desktop runner using minifb.
//!
//! Runs the full rwm-core WM logic in a real window on the host Linux machine.
//!
//!   Mod (Left-Alt)+1..9     — view tag N
//!   Mod+Shift+1..9          — move focused window to tag N
//!   Mod+j / Mod+k           — focus next / prev
//!   Mod+h / Mod+l           — shrink / grow master factor
//!   Mod+i / Mod+d           — nmaster +/-
//!   Mod+t/m/f/g/b/c/w       — switch layout
//!   Mod+n                   — spawn new client
//!   Mod+Shift+c             — close focused
//!   Mod+Space               — toggle float
//!   Mod+Shift+b             — toggle bar
//!   Mod+Tab                 — previous tag
//!   Mod+0                   — view all tags
//!   Mod+Shift+q             — quit

use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};
use rwm_core::{
    client::Client,
    layout::{builtin_layouts, Arrangement},
    monitor::Monitor,
    state::WmState,
    Rect,
};

// ── Catppuccin Mocha palette ─────────────────────────────────────────────────
const C_CRUST:    u32 = 0x11_11_1b; // deepest bg / shadow
const C_MANTLE:   u32 = 0x18_18_25; // bar bg
const C_BASE:     u32 = 0x1e_1e_2e; // desktop bg
const C_SURF0:    u32 = 0x31_32_44; // window bg
const C_SURF1:    u32 = 0x45_47_5a; // titlebar bg
const C_SURF2:    u32 = 0x58_5b_70; // border unfocused
const C_OVERLAY0: u32 = 0x6c_70_86; // muted text
const C_SUBTEXT:  u32 = 0xa6_ad_c8; // secondary text
const C_TEXT:     u32 = 0xcd_d6_f4; // primary text
const C_BLUE:     u32 = 0x89_b4_fa; // focused accent
const C_LAVENDER: u32 = 0xb4_be_fe; // active tag text
const C_MAUVE:    u32 = 0xcb_a6_f7; // layout symbol
const C_RED:      u32 = 0xf3_8b_a8; // close button
const C_YELLOW:   u32 = 0xf9_e2_af; // minimize button
const C_GREEN:    u32 = 0xa6_e3_a1; // maximize button
const C_PEACH:    u32 = 0xfa_b3_87; // floating / occupied dot

// ── Layout constants ─────────────────────────────────────────────────────────
const BAR_H:       u32 = 34;
const TITLEBAR_H:  u32 = 22;
const BORDER:      u32 = 1;        // 1-px sharp border
const SHADOW:      u32 = 4;        // shadow offset in px
const MFACT_STEP:  f32 = 0.05;
const WIN_W:      usize = 1280;
const WIN_H:      usize = 800;

// Tag pill geometry
const PILL_W:     u32 = 28;
const PILL_H:     u32 = 22;
const PILL_PAD:   u32 = 4;  // horizontal gap between pills
const PILL_OFF_Y: u32 = (BAR_H - PILL_H) / 2;

// ── App name list for spawned clients ────────────────────────────────────────
const APP_NAMES: &[&str] = &[
    "Terminal", "Editor", "Browser", "Files", "Music",
    "Settings", "Calendar", "Notes", "Monitor",
];

// ── Software framebuffer ─────────────────────────────────────────────────────
struct MinifbBackend {
    buf: Vec<u32>,
    w:   u32,
    h:   u32,
    window: Window,
}

impl MinifbBackend {
    fn new(w: usize, h: usize) -> Self {
        let window = Window::new(
            "Kingdom OS — RogueWM",
            w, h,
            WindowOptions { scale: Scale::X1, resize: false, ..Default::default() },
        ).expect("failed to open window");
        Self { buf: vec![0u32; w * h], w: w as u32, h: h as u32, window }
    }

    #[inline]
    fn put(&mut self, x: u32, y: u32, c: u32) {
        if x < self.w && y < self.h {
            self.buf[(y * self.w + x) as usize] = c;
        }
    }

    fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        let x2 = (x + w).min(self.w);
        let y2 = (y + h).min(self.h);
        for row in y.min(self.h)..y2 {
            let s = (row * self.w + x) as usize;
            let e = (row * self.w + x2) as usize;
            self.buf[s..e].fill(color);
        }
    }

    fn clear(&mut self, color: u32) { self.buf.fill(color); }

    /// Rounded filled rectangle.
    fn fill_rounded(&mut self, x: u32, y: u32, w: u32, h: u32, r: u32, color: u32) {
        if r == 0 || w < 2 * r || h < 2 * r {
            self.fill_rect(x, y, w, h, color);
            return;
        }
        let r2 = (r as i64).pow(2);
        for dy in 0..h {
            let (lx, rw) = if dy < r {
                let d = (r - dy) as i64;
                let xoff = ((r2 - d * d) as f64).sqrt() as u32;
                let l = r.saturating_sub(xoff);
                let right = (w - r) + xoff;
                (l, right.saturating_sub(l))
            } else if dy >= h - r {
                let d = (dy - (h - r)) as i64;
                let xoff = ((r2 - d * d) as f64).sqrt() as u32;
                let l = r.saturating_sub(xoff);
                let right = (w - r) + xoff;
                (l, right.saturating_sub(l))
            } else { (0, w) };
            self.fill_rect(x + lx, y + dy, rw, 1, color);
        }
    }

    /// Draw a filled circle (for traffic-light buttons).
    fn fill_circle(&mut self, cx: u32, cy: u32, r: u32, color: u32) {
        let r2 = (r as i64).pow(2);
        for dy in 0..=r * 2 {
            let y = cy.wrapping_add(dy).wrapping_sub(r);
            let d = (dy as i64 - r as i64).abs();
            let xspan = (((r2 - d * d) as f64).sqrt() as u32).min(r);
            for dx in 0..=xspan * 2 {
                let x = cx.wrapping_add(dx).wrapping_sub(xspan);
                self.put(x, y, color);
            }
        }
    }

    /// Alpha-blend a color onto existing pixel (alpha 0–255).
    fn blend_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32, alpha: u32) {
        let x2 = (x + w).min(self.w);
        let y2 = (y + h).min(self.h);
        let sr = (color >> 16) & 0xFF;
        let sg = (color >>  8) & 0xFF;
        let sb =  color        & 0xFF;
        for row in y.min(self.h)..y2 {
            for col in x..x2 {
                let i = (row * self.w + col) as usize;
                let dst = self.buf[i];
                let dr = (dst >> 16) & 0xFF;
                let dg = (dst >>  8) & 0xFF;
                let db =  dst        & 0xFF;
                let r = (sr * alpha + dr * (255 - alpha)) / 255;
                let g = (sg * alpha + dg * (255 - alpha)) / 255;
                let b = (sb * alpha + db * (255 - alpha)) / 255;
                self.buf[i] = (r << 16) | (g << 8) | b;
            }
        }
    }

    fn flush(&mut self) {
        self.window.update_with_buffer(&self.buf, self.w as usize, self.h as usize)
            .expect("window update failed");
    }
    fn is_open(&self)       -> bool       { self.window.is_open() }
    fn keys_down(&self)     -> Vec<Key>   { self.window.get_keys() }
    fn keys_pressed(&self)  -> Vec<Key>   { self.window.get_keys_pressed(KeyRepeat::No) }
}

// ── 4×6 pixel font (full ASCII 0x20–0x7E) ───────────────────────────────────
#[rustfmt::skip]
static GLYPH: [u32; 95] = [
    0x000000, 0x020222, 0x00000A, 0x0AFAFA, 0x07861E, 0x094B26, 0x0D6664, 0x000002,
    0x042112, 0x021224, 0x000A5A, 0x002720, 0x012000, 0x000600, 0x020000, 0x001248,
    0b_0110_1001_1011_1101_1001_0110,
    0b_0100_1100_0100_0100_0100_1110,
    0b_0110_1001_0001_0010_0100_1111,
    0b_1110_0001_0110_0001_0001_1110,
    0b_0010_0110_1010_1111_0010_0010,
    0b_1111_1000_1110_0001_0001_1110,
    0b_0110_1000_1110_1001_1001_0110,
    0b_1111_0001_0010_0010_0100_0100,
    0b_0110_1001_0110_1001_1001_0110,
    0b_0110_1001_0111_0001_0001_0110,
    0x006060, 0x012060, 0x042124, 0x00F0F0, 0x042412, 0x020210,
    0b_0110_1001_1011_1011_1000_0110,
    0x099F96, 0x079797, 0x0E111E, 0x079997, 0x0F171F, 0x01171F, 0x0E9D1E,
    0x099F99, 0x072227, 0x06544E, 0x095359, 0x0F1111, 0x0999F9, 0x099DB9, 0x069996,
    0x011797, 0x0ED996, 0x095797, 0x07861E, 0x02222F, 0x069999, 0x066999, 0x06F999,
    0x096669, 0x022269, 0x0F124F,
    0b_0110_0100_0100_0100_0100_0110,
    0b_1000_1000_0100_0010_0001_0001,
    0b_0110_0010_0010_0010_0010_0110,
    0x000096, 0x0F0000, 0x000004,
    0x099F96, 0x079797, 0x0E111E, 0x079997, 0x0F171F, 0x01171F, 0x0E9D1E,
    0x099F99, 0x072227, 0x06544E, 0x095359, 0x0F1111, 0x0999F9, 0x099DB9, 0x069996,
    0x011797, 0x0ED996, 0x095797, 0x07861E, 0x02222F, 0x069999, 0x066999, 0x06F999,
    0x096669, 0x022269, 0x0F124F,
    0x026226, 0b_0100_0100_0100_0100_0100_0100, 0x064426, 0x00050A,
];

const GW: u32 = 4;
const GH: u32 = 6;

fn glyph(ch: u8) -> u32 {
    if ch < 0x20 || ch > 0x7E { 0 } else { GLYPH[(ch - 0x20) as usize] }
}

/// Draw one character at 1× scale.
fn draw_char(fb: &mut MinifbBackend, px: u32, py: u32, ch: u8, color: u32) {
    let bits = glyph(ch);
    if bits == 0 { return; }
    for row in 0..GH {
        let rb = (bits >> (row * GW)) & 0xF;
        if rb == 0 { continue; }
        let y = py + row; if y >= fb.h { break; }
        let mut col = 0u32;
        while col < GW {
            if (rb >> col) & 1 == 1 {
                let s = col;
                while col < GW && (rb >> col) & 1 == 1 { col += 1; }
                let x0 = (px + s).min(fb.w) as usize;
                let x1 = (px + col).min(fb.w) as usize;
                fb.buf[(y * fb.w) as usize + x0..(y * fb.w) as usize + x1].fill(color);
            } else { col += 1; }
        }
    }
}

/// Draw one character at 2× scale (8×12 pixels).
fn draw_char2(fb: &mut MinifbBackend, px: u32, py: u32, ch: u8, color: u32) {
    let bits = glyph(ch);
    if bits == 0 { return; }
    for row in 0..GH {
        let rb = (bits >> (row * GW)) & 0xF;
        if rb == 0 { continue; }
        let y0 = py + row * 2; if y0 + 1 >= fb.h { break; }
        let mut col = 0u32;
        while col < GW {
            if (rb >> col) & 1 == 1 {
                let s = col;
                while col < GW && (rb >> col) & 1 == 1 { col += 1; }
                let x0 = (px + s * 2) as usize;
                let x1 = (px + col * 2) as usize;
                let w = x1 - x0;
                let base0 = (y0 * fb.w) as usize + x0;
                let base1 = ((y0 + 1) * fb.w) as usize + x0;
                if x1 <= fb.w as usize {
                    fb.buf[base0..base0 + w].fill(color);
                    fb.buf[base1..base1 + w].fill(color);
                }
            } else { col += 1; }
        }
    }
}

/// Draw a string at 2× scale. Returns x after last char.
fn draw_str2(fb: &mut MinifbBackend, mut px: u32, py: u32, s: &str, color: u32) -> u32 {
    for ch in s.bytes() {
        draw_char2(fb, px, py, ch, color);
        px += GW * 2 + 1;
    }
    px
}

/// Draw a string at 1× scale. Returns x after last char.
fn draw_str1(fb: &mut MinifbBackend, mut px: u32, py: u32, s: &str, color: u32) -> u32 {
    for ch in s.bytes() {
        draw_char(fb, px, py, ch, color);
        px += GW + 1;
    }
    px
}

/// Measure string width at 2× scale.
fn str_w2(s: &str) -> u32 { s.len() as u32 * (GW * 2 + 1) }
/// Measure string width at 1× scale.
fn str_w1(s: &str) -> u32 { s.len() as u32 * (GW + 1) }

// ── Color helpers ─────────────────────────────────────────────────────────────
fn lerp_color(a: u32, b: u32, t: f32) -> u32 {
    let lerp = |ca: u32, cb: u32| ((ca as f32 + (cb as f32 - ca as f32) * t) as u32).min(255);
    (lerp((a >> 16) & 0xFF, (b >> 16) & 0xFF) << 16)
    | (lerp((a >> 8) & 0xFF, (b >> 8) & 0xFF) << 8)
    |  lerp(a & 0xFF, b & 0xFF)
}

fn darken(c: u32, amt: u32) -> u32 {
    let d = |x: u32| x.saturating_sub(amt);
    (d((c >> 16) & 0xFF) << 16) | (d((c >> 8) & 0xFF) << 8) | d(c & 0xFF)
}

fn lighten(c: u32, amt: u32) -> u32 {
    let l = |x: u32| x.saturating_add(amt).min(255);
    (l((c >> 16) & 0xFF) << 16) | (l((c >> 8) & 0xFF) << 8) | l(c & 0xFF)
}

// ── Background ────────────────────────────────────────────────────────────────

fn draw_background(fb: &mut MinifbBackend) {
    // Vertical gradient: C_BASE (top) → slightly darker at bottom
    let h = fb.h;
    let w = fb.w;
    for row in 0..h {
        let t = row as f32 / h as f32;
        let c = lerp_color(C_BASE, darken(C_BASE, 12), t);
        let s = (row * w) as usize;
        fb.buf[s..s + w as usize].fill(c);
    }

    // Subtle dot grid (every 24px)
    let dot_color = lighten(C_BASE, 8);
    let mut gy = 0u32;
    while gy < h {
        let mut gx = 0u32;
        while gx < w {
            fb.put(gx, gy, dot_color);
            gx += 24;
        }
        gy += 24;
    }
}

// ── Bar ───────────────────────────────────────────────────────────────────────

fn draw_bar(fb: &mut MinifbBackend, state: &WmState, show_bar: bool) {
    if !show_bar { return; }

    // Bar background
    fb.fill_rect(0, 0, fb.w, BAR_H, C_MANTLE);

    let cur_tags = state.monitors[0].current_tags();
    let text_y2 = (BAR_H - GH * 2) / 2;   // y for 2× glyphs (centred in bar)
    let text_y1 = (BAR_H - GH)     / 2;   // y for 1× glyphs

    // ── Left: "KOS" logo ─────────────────────────────────────────────────────
    let logo = "KOS";
    let lx = 8u32;
    // Accent square
    fb.fill_rounded(lx, (BAR_H - 18) / 2, 18, 18, 4, C_BLUE);
    // "K" in the square
    let kx = lx + (18 - GW * 2) / 2;
    let ky = (BAR_H - GH * 2) / 2;
    draw_char2(fb, kx, ky, b'K', C_MANTLE);
    // "OS" next to it
    draw_str2(fb, lx + 22, text_y2, &logo[1..], C_TEXT);

    // ── Tag pills ────────────────────────────────────────────────────────────
    let mut tag_x = lx + 22 + str_w2(&logo[1..]) + 12;

    for i in 0u32..9 {
        let bit = 1u32 << i;
        let active   = (cur_tags & bit) != 0;
        let occupied = state.clients_on_tag(0, bit) > 0;

        let (pill_bg, num_color) = if active {
            (C_BLUE, C_MANTLE)
        } else if occupied {
            (C_SURF1, C_SUBTEXT)
        } else {
            (C_SURF0, C_OVERLAY0)
        };

        fb.fill_rounded(tag_x, PILL_OFF_Y, PILL_W, PILL_H, PILL_H / 2, pill_bg);

        // Number centred in pill (2× scale)
        let num = b'1' + i as u8;
        let nx = tag_x + (PILL_W - GW * 2) / 2;
        let ny = PILL_OFF_Y + (PILL_H - GH * 2) / 2;
        draw_char2(fb, nx, ny, num, num_color);

        // Occupied dot below number for non-active tags
        if occupied && !active {
            let dot_x = tag_x + PILL_W / 2;
            let dot_y = PILL_OFF_Y + PILL_H - 4;
            fb.fill_circle(dot_x, dot_y, 2, C_PEACH);
        }

        tag_x += PILL_W + PILL_PAD;
    }

    // Vertical divider after tags
    let div_x = tag_x + 4;
    fb.fill_rect(div_x, 6, 1, BAR_H - 12, C_SURF2);

    // ── Layout symbol (after divider) ────────────────────────────────────────
    let sym_x = div_x + 8;
    let sym = state.current_layout(0)
        .map(|l| l.symbol())
        .unwrap_or("[]");
    let sym_end = draw_str1(fb, sym_x, text_y1, sym, C_MAUVE);

    // ── Centred window title ──────────────────────────────────────────────────
    if let Some(cid) = state.monitors[0].focused {
        if let Some(client) = state.clients.get(cid) {
            let title = &client.name;
            let tw = str_w2(title);
            let tx = if fb.w / 2 > tw / 2 { fb.w / 2 - tw / 2 } else { sym_end + 8 };
            draw_str2(fb, tx, text_y2, title, C_TEXT);
            // Floating badge
            if client.is_floating {
                let bx = tx + tw + 6;
                fb.fill_rounded(bx, (BAR_H - 14) / 2, str_w1("float") + 8, 14, 7, darken(C_PEACH, 60));
                draw_str1(fb, bx + 4, text_y1, "float", C_PEACH);
            }
        }
    }

    // ── Right: status text ────────────────────────────────────────────────────
    let status = "kingdom-os";
    let sw = str_w1(status);
    let rx = fb.w.saturating_sub(sw + 10);
    draw_str1(fb, rx, text_y1, status, C_OVERLAY0);

    // Bottom border line (accent)
    fb.fill_rect(0, BAR_H - 1, fb.w, 1, C_SURF2);
}

// ── Window ────────────────────────────────────────────────────────────────────

fn draw_window(fb: &mut MinifbBackend, r: Rect, title: &str, focused: bool) {
    if r.w < 4 || r.h < 4 { return; }
    let x = r.x.max(0) as u32;
    let y = r.y.max(0) as u32;
    let w = r.w;
    let h = r.h;

    // Drop shadow
    fb.blend_rect(
        x + SHADOW, y + SHADOW,
        w.saturating_sub(SHADOW / 2),
        h.saturating_sub(SHADOW / 2),
        C_CRUST, 180,
    );

    // Window body
    let body_color = if focused { lighten(C_SURF0, 6) } else { C_SURF0 };
    fb.fill_rounded(x, y, w, h, 8, body_color);

    // Border
    let border_color = if focused { C_BLUE } else { C_SURF2 };
    // Top
    fb.fill_rect(x, y, w, BORDER, border_color);
    // Bottom
    fb.fill_rect(x, y + h - BORDER, w, BORDER, border_color);
    // Left
    fb.fill_rect(x, y, BORDER, h, border_color);
    // Right
    fb.fill_rect(x + w - BORDER, y, BORDER, h, border_color);

    // Glowing left accent strip on focused window
    if focused {
        fb.fill_rect(x, y, 3, h, C_BLUE);
    }

    // Title bar
    if h > TITLEBAR_H + BORDER * 2 {
        let tb_y = y + BORDER;
        let tb_h = TITLEBAR_H;
        let tb_color = if focused { lighten(C_SURF1, 8) } else { C_SURF1 };

        // Title bar background (rounded top only via overdraw)
        fb.fill_rect(x + BORDER, tb_y, w - BORDER * 2, tb_h, tb_color);

        // Separator under title bar
        fb.fill_rect(x + BORDER, tb_y + tb_h, w - BORDER * 2, 1,
            if focused { darken(C_BLUE, 100) } else { C_SURF2 });

        // Traffic light buttons (8px diameter circles)
        let btn_y = tb_y + tb_h / 2;
        let btn_start = x + BORDER + 10;
        fb.fill_circle(btn_start,      btn_y, 5, C_RED);
        fb.fill_circle(btn_start + 16, btn_y, 5, C_YELLOW);
        fb.fill_circle(btn_start + 32, btn_y, 5, C_GREEN);

        // Window title centered in title bar
        if !title.is_empty() {
            let tw = str_w1(title);
            let max_title_w = w.saturating_sub(btn_start - x + 32 + 16 + 8);
            let _ = max_title_w; // we just clip via min(fb.w)
            let tx = if w / 2 > tw / 2 { x + w / 2 - tw / 2 } else { btn_start + 44 };
            let ty = tb_y + (tb_h - GH) / 2;
            let title_color = if focused { C_TEXT } else { C_OVERLAY0 };
            draw_str1(fb, tx, ty, title, title_color);
        }

        // Content area — subtle horizontal scanlines for depth
        let content_y = tb_y + tb_h + 1 + BORDER;
        let content_h = h.saturating_sub(TITLEBAR_H + BORDER * 3 + 1);
        if content_h > 4 {
            // Even rows slightly lighter — very subtle texture
            let mut cy = content_y;
            while cy < content_y + content_h {
                if (cy - content_y) % 2 == 0 {
                    let scanline = lighten(body_color, 2);
                    fb.fill_rect(x + BORDER, cy, w - BORDER * 2, 1, scanline);
                }
                cy += 1;
            }
        }
    }
}

// ── Full redraw ───────────────────────────────────────────────────────────────

fn redraw(fb: &mut MinifbBackend, state: &WmState, show_bar: bool) {
    draw_background(fb);

    let focused_cid = state.monitors[0].focused;
    let area        = state.monitors[0].window_area;
    let cur_tags    = state.monitors[0].current_tags();

    // Tiled arrangement
    let visible_tiled: Vec<_> = state.visible_tiled(0);
    let arrangement: Arrangement = if let Some(layout) = state.current_layout(0) {
        layout.arrange(&state.monitors[0], &visible_tiled, area)
    } else {
        visible_tiled.iter().map(|&(cid, _)| (cid, area)).collect()
    };

    // Floating/fullscreen
    let mut floating: Vec<(rwm_core::ClientId, Rect)> = Vec::new();
    for (cid, client) in &state.clients {
        if !rwm_core::is_visible(client.tags, cur_tags) { continue; }
        if client.is_floating || client.is_fullscreen {
            floating.push((cid, client.geom));
        }
    }

    // Render: unfocused tiled → unfocused floating → focused
    let mut focused_draw: Option<(Rect, String)> = None;

    for (cid, geom) in &arrangement {
        let is_foc  = Some(*cid) == focused_cid;
        let title   = state.clients.get(*cid).map(|c| c.name.clone()).unwrap_or_default();
        if is_foc {
            focused_draw = Some((*geom, title));
        } else {
            draw_window(fb, *geom, &title, false);
        }
    }

    for (cid, geom) in &floating {
        let is_foc  = Some(*cid) == focused_cid;
        let title   = state.clients.get(*cid).map(|c| c.name.clone()).unwrap_or_default();
        if is_foc {
            focused_draw = Some((*geom, title));
        } else {
            draw_window(fb, *geom, &title, false);
        }
    }

    // Draw focused on top
    if let Some((geom, title)) = focused_draw {
        draw_window(fb, geom, &title, true);
    }

    draw_bar(fb, state, show_bar);
    fb.flush();
}

// ── WM action helpers ─────────────────────────────────────────────────────────

fn focus_stack(state: &mut WmState, dir: i32) {
    let tags = state.monitors[0].current_tags();
    let visible: Vec<_> = state.monitors[0].clients.iter()
        .filter(|&&c| state.clients.get(c)
            .map(|cl| rwm_core::is_visible(cl.tags, tags)).unwrap_or(false))
        .copied().collect();
    if visible.is_empty() { return; }
    let cur = state.monitors[0].focused;
    let pos = cur.and_then(|c| visible.iter().position(|&id| id == c)).unwrap_or(0);
    let next = if dir > 0 { (pos + 1) % visible.len() }
               else { (pos + visible.len() - 1) % visible.len() };
    state.monitors[0].focused = Some(visible[next]);
    state.monitors[0].raise_in_stack(visible[next]);
}

fn view_tag(state: &mut WmState, tag: u32) {
    let bit = 1u32 << (tag.saturating_sub(1));
    state.view_tags(0, bit);
    let tags = state.monitors[0].current_tags();
    state.monitors[0].focused = state.monitors[0].clients.iter()
        .find(|&&c| state.clients.get(c)
            .map(|cl| rwm_core::is_visible(cl.tags, tags)).unwrap_or(false))
        .copied();
}

fn move_to_tag(state: &mut WmState, tag: u32) {
    let bit = 1u32 << (tag.saturating_sub(1));
    if let Some(cid) = state.monitors[0].focused {
        state.tag_client(cid, bit);
        let tags = state.monitors[0].current_tags();
        state.monitors[0].focused = state.monitors[0].clients.iter()
            .find(|&&c| state.clients.get(c)
                .map(|cl| rwm_core::is_visible(cl.tags, tags)).unwrap_or(false))
            .copied();
    }
}

fn set_layout(state: &mut WmState, name: &str) {
    if let Some(idx) = state.layouts.iter().position(|l| l.name() == name) {
        let mon = &mut state.monitors[0];
        mon.layout[mon.sel_layout] = rwm_core::layout::LayoutId(idx);
        mon.layout_symbol = state.layouts[idx].symbol().to_string();
    }
}

fn adjust_mfact(state: &mut WmState, delta: f32) {
    state.monitors[0].mfact = (state.monitors[0].mfact + delta).clamp(0.1, 0.9);
}

fn spawn_client(state: &mut WmState) {
    let tags = state.monitors[0].current_tags();
    let win  = state.clients.len() as u32;
    let name = APP_NAMES[win as usize % APP_NAMES.len()];
    let mut c = Client::new(win, Rect::new(0, 0, 200, 150), 0);
    c.tags = tags;
    c.border_width = BORDER;
    c.name = name.to_string();
    let cid = state.add_client(c, 0);
    state.monitors[0].focused = Some(cid);
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let mut fb = MinifbBackend::new(WIN_W, WIN_H);

    let mut state = WmState::new();
    state.screen_w = WIN_W as u32;
    state.screen_h = WIN_H as u32;
    state.bar_height = BAR_H;
    state.layouts = builtin_layouts(true);

    let mon_geom = Rect::new(0, 0, WIN_W as u32, WIN_H as u32);
    let mut mon = Monitor::new(0, mon_geom);
    mon.update_bar_pos(BAR_H);
    mon.mfact = 0.55;
    mon.nmaster = 1;
    state.monitors.push(mon);
    state.sel_mon = 0;

    // Pre-populate one client per tag with descriptive names
    for (i, &name) in APP_NAMES.iter().enumerate().take(9) {
        let bit = 1u32 << i;
        let mut c = Client::new(i as u32, Rect::new(0, 0, 100, 100), 0);
        c.tags = bit;
        c.border_width = BORDER;
        c.name = name.to_string();
        let cid = state.add_client(c, 0);
        if i == 0 { state.monitors[0].focused = Some(cid); }
    }

    let mut show_bar = true;
    redraw(&mut fb, &state, show_bar);

    let mut mod_held   = false;
    let mut shift_held = false;
    let mut ctrl_held  = false;

    while fb.is_open() {
        let down = fb.keys_down();
        mod_held   = down.contains(&Key::LeftAlt)   || down.contains(&Key::RightAlt);
        shift_held = down.contains(&Key::LeftShift)  || down.contains(&Key::RightShift);
        ctrl_held  = down.contains(&Key::LeftCtrl)   || down.contains(&Key::RightCtrl);

        let pressed = fb.keys_pressed();
        if pressed.contains(&Key::Escape) { break; }

        let mut dirty = false;

        if mod_held {
            for &key in &pressed {
                match key {
                    Key::Key1 if !shift_held && !ctrl_held => { view_tag(&mut state, 1); dirty = true; }
                    Key::Key2 if !shift_held && !ctrl_held => { view_tag(&mut state, 2); dirty = true; }
                    Key::Key3 if !shift_held && !ctrl_held => { view_tag(&mut state, 3); dirty = true; }
                    Key::Key4 if !shift_held && !ctrl_held => { view_tag(&mut state, 4); dirty = true; }
                    Key::Key5 if !shift_held && !ctrl_held => { view_tag(&mut state, 5); dirty = true; }
                    Key::Key6 if !shift_held && !ctrl_held => { view_tag(&mut state, 6); dirty = true; }
                    Key::Key7 if !shift_held && !ctrl_held => { view_tag(&mut state, 7); dirty = true; }
                    Key::Key8 if !shift_held && !ctrl_held => { view_tag(&mut state, 8); dirty = true; }
                    Key::Key9 if !shift_held && !ctrl_held => { view_tag(&mut state, 9); dirty = true; }
                    Key::Key0 if !shift_held => {
                        state.view_tags(0, rwm_core::TAGMASK); dirty = true;
                    }
                    Key::Tab => {
                        state.monitors[0].sel_tags ^= 1; dirty = true;
                    }
                    Key::Key1 if ctrl_held && !shift_held => { state.toggle_view(0, 1<<0); dirty = true; }
                    Key::Key2 if ctrl_held && !shift_held => { state.toggle_view(0, 1<<1); dirty = true; }
                    Key::Key3 if ctrl_held && !shift_held => { state.toggle_view(0, 1<<2); dirty = true; }
                    Key::Key4 if ctrl_held && !shift_held => { state.toggle_view(0, 1<<3); dirty = true; }
                    Key::Key5 if ctrl_held && !shift_held => { state.toggle_view(0, 1<<4); dirty = true; }
                    Key::Key6 if ctrl_held && !shift_held => { state.toggle_view(0, 1<<5); dirty = true; }
                    Key::Key7 if ctrl_held && !shift_held => { state.toggle_view(0, 1<<6); dirty = true; }
                    Key::Key8 if ctrl_held && !shift_held => { state.toggle_view(0, 1<<7); dirty = true; }
                    Key::Key9 if ctrl_held && !shift_held => { state.toggle_view(0, 1<<8); dirty = true; }
                    Key::Key1 if shift_held && !ctrl_held => { move_to_tag(&mut state, 1); dirty = true; }
                    Key::Key2 if shift_held && !ctrl_held => { move_to_tag(&mut state, 2); dirty = true; }
                    Key::Key3 if shift_held && !ctrl_held => { move_to_tag(&mut state, 3); dirty = true; }
                    Key::Key4 if shift_held && !ctrl_held => { move_to_tag(&mut state, 4); dirty = true; }
                    Key::Key5 if shift_held && !ctrl_held => { move_to_tag(&mut state, 5); dirty = true; }
                    Key::Key6 if shift_held && !ctrl_held => { move_to_tag(&mut state, 6); dirty = true; }
                    Key::Key7 if shift_held && !ctrl_held => { move_to_tag(&mut state, 7); dirty = true; }
                    Key::Key8 if shift_held && !ctrl_held => { move_to_tag(&mut state, 8); dirty = true; }
                    Key::Key9 if shift_held && !ctrl_held => { move_to_tag(&mut state, 9); dirty = true; }
                    Key::J => { focus_stack(&mut state,  1); dirty = true; }
                    Key::K => { focus_stack(&mut state, -1); dirty = true; }
                    Key::Enter => {
                        let tags = state.monitors[0].current_tags();
                        let tiled: Vec<_> = state.monitors[0].clients.iter()
                            .filter(|&&c| state.clients.get(c).map(|cl|
                                rwm_core::is_visible(cl.tags, tags) && !cl.is_floating && !cl.is_fullscreen)
                                .unwrap_or(false))
                            .copied().collect();
                        if tiled.len() >= 2 {
                            if let Some(foc) = state.monitors[0].focused {
                                let master = tiled[0];
                                let other  = if master == foc { tiled[1] } else { foc };
                                if let (Some(a), Some(b)) = (state.clients.get(master), state.clients.get(other)) {
                                    let ta = a.tags; let tb = b.tags;
                                    if let Some(a) = state.clients.get_mut(master) { a.tags = tb; }
                                    if let Some(b) = state.clients.get_mut(other)  { b.tags = ta; }
                                }
                                state.monitors[0].focused = Some(other);
                            }
                        }
                        dirty = true;
                    }
                    Key::H if !shift_held => { adjust_mfact(&mut state, -MFACT_STEP); dirty = true; }
                    Key::L if !shift_held => { adjust_mfact(&mut state,  MFACT_STEP); dirty = true; }
                    Key::I if !shift_held => { state.monitors[0].nmaster += 1; dirty = true; }
                    Key::D if !shift_held => {
                        if state.monitors[0].nmaster > 0 { state.monitors[0].nmaster -= 1; }
                        dirty = true;
                    }
                    Key::T if !shift_held => { set_layout(&mut state, "tile");           dirty = true; }
                    Key::M if !shift_held => { set_layout(&mut state, "monocle");        dirty = true; }
                    Key::F if !shift_held => { set_layout(&mut state, "spiral");         dirty = true; }
                    Key::W if !shift_held => { set_layout(&mut state, "dwindle");        dirty = true; }
                    Key::G if !shift_held => { set_layout(&mut state, "grid");           dirty = true; }
                    Key::B if !shift_held => { set_layout(&mut state, "bstack");         dirty = true; }
                    Key::C if !shift_held => { set_layout(&mut state, "centeredmaster"); dirty = true; }
                    Key::B if shift_held  => { show_bar = !show_bar;                     dirty = true; }
                    Key::Space => {
                        if let Some(cid) = state.monitors[0].focused {
                            if let Some(c) = state.clients.get_mut(cid) {
                                c.is_floating = !c.is_floating;
                            }
                        }
                        dirty = true;
                    }
                    Key::C if shift_held => {
                        if let Some(cid) = state.monitors[0].focused {
                            state.remove_client(cid);
                        }
                        dirty = true;
                    }
                    Key::N => { spawn_client(&mut state); dirty = true; }
                    Key::Q if shift_held => { return; }
                    _ => {}
                }
            }
        }

        if dirty {
            redraw(&mut fb, &state, show_bar);
        } else {
            fb.flush();
        }

        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
