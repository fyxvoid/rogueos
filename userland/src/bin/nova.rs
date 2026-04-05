//! nova — RogueOS Native Compositor
//!
//! Nova is the primary compositor for RogueOS.  It owns the GPU framebuffer
//! exclusively, implements a dwm-inspired tiling window manager, and speaks
//! the RwmMsg IPC protocol to coordinate with client applications.
//!
//! ## Architecture
//!
//! ```
//! Client apps ──RwmType::Register──► nova ──RwmType::Geometry──► apps
//! Client apps ──RwmType::SurfaceCommit──────────────────────────►│
//!                                                                  │
//!                                                                  ▼
//!                                                        SYS_SURFACE_COMMIT
//!                                                                  │
//!                                                                  ▼
//!                                                     GOP Framebuffer
//! ```
//!
//! ## Key design points
//!
//! - Calls `SYS_CLAIM_COMPOSITOR` at startup — becomes the sole owner of
//!   the kernel surface compositor.
//! - Embedded 8×8 bitmap font — no dynamic allocation required.
//! - No fork(), no dynamic memory: all state lives in fixed-size arrays.
//! - Capability-gated: nova requires `cap::COMPOSITOR_WM` to be spawned.
//!
//! ## Layouts
//!
//! | Symbol | Name           |
//! |--------|----------------|
//! | []=    | Tile (master+stack) |
//! | [M]    | Monocle        |
//! | HHH    | Grid           |
//! | TTT    | Bottom-stack   |
//! | [@]    | Spiral         |
//! | [\\]   | Dwindle        |
//! | \|M\|  | CenteredMaster |
//!
//! ## Keybindings (Mod = Super/Win key)
//!
//! | Binding         | Action                        |
//! |-----------------|-------------------------------|
//! | Mod+1…9         | Switch to tag N               |
//! | Mod+0           | View all tags                 |
//! | Mod+j           | Focus next window             |
//! | Mod+k           | Focus previous window         |
//! | Mod+h           | Shrink master area            |
//! | Mod+l           | Grow master area              |
//! | Mod+,           | Decrease master count         |
//! | Mod+.           | Increase master count         |
//! | Mod+Space       | Next layout                   |
//! | Mod+BackSpace   | Previous layout               |
//! | Mod+g           | Toggle gaps                   |
//! | Mod+f           | Toggle fullscreen (focused)   |
//! | Mod+q           | Close focused window          |
//! | Mod+Enter       | Spawn shell                   |

#![no_std]
#![no_main]

use libs::{
    keycodes::*, IPC_NONBLOCK, RwmMsg, RwmPayload, RwmType,
    PayloadEventFocus, PayloadEventKey, PayloadGeometry,
    PayloadSurfaceAssign,
    SYSERR_AGAIN,
};
use userland::{
    sys_claim_compositor,
    sys_fb_fill_rect, sys_fb_flush, sys_getpid,
    sys_ipc_recv, sys_ipc_send,
    sys_poll_input, sys_poll_mouse,
    sys_screen_size, sys_spawn,
    sys_surface_commit, sys_surface_create,
    sys_surface_destroy, sys_write,
};

// ── Theme (Rogue Dark — custom palette) ─────────────────────────────────────

const C_BG:         u32 = 0xFF_0D_0E_15; // near-black desktop
const C_BAR_BG:     u32 = 0xFF_12_13_1C; // bar background
const C_WIN_BG:     u32 = 0xFF_1A_1B_26; // unfocused window placeholder
const C_WIN_ACT:    u32 = 0xFF_1F_20_2E; // focused window placeholder
const C_BORDER_ACT: u32 = 0xFF_7A_A2_F7; // focused border (Tokyo Night blue)
const C_BORDER_IN:  u32 = 0xFF_24_25_35; // unfocused border
const C_TAG_ACT:    u32 = 0xFF_7A_A2_F7; // active tag text / highlight
const C_TAG_OCC:    u32 = 0xFF_4A_4F_74; // occupied but inactive tag
const C_TAG_IN:     u32 = 0xFF_1E_1F_2E; // empty inactive tag
const C_TEXT:       u32 = 0xFF_C0_CA_F5; // default bar text
const C_TEXT_DIM:   u32 = 0xFF_56_5F_89; // dimmed bar text
const C_ACCENT:     u32 = 0xFF_BB_9A_F7; // purple accent (layout symbol)
const C_SEP:        u32 = 0xFF_24_25_35; // separator lines

// ── Layout constants ─────────────────────────────────────────────────────────

const MAX_CLIENTS: usize = 16;
const TAG_CNT:     usize = 9;
const BAR_H:       u32   = 20;
const BORDER:      u32   = 2;
const GAP:         i32   = 4;
const TAG_W:       u32   = 22;
const FONT_W:      u32   = 8;
const FONT_H:      u32   = 8;
const PROG_SHELL:  u32   = 0;

// ── Embedded 8×8 bitmap font (ASCII 0x20–0x7E) ──────────────────────────────
//
// Each character is 8 bytes; one byte per row, MSB is leftmost pixel.
// Glyphs cover printable ASCII (space through tilde).

const FONT_FIRST: u8 = 0x20; // space
const FONT_LAST:  u8 = 0x7E; // tilde
const FONT_COUNT: usize = (FONT_LAST - FONT_FIRST + 1) as usize;

#[rustfmt::skip]
static FONT8X8: [[u8; 8]; FONT_COUNT] = [
    // 0x20 space
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00],
    // 0x21 !
    [0x18,0x3C,0x3C,0x18,0x18,0x00,0x18,0x00],
    // 0x22 "
    [0x36,0x36,0x00,0x00,0x00,0x00,0x00,0x00],
    // 0x23 #
    [0x36,0x36,0x7F,0x36,0x7F,0x36,0x36,0x00],
    // 0x24 $
    [0x0C,0x3E,0x03,0x1E,0x30,0x1F,0x0C,0x00],
    // 0x25 %
    [0x00,0x63,0x33,0x18,0x0C,0x66,0x63,0x00],
    // 0x26 &
    [0x1C,0x36,0x1C,0x6E,0x3B,0x33,0x6E,0x00],
    // 0x27 '
    [0x06,0x06,0x03,0x00,0x00,0x00,0x00,0x00],
    // 0x28 (
    [0x18,0x0C,0x06,0x06,0x06,0x0C,0x18,0x00],
    // 0x29 )
    [0x06,0x0C,0x18,0x18,0x18,0x0C,0x06,0x00],
    // 0x2A *
    [0x00,0x66,0x3C,0xFF,0x3C,0x66,0x00,0x00],
    // 0x2B +
    [0x00,0x0C,0x0C,0x3F,0x0C,0x0C,0x00,0x00],
    // 0x2C ,
    [0x00,0x00,0x00,0x00,0x00,0x0C,0x0C,0x06],
    // 0x2D -
    [0x00,0x00,0x00,0x3F,0x00,0x00,0x00,0x00],
    // 0x2E .
    [0x00,0x00,0x00,0x00,0x00,0x0C,0x0C,0x00],
    // 0x2F /
    [0x60,0x30,0x18,0x0C,0x06,0x03,0x01,0x00],
    // 0x30 0
    [0x3E,0x63,0x73,0x7B,0x6F,0x67,0x3E,0x00],
    // 0x31 1
    [0x0C,0x0E,0x0C,0x0C,0x0C,0x0C,0x3F,0x00],
    // 0x32 2
    [0x1E,0x33,0x30,0x1C,0x06,0x33,0x3F,0x00],
    // 0x33 3
    [0x1E,0x33,0x30,0x1C,0x30,0x33,0x1E,0x00],
    // 0x34 4
    [0x38,0x3C,0x36,0x33,0x7F,0x30,0x78,0x00],
    // 0x35 5
    [0x3F,0x03,0x1F,0x30,0x30,0x33,0x1E,0x00],
    // 0x36 6
    [0x1C,0x06,0x03,0x1F,0x33,0x33,0x1E,0x00],
    // 0x37 7
    [0x3F,0x33,0x30,0x18,0x0C,0x0C,0x0C,0x00],
    // 0x38 8
    [0x1E,0x33,0x33,0x1E,0x33,0x33,0x1E,0x00],
    // 0x39 9
    [0x1E,0x33,0x33,0x3E,0x30,0x18,0x0E,0x00],
    // 0x3A :
    [0x00,0x0C,0x0C,0x00,0x00,0x0C,0x0C,0x00],
    // 0x3B ;
    [0x00,0x0C,0x0C,0x00,0x00,0x0C,0x0C,0x06],
    // 0x3C <
    [0x18,0x0C,0x06,0x03,0x06,0x0C,0x18,0x00],
    // 0x3D =
    [0x00,0x00,0x3F,0x00,0x00,0x3F,0x00,0x00],
    // 0x3E >
    [0x06,0x0C,0x18,0x30,0x18,0x0C,0x06,0x00],
    // 0x3F ?
    [0x1E,0x33,0x30,0x18,0x0C,0x00,0x0C,0x00],
    // 0x40 @
    [0x3E,0x63,0x7B,0x7B,0x7B,0x03,0x1E,0x00],
    // 0x41 A
    [0x0C,0x1E,0x33,0x33,0x3F,0x33,0x33,0x00],
    // 0x42 B
    [0x3F,0x66,0x66,0x3E,0x66,0x66,0x3F,0x00],
    // 0x43 C
    [0x3C,0x66,0x03,0x03,0x03,0x66,0x3C,0x00],
    // 0x44 D
    [0x1F,0x36,0x66,0x66,0x66,0x36,0x1F,0x00],
    // 0x45 E
    [0x7F,0x46,0x16,0x1E,0x16,0x46,0x7F,0x00],
    // 0x46 F
    [0x7F,0x46,0x16,0x1E,0x16,0x06,0x0F,0x00],
    // 0x47 G
    [0x3C,0x66,0x03,0x03,0x73,0x66,0x7C,0x00],
    // 0x48 H
    [0x33,0x33,0x33,0x3F,0x33,0x33,0x33,0x00],
    // 0x49 I
    [0x1E,0x0C,0x0C,0x0C,0x0C,0x0C,0x1E,0x00],
    // 0x4A J
    [0x78,0x30,0x30,0x30,0x33,0x33,0x1E,0x00],
    // 0x4B K
    [0x67,0x66,0x36,0x1E,0x36,0x66,0x67,0x00],
    // 0x4C L
    [0x0F,0x06,0x06,0x06,0x46,0x66,0x7F,0x00],
    // 0x4D M
    [0x63,0x77,0x7F,0x7F,0x6B,0x63,0x63,0x00],
    // 0x4E N
    [0x63,0x67,0x6F,0x7B,0x73,0x63,0x63,0x00],
    // 0x4F O
    [0x1C,0x36,0x63,0x63,0x63,0x36,0x1C,0x00],
    // 0x50 P
    [0x3F,0x66,0x66,0x3E,0x06,0x06,0x0F,0x00],
    // 0x51 Q
    [0x1E,0x33,0x33,0x33,0x3B,0x1E,0x38,0x00],
    // 0x52 R
    [0x3F,0x66,0x66,0x3E,0x36,0x66,0x67,0x00],
    // 0x53 S
    [0x1E,0x33,0x07,0x0E,0x38,0x33,0x1E,0x00],
    // 0x54 T
    [0x3F,0x2D,0x0C,0x0C,0x0C,0x0C,0x1E,0x00],
    // 0x55 U
    [0x33,0x33,0x33,0x33,0x33,0x33,0x3F,0x00],
    // 0x56 V
    [0x33,0x33,0x33,0x33,0x33,0x1E,0x0C,0x00],
    // 0x57 W
    [0x63,0x63,0x63,0x6B,0x7F,0x77,0x63,0x00],
    // 0x58 X
    [0x63,0x63,0x36,0x1C,0x1C,0x36,0x63,0x00],
    // 0x59 Y
    [0x33,0x33,0x33,0x1E,0x0C,0x0C,0x1E,0x00],
    // 0x5A Z
    [0x7F,0x63,0x31,0x18,0x4C,0x66,0x7F,0x00],
    // 0x5B [
    [0x1E,0x06,0x06,0x06,0x06,0x06,0x1E,0x00],
    // 0x5C backslash
    [0x03,0x06,0x0C,0x18,0x30,0x60,0x40,0x00],
    // 0x5D ]
    [0x1E,0x18,0x18,0x18,0x18,0x18,0x1E,0x00],
    // 0x5E ^
    [0x08,0x1C,0x36,0x63,0x00,0x00,0x00,0x00],
    // 0x5F _
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0xFF],
    // 0x60 `
    [0x0C,0x0C,0x18,0x00,0x00,0x00,0x00,0x00],
    // 0x61 a
    [0x00,0x00,0x1E,0x30,0x3E,0x33,0x6E,0x00],
    // 0x62 b
    [0x07,0x06,0x06,0x3E,0x66,0x66,0x3B,0x00],
    // 0x63 c
    [0x00,0x00,0x1E,0x33,0x03,0x33,0x1E,0x00],
    // 0x64 d
    [0x38,0x30,0x30,0x3e,0x33,0x33,0x6E,0x00],
    // 0x65 e
    [0x00,0x00,0x1E,0x33,0x3f,0x03,0x1E,0x00],
    // 0x66 f
    [0x1C,0x36,0x06,0x0f,0x06,0x06,0x0F,0x00],
    // 0x67 g
    [0x00,0x00,0x6E,0x33,0x33,0x3E,0x30,0x1F],
    // 0x68 h
    [0x07,0x06,0x36,0x6E,0x66,0x66,0x67,0x00],
    // 0x69 i
    [0x0C,0x00,0x0E,0x0C,0x0C,0x0C,0x1E,0x00],
    // 0x6A j
    [0x30,0x00,0x30,0x30,0x30,0x33,0x33,0x1E],
    // 0x6B k
    [0x07,0x06,0x66,0x36,0x1E,0x36,0x67,0x00],
    // 0x6C l
    [0x0E,0x0C,0x0C,0x0C,0x0C,0x0C,0x1E,0x00],
    // 0x6D m
    [0x00,0x00,0x33,0x7F,0x7F,0x6B,0x63,0x00],
    // 0x6E n
    [0x00,0x00,0x1F,0x33,0x33,0x33,0x33,0x00],
    // 0x6F o
    [0x00,0x00,0x1E,0x33,0x33,0x33,0x1E,0x00],
    // 0x70 p
    [0x00,0x00,0x3B,0x66,0x66,0x3E,0x06,0x0F],
    // 0x71 q
    [0x00,0x00,0x6E,0x33,0x33,0x3E,0x30,0x78],
    // 0x72 r
    [0x00,0x00,0x3B,0x6E,0x66,0x06,0x0F,0x00],
    // 0x73 s
    [0x00,0x00,0x3E,0x03,0x1E,0x30,0x1F,0x00],
    // 0x74 t
    [0x08,0x0C,0x3E,0x0C,0x0C,0x2C,0x18,0x00],
    // 0x75 u
    [0x00,0x00,0x33,0x33,0x33,0x33,0x6E,0x00],
    // 0x76 v
    [0x00,0x00,0x33,0x33,0x33,0x1E,0x0C,0x00],
    // 0x77 w
    [0x00,0x00,0x63,0x6B,0x7F,0x7F,0x36,0x00],
    // 0x78 x
    [0x00,0x00,0x63,0x36,0x1C,0x36,0x63,0x00],
    // 0x79 y
    [0x00,0x00,0x33,0x33,0x33,0x3E,0x30,0x1F],
    // 0x7A z
    [0x00,0x00,0x3F,0x19,0x0C,0x26,0x3F,0x00],
    // 0x7B {
    [0x38,0x0C,0x0C,0x07,0x0C,0x0C,0x38,0x00],
    // 0x7C |
    [0x18,0x18,0x18,0x00,0x18,0x18,0x18,0x00],
    // 0x7D }
    [0x07,0x0C,0x0C,0x38,0x0C,0x0C,0x07,0x00],
    // 0x7E ~
    [0x6E,0x3B,0x00,0x00,0x00,0x00,0x00,0x00],
];

/// Draw a single 8×8 glyph at (px, py) with the given foreground color.
/// Transparent background (skips background pixels).
fn draw_char(ch: u8, px: u32, py: u32, fg: u32) {
    if ch < FONT_FIRST || ch > FONT_LAST { return; }
    let idx = (ch - FONT_FIRST) as usize;
    let glyph = &FONT8X8[idx];
    for row in 0..8u32 {
        let bits = glyph[row as usize];
        for col in 0..8u32 {
            if (bits >> (7 - col)) & 1 != 0 {
                sys_fb_fill_rect(px + col, py + row, 1, 1, fg);
            }
        }
    }
}

/// Draw a byte string at (x, y) — returns x after last glyph.
fn draw_str(s: &[u8], x: u32, y: u32, fg: u32) -> u32 {
    let mut cx = x;
    for &b in s {
        draw_char(b, cx, y, fg);
        cx += FONT_W;
    }
    cx
}

/// Draw a byte string clipped to `max_w` pixels.
fn draw_str_clipped(s: &[u8], x: u32, y: u32, fg: u32, max_w: u32) {
    let max_chars = (max_w / FONT_W) as usize;
    let n = s.len().min(max_chars);
    draw_str(&s[..n], x, y, fg);
}

// ── Layout engine ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Layout {
    Tile, Monocle, Grid, BStack, Spiral, Dwindle, CenteredMaster,
}

impl Layout {
    fn symbol(self) -> &'static [u8] {
        match self {
            Layout::Tile           => b"[]=",
            Layout::Monocle        => b"[M]",
            Layout::Grid           => b"HHH",
            Layout::BStack         => b"TTT",
            Layout::Spiral         => b"[@]",
            Layout::Dwindle        => b"[\\]",
            Layout::CenteredMaster => b"|M|",
        }
    }
    fn next(self) -> Self {
        match self {
            Layout::Tile           => Layout::Monocle,
            Layout::Monocle        => Layout::Grid,
            Layout::Grid           => Layout::BStack,
            Layout::BStack         => Layout::Spiral,
            Layout::Spiral         => Layout::Dwindle,
            Layout::Dwindle        => Layout::CenteredMaster,
            Layout::CenteredMaster => Layout::Tile,
        }
    }
    fn prev(self) -> Self {
        match self {
            Layout::Tile           => Layout::CenteredMaster,
            Layout::Monocle        => Layout::Tile,
            Layout::Grid           => Layout::Monocle,
            Layout::BStack         => Layout::Grid,
            Layout::Spiral         => Layout::BStack,
            Layout::Dwindle        => Layout::Spiral,
            Layout::CenteredMaster => Layout::Dwindle,
        }
    }
}

// ── Client record ────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Client {
    alive:      bool,
    pid:        u32,
    surface_id: u32,
    tags:       u32,
    x: i32, y: i32, w: u32, h: u32,
    floating:   bool,
    fullscreen: bool,
    title:      [u8; 48],
    title_len:  u8,
}

impl Client {
    const fn empty() -> Self {
        Client {
            alive: false, pid: 0, surface_id: 0,
            tags: 1, x: 0, y: 0, w: 0, h: 0,
            floating: false, fullscreen: false,
            title: [0u8; 48], title_len: 0,
        }
    }
}

// ── Nova compositor state ─────────────────────────────────────────────────────

struct Nova {
    clients:     [Client; MAX_CLIENTS],
    n:           usize,
    focused:     usize,
    tagset:      [u32; 2],
    sel_tag:     usize,
    layout:      Layout,
    nmaster:     usize,
    mfact:       u32,   // master width % (5..95)
    gaps_on:     bool,
    screen_w:    u32,
    screen_h:    u32,
    my_pid:      u32,
    mod_pressed: bool,
    seq:         u16,
    /// Frame counter — used to drive a simple busy indicator.
    frame:       u32,
}

impl Nova {
    fn new(w: u32, h: u32, pid: u32) -> Self {
        Nova {
            clients: [Client::empty(); MAX_CLIENTS],
            n: 0, focused: 0,
            tagset: [1, 1], sel_tag: 0,
            layout: Layout::Tile,
            nmaster: 1, mfact: 55,
            gaps_on: true,
            screen_w: w, screen_h: h,
            my_pid: pid,
            mod_pressed: false, seq: 0, frame: 0,
        }
    }

    fn cur_tags(&self) -> u32 { self.tagset[self.sel_tag] }
    fn work_y(&self) -> u32 { BAR_H }
    fn work_h(&self) -> u32 { self.screen_h.saturating_sub(BAR_H) }

    // ── Client management ────────────────────────────────────────────────────

    fn add_client(&mut self, pid: u32, surface_id: u32, title: &[u8]) -> Option<usize> {
        let tags = self.cur_tags();
        for i in 0..MAX_CLIENTS {
            if !self.clients[i].alive {
                let c = &mut self.clients[i];
                c.alive = true;
                c.pid = pid;
                c.surface_id = surface_id;
                c.tags = tags;
                c.floating = false;
                c.fullscreen = false;
                let n = title.len().min(48);
                c.title[..n].copy_from_slice(&title[..n]);
                c.title_len = n as u8;
                self.n += 1;
                self.focused = i;
                return Some(i);
            }
        }
        None
    }

    fn remove_client(&mut self, pid: u32) {
        for i in 0..MAX_CLIENTS {
            if self.clients[i].alive && self.clients[i].pid == pid {
                let sid = self.clients[i].surface_id;
                if sid != 0 { sys_surface_destroy(sid); }
                self.clients[i] = Client::empty();
                self.n = self.n.saturating_sub(1);
                if self.focused == i {
                    self.focused = self.next_visible(i);
                }
                return;
            }
        }
    }

    fn client_by_pid(&mut self, pid: u32) -> Option<&mut Client> {
        self.clients.iter_mut().find(|c| c.alive && c.pid == pid)
    }

    fn next_visible(&self, from: usize) -> usize {
        let tags = self.cur_tags();
        for d in 1..=MAX_CLIENTS {
            let i = (from + d) % MAX_CLIENTS;
            if self.clients[i].alive && (self.clients[i].tags & tags) != 0 { return i; }
        }
        from
    }

    fn prev_visible(&self, from: usize) -> usize {
        let tags = self.cur_tags();
        for d in 1..=MAX_CLIENTS {
            let i = (from + MAX_CLIENTS - d) % MAX_CLIENTS;
            if self.clients[i].alive && (self.clients[i].tags & tags) != 0 { return i; }
        }
        from
    }

    fn visible_count(&self) -> usize {
        let tags = self.cur_tags();
        self.clients.iter()
            .filter(|c| c.alive && (c.tags & tags) != 0 && !c.floating && !c.fullscreen)
            .count()
    }

    // ── Layout computation ───────────────────────────────────────────────────

    fn arrange(&mut self) {
        let sw  = self.screen_w as i32;
        let wh  = self.work_h() as i32;
        let oy  = self.work_y() as i32;
        let g   = if self.gaps_on { GAP } else { 0 };
        let b   = BORDER as i32;

        // Collect visible non-floating non-fullscreen clients.
        let mut vis = [0usize; MAX_CLIENTS];
        let mut vn  = 0usize;
        let tags    = self.cur_tags();
        for i in 0..MAX_CLIENTS {
            let c = &self.clients[i];
            if c.alive && (c.tags & tags) != 0 && !c.floating && !c.fullscreen {
                vis[vn] = i;
                vn += 1;
            }
        }
        if vn == 0 { return; }

        match self.layout {
            Layout::Tile           => self.arrange_tile(&vis, vn, sw, wh, oy, g, b),
            Layout::Monocle        => {
                for k in 0..vn {
                    let c = &mut self.clients[vis[k]];
                    c.x = g; c.y = oy + g;
                    c.w = (sw - 2*g - 2*b).max(1) as u32;
                    c.h = (wh - 2*g - 2*b).max(1) as u32;
                }
            }
            Layout::Grid           => self.arrange_grid(&vis, vn, sw, wh, oy, g, b),
            Layout::BStack         => self.arrange_bstack(&vis, vn, sw, wh, oy, g, b),
            Layout::Spiral         => self.arrange_spiral(&vis, vn, sw, wh, oy, g, b, false),
            Layout::Dwindle        => self.arrange_spiral(&vis, vn, sw, wh, oy, g, b, true),
            Layout::CenteredMaster => self.arrange_centered(&vis, vn, sw, wh, oy, g, b),
        }
    }

    fn arrange_tile(&mut self, vis: &[usize], n: usize, sw: i32, wh: i32, oy: i32, g: i32, b: i32) {
        let nm = self.nmaster.min(n);
        let mfact = self.mfact as i32;
        let master_w = if nm == n { sw - 2*g } else { (sw - 2*g) * mfact / 100 };
        let stack_w  = sw - 2*g - master_w - g;
        for k in 0..n {
            let c = &mut self.clients[vis[k]];
            if k < nm {
                let slot_h = (wh - 2*g - (nm as i32 - 1)*g) / nm as i32;
                c.x = g; c.y = oy + g + k as i32*(slot_h + g);
                c.w = (master_w - 2*b).max(1) as u32;
                c.h = (slot_h   - 2*b).max(1) as u32;
            } else {
                let si = k - nm;
                let sc = n - nm;
                let slot_h = (wh - 2*g - (sc as i32 - 1)*g) / sc as i32;
                c.x = g + master_w + g; c.y = oy + g + si as i32*(slot_h + g);
                c.w = (stack_w - 2*b).max(1) as u32;
                c.h = (slot_h  - 2*b).max(1) as u32;
            }
        }
    }

    fn arrange_grid(&mut self, vis: &[usize], n: usize, sw: i32, wh: i32, oy: i32, g: i32, b: i32) {
        let cols = { let mut c = 1; while c * c < n { c += 1; } c } as i32;
        let rows = (n as i32 + cols - 1) / cols;
        for k in 0..n {
            let col = k as i32 % cols;
            let row = k as i32 / cols;
            let cw = (sw - 2*g - (cols-1)*g) / cols;
            let rh = (wh - 2*g - (rows-1)*g) / rows;
            let c = &mut self.clients[vis[k]];
            c.x = g + col*(cw+g); c.y = oy + g + row*(rh+g);
            c.w = (cw - 2*b).max(1) as u32;
            c.h = (rh - 2*b).max(1) as u32;
        }
    }

    fn arrange_bstack(&mut self, vis: &[usize], n: usize, sw: i32, wh: i32, oy: i32, g: i32, b: i32) {
        let nm = self.nmaster.min(n);
        let mfact = self.mfact as i32;
        let master_h = if nm == n { wh - 2*g } else { (wh - 2*g) * mfact / 100 };
        let stack_h  = wh - 2*g - master_h - g;
        let sc = n - nm;
        for k in 0..n {
            let c = &mut self.clients[vis[k]];
            if k < nm {
                let slot_w = (sw - 2*g - (nm as i32 - 1)*g) / nm as i32;
                c.x = g + k as i32*(slot_w+g); c.y = oy + g;
                c.w = (slot_w  - 2*b).max(1) as u32;
                c.h = (master_h - 2*b).max(1) as u32;
            } else {
                let si = k - nm;
                let slot_w = if sc > 0 { (sw - 2*g - (sc as i32 - 1)*g) / sc as i32 } else { sw };
                c.x = g + si as i32*(slot_w+g); c.y = oy + g + master_h + g;
                c.w = (slot_w  - 2*b).max(1) as u32;
                c.h = (stack_h - 2*b).max(1) as u32;
            }
        }
    }

    fn arrange_spiral(&mut self, vis: &[usize], n: usize, sw: i32, wh: i32, oy: i32, g: i32, b: i32, dwindle: bool) {
        let mut rx = g; let mut ry = oy + g;
        let mut rw = sw - 2*g; let mut rh = wh - 2*g;
        for k in 0..n {
            let c = &mut self.clients[vis[k]];
            if k + 1 == n {
                c.x = rx; c.y = ry;
                c.w = (rw - 2*b).max(1) as u32;
                c.h = (rh - 2*b).max(1) as u32;
                break;
            }
            let horiz = if dwindle { k % 2 == 0 } else { k % 2 == 0 };
            if horiz {
                let half = rw / 2;
                c.x = rx; c.y = ry;
                c.w = (half - g/2 - 2*b).max(1) as u32;
                c.h = (rh - 2*b).max(1) as u32;
                rx += half + g/2; rw -= half + g/2;
            } else {
                let half = rh / 2;
                c.x = rx; c.y = ry;
                c.w = (rw - 2*b).max(1) as u32;
                c.h = (half - g/2 - 2*b).max(1) as u32;
                ry += half + g/2; rh -= half + g/2;
            }
        }
    }

    fn arrange_centered(&mut self, vis: &[usize], n: usize, sw: i32, wh: i32, oy: i32, g: i32, b: i32) {
        let nm = self.nmaster.min(n);
        let mfact = self.mfact as i32;
        let master_w = (sw - 2*g) * mfact / 100;
        let side_w   = (sw - 2*g - master_w - 2*g) / 2;
        let master_x = g + side_w + g;
        let sc_l = (n - nm) / 2;
        let sc_r = n - nm - sc_l;
        let mut li = 0; let mut ri = 0;
        for k in 0..n {
            let c = &mut self.clients[vis[k]];
            if k < nm {
                let slot_h = (wh - 2*g - (nm as i32 - 1)*g) / nm as i32;
                c.x = master_x; c.y = oy + g + k as i32*(slot_h+g);
                c.w = (master_w - 2*b).max(1) as u32;
                c.h = (slot_h   - 2*b).max(1) as u32;
            } else if li < sc_l {
                let sc = sc_l.max(1);
                let slot_h = (wh - 2*g - (sc as i32 - 1)*g) / sc as i32;
                c.x = g; c.y = oy + g + li as i32*(slot_h+g);
                c.w = (side_w - 2*b).max(1) as u32;
                c.h = (slot_h - 2*b).max(1) as u32;
                li += 1;
            } else {
                let sc = sc_r.max(1);
                let slot_h = (wh - 2*g - (sc as i32 - 1)*g) / sc as i32;
                c.x = master_x + master_w + g; c.y = oy + g + ri as i32*(slot_h+g);
                c.w = (side_w - 2*b).max(1) as u32;
                c.h = (slot_h - 2*b).max(1) as u32;
                ri += 1;
            }
        }
    }

    // ── Rendering ────────────────────────────────────────────────────────────

    fn send_geometry(&mut self, idx: usize) {
        let c = &self.clients[idx];
        if !c.alive { return; }
        let msg = make_msg(
            RwmType::Geometry as u8, self.my_pid, self.seq,
            RwmPayload { geometry: PayloadGeometry { x: c.x, y: c.y, w: c.w, h: c.h, _pad: [0; 40] } },
        );
        self.seq = self.seq.wrapping_add(1);
        sys_ipc_send(c.pid, &msg, 0);
    }

    fn send_surface_assign(&mut self, idx: usize) {
        let c = &self.clients[idx];
        if !c.alive || c.surface_id == 0 { return; }
        let msg = make_msg(
            RwmType::SurfaceAssign as u8, self.my_pid, self.seq,
            RwmPayload { surface_assign: PayloadSurfaceAssign { surface_id: c.surface_id, _pad: [0; 52] } },
        );
        self.seq = self.seq.wrapping_add(1);
        sys_ipc_send(c.pid, &msg, 0);
    }

    /// Render the status bar at the top of the screen.
    fn draw_bar(&self) {
        let sw = self.screen_w;
        let bh = BAR_H;
        let ty = (bh.saturating_sub(FONT_H)) / 2; // vertical center of font in bar

        // Bar background.
        sys_fb_fill_rect(0, 0, sw, bh, C_BAR_BG);

        // ── Tag strip ────────────────────────────────────────────────────────
        let cur = self.cur_tags();
        for t in 0..TAG_CNT {
            let bit = 1u32 << t;
            let occupied = self.clients.iter().any(|c| c.alive && (c.tags & bit) != 0);
            let active   = (cur & bit) != 0;
            let bg_col   = if active { C_TAG_ACT } else if occupied { C_TAG_OCC } else { C_TAG_IN };
            let fg_col   = if active { C_BAR_BG  } else if occupied { C_TEXT }     else { C_TEXT_DIM };
            let tx = t as u32 * TAG_W;
            sys_fb_fill_rect(tx, 0, TAG_W, bh, bg_col);
            // Tag number: '1'..'9'
            let ch = b'1' + t as u8;
            let char_x = tx + (TAG_W.saturating_sub(FONT_W)) / 2;
            draw_char(ch, char_x, ty, fg_col);
        }

        // Separator after tags.
        let sep_x = TAG_CNT as u32 * TAG_W;
        sys_fb_fill_rect(sep_x, 0, 1, bh, C_SEP);

        // ── Layout symbol ────────────────────────────────────────────────────
        let sym = self.layout.symbol();
        let sym_x = sep_x + 4;
        draw_str(sym, sym_x, ty, C_ACCENT);
        let after_sym = sym_x + sym.len() as u32 * FONT_W + 4;

        // Separator.
        sys_fb_fill_rect(after_sym, 0, 1, bh, C_SEP);

        // ── Window title ─────────────────────────────────────────────────────
        let title_x = after_sym + 4;
        let title_max_w = sw.saturating_sub(title_x + 4);
        if let Some(c) = self.clients.get(self.focused) {
            if c.alive {
                let tlen = c.title_len as usize;
                draw_str_clipped(&c.title[..tlen], title_x, ty, C_TEXT, title_max_w);
            }
        }

        // ── Window count badge (right side) ──────────────────────────────────
        let n_vis = self.visible_count();
        if n_vis > 0 {
            // Draw "[N]" right-aligned with small margin.
            let badge_w = 3 * FONT_W; // "[N]" = 3 chars max for single digit
            let bx = sw.saturating_sub(badge_w + 4);
            let buf = [b'[', b'0' + (n_vis.min(9) as u8), b']'];
            draw_str(&buf, bx, ty, C_TEXT_DIM);
        }
    }

    /// Draw window borders for all visible clients.
    fn draw_decorations(&self) {
        let tags = self.cur_tags();
        for (i, c) in self.clients.iter().enumerate() {
            if !c.alive || (c.tags & tags) == 0 || c.fullscreen { continue; }
            let col = if i == self.focused { C_BORDER_ACT } else { C_BORDER_IN };
            let b   = BORDER;
            let cx  = c.x as u32;
            let cy  = c.y as u32;
            // Top / bottom / left / right
            sys_fb_fill_rect(cx,           cy,           c.w + 2*b, b,           col);
            sys_fb_fill_rect(cx,           cy + c.h + b, c.w + 2*b, b,           col);
            sys_fb_fill_rect(cx,           cy,           b,          c.h + 2*b,  col);
            sys_fb_fill_rect(cx + c.w + b, cy,           b,          c.h + 2*b,  col);
        }
    }

    /// Composite one full frame to the framebuffer.
    fn composite_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);

        // 1. Desktop background.
        sys_fb_fill_rect(0, self.work_y(), self.screen_w, self.work_h(), C_BG);

        // 2. Client surfaces in z-order.
        let tags = self.cur_tags();
        for i in 0..MAX_CLIENTS {
            let c = &self.clients[i];
            if !c.alive || (c.tags & tags) == 0 { continue; }
            if c.surface_id != 0 {
                sys_surface_commit(c.surface_id, c.x as u32, c.y as u32);
            } else {
                let col = if i == self.focused { C_WIN_ACT } else { C_WIN_BG };
                sys_fb_fill_rect(c.x as u32, c.y as u32, c.w, c.h, col);
            }
        }

        // 3. Window borders on top.
        self.draw_decorations();

        // 4. Status bar (always on top).
        self.draw_bar();

        sys_fb_flush();
    }

    // ── Focus management ─────────────────────────────────────────────────────

    fn focus(&mut self, idx: usize) {
        if self.focused != idx {
            let old = self.focused;
            if self.clients[old].alive {
                let msg = make_msg(
                    RwmType::EventFocus as u8, self.my_pid, self.seq,
                    RwmPayload { event_focus: PayloadEventFocus { focused: 0, _pad: [0; 55] } },
                );
                self.seq = self.seq.wrapping_add(1);
                sys_ipc_send(self.clients[old].pid, &msg, 0);
            }
        }
        self.focused = idx;
        if self.clients[idx].alive {
            let msg = make_msg(
                RwmType::EventFocus as u8, self.my_pid, self.seq,
                RwmPayload { event_focus: PayloadEventFocus { focused: 1, _pad: [0; 55] } },
            );
            self.seq = self.seq.wrapping_add(1);
            sys_ipc_send(self.clients[idx].pid, &msg, 0);
        }
    }

    fn forward_key(&mut self, keycode: u8, pressed: bool) {
        let f = self.focused;
        if f >= MAX_CLIENTS || !self.clients[f].alive { return; }
        let pid = self.clients[f].pid;
        let msg = make_msg(
            RwmType::EventKey as u8, self.my_pid, self.seq,
            RwmPayload { event_key: PayloadEventKey { keycode, pressed: pressed as u8, _pad: [0; 54] } },
        );
        self.seq = self.seq.wrapping_add(1);
        sys_ipc_send(pid, &msg, 0);
    }

    // ── WM keybinding handler ────────────────────────────────────────────────

    /// Process a Mod+key shortcut.  Returns true if the shortcut was consumed.
    fn handle_shortcut(&mut self, keycode: u8, pressed: bool) -> bool {
        if !pressed { return false; }
        match keycode {
            // Tag switching: Mod+1..9 → view tag N
            KEY_1..=KEY_9 => {
                let t = (keycode - KEY_1) as usize;
                self.tagset[self.sel_tag] = 1 << t;
                self.arrange();
                return true;
            }
            // Mod+0 → view all tags
            KEY_0 => {
                self.tagset[self.sel_tag] = (1 << TAG_CNT) - 1;
                self.arrange();
                return true;
            }
            // Focus movement
            KEY_J => {
                let nxt = self.next_visible(self.focused);
                self.focus(nxt);
                return true;
            }
            KEY_K => {
                let prv = self.prev_visible(self.focused);
                self.focus(prv);
                return true;
            }
            // Layout cycling
            KEY_SPACE => {
                self.layout = self.layout.next();
                self.arrange();
                return true;
            }
            KEY_BACKSPACE => {
                self.layout = self.layout.prev();
                self.arrange();
                return true;
            }
            // Toggle gaps
            KEY_G => {
                self.gaps_on = !self.gaps_on;
                self.arrange();
                return true;
            }
            // Master area resize
            KEY_H => {
                self.mfact = self.mfact.saturating_sub(5).max(5);
                self.arrange();
                return true;
            }
            KEY_L => {
                self.mfact = (self.mfact + 5).min(95);
                self.arrange();
                return true;
            }
            // Master count
            KEY_COMMA => {
                if self.nmaster > 0 { self.nmaster -= 1; self.arrange(); }
                return true;
            }
            KEY_PERIOD => {
                self.nmaster += 1;
                self.arrange();
                return true;
            }
            // Toggle fullscreen for focused client
            KEY_F => {
                let f = self.focused;
                if f < MAX_CLIENTS && self.clients[f].alive {
                    self.clients[f].fullscreen = !self.clients[f].fullscreen;
                    if self.clients[f].fullscreen {
                        self.clients[f].x = 0;
                        self.clients[f].y = 0;
                        self.clients[f].w = self.screen_w;
                        self.clients[f].h = self.screen_h;
                    } else {
                        self.arrange();
                    }
                }
                return true;
            }
            // Spawn shell: Mod+Enter
            KEY_ENTER => {
                sys_spawn(PROG_SHELL);
                return true;
            }
            _ => {}
        }
        false
    }
}

// ── IPC message handling ─────────────────────────────────────────────────────

fn handle_ipc(nova: &mut Nova) -> bool {
    let mut msg = RwmMsg::ZERO;
    let r = sys_ipc_recv(&mut msg, IPC_NONBLOCK);
    if r == SYSERR_AGAIN as isize || r < 0 { return false; }

    let t = msg.msg_type;

    // ── Registration ─────────────────────────────────────────────────────────
    if t == RwmType::Register as u8 {
        let pid    = msg.sender_pid;
        let title  = unsafe { &msg.payload.register.title };
        let tlen   = title.iter().position(|&b| b == 0).unwrap_or(title.len());

        let sid_raw = sys_surface_create();
        let sid = if sid_raw > 0 { sid_raw as u32 } else { 0 };

        if let Some(idx) = nova.add_client(pid, sid, &title[..tlen]) {
            nova.arrange();
            nova.send_geometry(idx);
            if sid != 0 { nova.send_surface_assign(idx); }
            nova.composite_frame();
        }
        return true;
    }

    if t == RwmType::Unregister as u8 {
        nova.remove_client(msg.sender_pid);
        nova.arrange();
        nova.composite_frame();
        return true;
    }

    // ── Surface commit (client rendered a new frame) ──────────────────────────
    if t == RwmType::SurfaceCommit as u8 {
        let sc = unsafe { msg.payload.surface_commit };
        if let Some(c) = nova.client_by_pid(msg.sender_pid) {
            if sc.x != 0 || sc.y != 0 { c.x = sc.x; c.y = sc.y; }
        }
        nova.composite_frame();
        return true;
    }

    // ── Title update ──────────────────────────────────────────────────────────
    if t == RwmType::SetTitle as u8 {
        let title = unsafe { &msg.payload.set_title.title };
        let n = title.iter().position(|&b| b == 0).unwrap_or(title.len()).min(48);
        if let Some(c) = nova.client_by_pid(msg.sender_pid) {
            c.title[..n].copy_from_slice(&title[..n]);
            c.title_len = n as u8;
        }
        return true;
    }

    false
}

// ── Helper ────────────────────────────────────────────────────────────────────

fn make_msg(msg_type: u8, sender_pid: u32, seq: u16, payload: RwmPayload) -> RwmMsg {
    RwmMsg { msg_type, flags: 0, seq, sender_pid, payload }
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[no_mangle]
fn _start() -> ! {
    log(b"[nova] starting\r\n");

    // ── Claim the compositor role ─────────────────────────────────────────────
    let claim = sys_claim_compositor();
    if claim < 0 {
        log(b"[nova] WARN: claim_compositor failed - another compositor may be running\r\n");
    } else {
        log(b"[nova] compositor claimed\r\n");
    }

    let mut sw: u32 = 1920;
    let mut sh: u32 = 1080;
    sys_screen_size(&mut sw, &mut sh);
    let my_pid = sys_getpid();

    log(b"[nova] screen_size ok\r\n");

    let mut nova = Nova::new(sw, sh, my_pid);

    // Draw the initial blank desktop + bar.
    nova.composite_frame();

    log(b"[nova] ready - event loop\r\n");

    let mut ev  = libs::KeyEvent   { keycode: 0, pressed: false };
    let mut mev = libs::MouseEvent { dx: 0, dy: 0, buttons: 0 };

    loop {
        // Process all pending IPC messages.
        while handle_ipc(&mut nova) {}

        // Keyboard input.
        let n = sys_poll_input(&mut ev);
        if n > 0 {
            let kc      = ev.keycode;
            let pressed = ev.pressed;
            if kc == KEY_MOD {
                nova.mod_pressed = pressed;
            } else if nova.mod_pressed {
                let consumed = nova.handle_shortcut(kc, pressed);
                if !consumed {
                    nova.forward_key(kc, pressed);
                }
                nova.composite_frame();
            } else {
                nova.forward_key(kc, pressed);
            }
        }

        // Mouse (reserved for future floating-window move/resize).
        sys_poll_mouse(&mut mev);
    }
}

fn log(msg: &[u8]) {
    let _ = sys_write(1, msg.as_ptr(), msg.len());
}
