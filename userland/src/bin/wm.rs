//! RogueOS WM — a dwm-inspired tiling window manager for RogueOS.
//!
//! Features
//! ────────
//! • 9-tag workspace system (bitmask, like dwm)
//! • 5 layout modes: Tile (master/stack), Monocle, Grid, BStack, Spiral
//! • Status bar: tag strip, layout symbol, window title, status text
//! • Compositor: background + window frames with rounded corners rendered
//!   via `sys_fb_fill_rect` (no GPU needed)
//! • Full keyboard handling: Super as Mod, extended PS/2 keycodes
//!
//! Keyboard shortcuts (Mod = Super/Win key)
//! ─────────────────────────────────────────
//! Mod+1..9          Switch to tag N
//! Mod+Shift+1..9    Move focused window to tag N
//! Mod+0             View all tags
//! Mod+J             Focus next window
//! Mod+K             Focus previous window
//! Mod+H             Decrease master width (−5%)
//! Mod+L             Increase master width (+5%)
//! Mod+Comma         Decrease nmaster
//! Mod+Period        Increase nmaster
//! Mod+Space         Cycle layouts forward
//! Mod+Shift+Space   Cycle layouts backward
//! Mod+Enter         Move focused to master position (zoom)
//! Mod+D             Spawn shell (terminal program)
//! Mod+Shift+C       Close focused window
//! Mod+F             Toggle fullscreen for focused window
//! Mod+T             Toggle floating for focused window
//! Mod+G             Toggle gaps on/off
//! Mod+Shift+Q       Reboot system

#![no_std]
#![no_main]

use libs::{keycodes::*, IPC_NONBLOCK, RwmMsg};
use libs::KeyEvent;
use userland::{
    sys_claim_compositor, sys_exit, sys_fb_clear, sys_fb_fill_rect, sys_fb_flush,
    sys_ipc_recv, sys_ipc_send,
    sys_poll_input, sys_spawn, sys_write,
    sys_surface_commit, sys_surface_destroy, sys_surface_set_z,
};

// RDP message type bytes (mirror libs::RwmType repr).
const RDP_CONNECT:      u8 = 0x50;
const RDP_GRANT:        u8 = 0x51;
const RDP_COMMIT:       u8 = 0x52;
const RDP_RESIZE:       u8 = 0x53;
const RDP_KEY:          u8 = 0x54;
const RDP_FOCUS:        u8 = 0x55;
const RDP_CLOSE:        u8 = 0x56;
const RDP_DISCONNECT:   u8 = 0x57;
const RDP_PRESENT_DONE: u8 = 0x58;

// ── Theme: Tokyo Night ────────────────────────────────────────────────────────

const C_BG:         u32 = 0xFF_1A_1B_26; // desktop background
const C_BAR_BG:     u32 = 0xFF_16_17_1F; // status bar background
const C_WIN_BG:     u32 = 0xFF_1F_20_2E; // inactive window fill
const C_WIN_ACT:    u32 = 0xFF_24_28_3D; // active window fill
const C_BORDER_ACT: u32 = 0xFF_7A_A2_F7; // active border (blue)
const C_BORDER_IN:  u32 = 0xFF_29_2E_42; // inactive border
const C_TAG_ACT:    u32 = 0xFF_7A_A2_F7; // active tag indicator
const C_TAG_OCC:    u32 = 0xFF_56_5F_89; // occupied (windows present) tag
const C_TAG_IN:     u32 = 0xFF_2A_2B_3D; // empty inactive tag
const C_LAYOUT_SYM: u32 = 0xFF_BB_9A_F7; // layout symbol colour (purple)
const C_TITLE:      u32 = 0xFF_C0_CA_F5; // focused title accent
const C_STATUS:     u32 = 0xFF_73_DA_FA; // status text accent (cyan)
const C_FLOAT_MARK: u32 = 0xFF_FF_9E_64; // floating window indicator (orange)

// ── Constants ────────────────────────────────────────────────────────────────

const SCREEN_W: u32 = 1920;
const SCREEN_H: u32 = 1080;
const BAR_H:    u32 = 22;
const BORDER:   u32 = 2;
/// Per-window title bar height (inside the window border, above the content area).
const TITLE_H:  u32 = 14;
const GAP:      i32 = 6;
const MAX_WIN:  usize = 16;
const TAG_CNT:  usize = 9;

// Program IDs (must match kernel/audits/main.rs register() calls).
const PROG_SHELL:  u32 = 0;
const PROG_EDITOR: u32 = 2;
const PROG_VIEWER: u32 = 3;

// ── Layout modes ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Layout {
    /// Classic master/stack tiling.  `[]= `
    Tile,
    /// All windows occupy the full area; only the focused one is visible. `[M]`
    Monocle,
    /// Uniform grid.  `HHH`
    Grid,
    /// Master on top, stack across the bottom.  `TTT`
    BStack,
    /// Fibonacci spiral.  `[@]`
    Spiral,
    /// Fibonacci dwindle (each subsequent window takes half the remaining area). `[\\]`
    Dwindle,
    /// Master centred, stack split left/right.  `|M|`
    CenteredMaster,
}

impl Layout {
    fn symbol(self) -> &'static [u8] {
        match self {
            Layout::Tile          => b"[]=",
            Layout::Monocle       => b"[M]",
            Layout::Grid          => b"HHH",
            Layout::BStack        => b"TTT",
            Layout::Spiral        => b"[@]",
            Layout::Dwindle       => b"[\\]",
            Layout::CenteredMaster => b"|M|",
        }
    }

    fn next(self) -> Self {
        match self {
            Layout::Tile          => Layout::Monocle,
            Layout::Monocle       => Layout::Grid,
            Layout::Grid          => Layout::BStack,
            Layout::BStack        => Layout::Spiral,
            Layout::Spiral        => Layout::Dwindle,
            Layout::Dwindle       => Layout::CenteredMaster,
            Layout::CenteredMaster => Layout::Tile,
        }
    }

    fn prev(self) -> Self {
        match self {
            Layout::Tile          => Layout::CenteredMaster,
            Layout::Monocle       => Layout::Tile,
            Layout::Grid          => Layout::Monocle,
            Layout::BStack        => Layout::Grid,
            Layout::Spiral        => Layout::BStack,
            Layout::Dwindle       => Layout::Spiral,
            Layout::CenteredMaster => Layout::Dwindle,
        }
    }
}

// ── Client (window) ───────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Client {
    /// Is this slot occupied?
    alive: bool,
    /// Tag membership bitmask (bit 0 = tag 1, …, bit 8 = tag 9).
    tags: u32,
    /// Geometry (computed by layout engine each redraw).
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    /// Floating: drawn at saved position, not managed by layout.
    floating: bool,
    /// Fullscreen: occupies entire screen, overlaid on bar.
    fullscreen: bool,
    /// Saved geometry (before fullscreen).
    saved_x: i32,
    saved_y: i32,
    saved_w: u32,
    saved_h: u32,
    /// Program ID of the spawned process (for title labelling).
    prog_id: u8,
    /// Spawned process ID (from sys_spawn), 0 if no process.
    pid: u32,
    /// RDP surface ID (created by client via SYS_SURFACE_CREATE). 0 = no surface.
    surface_id: u32,
    /// Short display title.
    title: [u8; 20],
    title_len: u8,
    /// Last content width/height sent to the RDP client (0 = never notified).
    /// Used to detect geometry changes and send RDP_RESIZE.
    notified_cw: u32,
    notified_ch: u32,
}

impl Client {
    const fn empty() -> Self {
        Self {
            alive: false,
            tags: 0,
            x: 0, y: 0, w: 0, h: 0,
            floating: false,
            fullscreen: false,
            saved_x: 0, saved_y: 0, saved_w: 0, saved_h: 0,
            prog_id: 0,
            pid: 0,
            surface_id: 0,
            title: [0; 20],
            title_len: 0,
            notified_cw: 0,
            notified_ch: 0,
        }
    }

    fn set_title(&mut self, s: &[u8]) {
        let n = s.len().min(self.title.len());
        self.title[..n].copy_from_slice(&s[..n]);
        self.title_len = n as u8;
    }
}

// ── WM state ─────────────────────────────────────────────────────────────────

struct Wm {
    clients:   [Client; MAX_WIN],
    n:         usize,
    focused:   usize,         // index into clients[]
    /// Two tagsets for toggling (current & previous), exactly like dwm.
    tagset:    [u32; 2],
    /// Index into tagset[] (0 or 1); current = tagset[sel_tags_idx].
    sel_tags_idx: usize,
    layout:    Layout,
    prev_layout: Layout,
    nmaster:   usize,         // number of master-column windows
    mfact:     u32,           // master column width percentage (5–95)
    gaps_on:   bool,
    show_bar:  bool,
    sw:        u32,           // screen width
    sh:        u32,           // screen height
    // Modifier tracking (updated on every key event).
    mod_dn:    bool,
    shift_dn:  bool,
    ctrl_dn:   bool,
    /// Needs a redraw on next composite pass.
    dirty:     bool,
}

impl Wm {
    #[inline]
    fn sel_tags(&self) -> u32 { self.tagset[self.sel_tags_idx] }

    fn new() -> Self {
        Self {
            clients:      [Client::empty(); MAX_WIN],
            n:            0,
            focused:      0,
            tagset:       [0x01, 0x01],
            sel_tags_idx: 0,
            layout:       Layout::Tile,
            prev_layout:  Layout::Tile,
            nmaster:      1,
            mfact:        55,
            gaps_on:      true,
            show_bar:     true,
            sw:           SCREEN_W,
            sh:           SCREEN_H,
            mod_dn:       false,
            shift_dn:     false,
            ctrl_dn:      false,
            dirty:        true,
        }
    }

    // ── Client management ────────────────────────────────────────────

    fn add_client(&mut self, prog_id: u8, title: &[u8]) -> usize {
        let cur_tags = self.sel_tags();
        for (i, c) in self.clients.iter_mut().enumerate() {
            if !c.alive {
                *c = Client::empty();
                c.alive   = true;
                c.tags    = cur_tags; // new window appears on current tags
                c.prog_id = prog_id;
                c.set_title(title);
                self.n += 1;
                return i;
            }
        }
        // No free slot — overwrite the oldest non-focused window.
        let fallback = (self.focused + 1) % MAX_WIN;
        let c = &mut self.clients[fallback];
        *c = Client::empty();
        c.alive   = true;
        c.tags    = cur_tags;
        c.prog_id = prog_id;
        c.set_title(title);
        fallback
    }

    fn remove_client(&mut self, idx: usize) {
        if idx >= MAX_WIN || !self.clients[idx].alive {
            return;
        }
        self.clients[idx] = Client::empty();
        if self.n > 0 { self.n -= 1; }
        // Refocus to the nearest alive client.
        if self.focused == idx {
            self.focused = self.next_visible(idx);
        }
    }

    // ── Visibility helpers ───────────────────────────────────────────

    fn is_visible(&self, idx: usize) -> bool {
        let c = &self.clients[idx];
        c.alive && (c.tags & self.sel_tags()) != 0
    }

    fn visible_count(&self) -> usize {
        (0..MAX_WIN).filter(|&i| self.is_visible(i)).count()
    }

    /// Collect indices of visible, non-floating, non-fullscreen clients (in order).
    fn visible_tiled(&self, out: &mut [usize; MAX_WIN]) -> usize {
        let mut cnt = 0;
        for i in 0..MAX_WIN {
            let c = &self.clients[i];
            if c.alive && (c.tags & self.sel_tags()) != 0 && !c.floating && !c.fullscreen {
                out[cnt] = i;
                cnt += 1;
            }
        }
        cnt
    }

    fn next_visible(&self, from: usize) -> usize {
        let start = (from + 1) % MAX_WIN;
        for off in 0..MAX_WIN {
            let i = (start + off) % MAX_WIN;
            if self.is_visible(i) { return i; }
        }
        from
    }

    fn prev_visible(&self, from: usize) -> usize {
        for off in 1..=MAX_WIN {
            let i = (from + MAX_WIN - off) % MAX_WIN;
            if self.is_visible(i) { return i; }
        }
        from
    }

    // ── Focus ────────────────────────────────────────────────────────

    fn focus_next(&mut self) {
        self.focused = self.next_visible(self.focused);
    }

    fn focus_prev(&mut self) {
        self.focused = self.prev_visible(self.focused);
    }

    /// Move focused client to the first master slot (dwm-style zoom).
    fn zoom(&mut self) {
        let mut tiled = [0usize; MAX_WIN];
        let n = self.visible_tiled(&mut tiled);
        if n < 2 { return; }
        // If focused is already master, promote the second client.
        let (master_idx, other_idx) = if tiled[0] == self.focused {
            (tiled[1], tiled[0])
        } else {
            (self.focused, tiled[0])
        };
        // Swap tags so they effectively change positions in the list.
        let tmp = self.clients[master_idx].tags;
        self.clients[master_idx].tags = self.clients[other_idx].tags;
        self.clients[other_idx].tags = tmp;
        // Swap prog_id / pid / title too.
        let ta = self.clients[master_idx];
        let tb = self.clients[other_idx];
        self.clients[master_idx] = tb;
        self.clients[other_idx]  = ta;
        self.focused = other_idx; // follow the originally-focused client
    }

    // ── Tags ─────────────────────────────────────────────────────────

    /// Switch current view to exactly tag_bit (dwm `view`).
    fn view_tag(&mut self, tag_bit: u32) {
        // Rotate tagset pair so the old current becomes "previous".
        self.sel_tags_idx ^= 1;
        self.tagset[self.sel_tags_idx] = tag_bit;
        if !self.is_visible(self.focused) {
            self.focused = self.next_visible(self.focused);
        }
    }

    /// Toggle tag_bit into/out of current view (dwm `toggleview`).
    fn toggle_view(&mut self, tag_bit: u32) {
        let new_tags = self.sel_tags() ^ tag_bit;
        if new_tags != 0 {
            self.tagset[self.sel_tags_idx] = new_tags;
        }
        if !self.is_visible(self.focused) {
            self.focused = self.next_visible(self.focused);
        }
    }

    /// Switch to previous tagset (dwm Mod+Tab).
    fn view_prev_tag(&mut self) {
        self.sel_tags_idx ^= 1;
        if !self.is_visible(self.focused) {
            self.focused = self.next_visible(self.focused);
        }
    }

    /// Toggle a tag bit on a client (dwm `toggletag`).
    fn toggle_client_tag(&mut self, idx: usize, tag_bit: u32) {
        if idx < MAX_WIN && self.clients[idx].alive {
            let new_tags = self.clients[idx].tags ^ tag_bit;
            if new_tags != 0 {
                self.clients[idx].tags = new_tags;
            }
        }
    }

    fn move_to_tag(&mut self, idx: usize, tag_bit: u32) {
        if idx < MAX_WIN && self.clients[idx].alive {
            self.clients[idx].tags = tag_bit;
        }
    }

    /// Toggle layout between current and previous (dwm Mod+Space in some configs).
    fn toggle_layout(&mut self) {
        let tmp = self.layout;
        self.layout = self.prev_layout;
        self.prev_layout = tmp;
    }

    // ── Fullscreen / floating toggles ────────────────────────────────

    fn toggle_fullscreen(&mut self, idx: usize) {
        let c = &mut self.clients[idx];
        if !c.alive { return; }
        if !c.fullscreen {
            c.saved_x = c.x; c.saved_y = c.y;
            c.saved_w = c.w; c.saved_h = c.h;
            c.x = 0; c.y = 0; c.w = self.sw; c.h = self.sh;
        } else {
            c.x = c.saved_x; c.y = c.saved_y;
            c.w = c.saved_w; c.h = c.saved_h;
        }
        c.fullscreen = !c.fullscreen;
    }

    fn toggle_floating(&mut self, idx: usize) {
        if idx < MAX_WIN {
            self.clients[idx].floating = !self.clients[idx].floating;
        }
    }

    // ── Layout engine ────────────────────────────────────────────────

    /// Compute and write (x, y, w, h) geometry into each visible tiled client.
    fn arrange(&mut self) {
        let mut tiled = [0usize; MAX_WIN];
        let n = self.visible_tiled(&mut tiled);
        if n == 0 { return; }

        let gap = if self.gaps_on { GAP } else { 0 };
        // Work area: below status bar (or full screen if bar hidden).
        let bar_offset = if self.show_bar { BAR_H as i32 } else { 0 };
        let wx: i32 = gap;
        let wy: i32 = bar_offset + gap;
        let ww: i32 = self.sw as i32 - gap * 2;
        let wh: i32 = self.sh as i32 - bar_offset - gap * 2;

        match self.layout {
            Layout::Tile          => self.arrange_tile   (&tiled[..n], wx, wy, ww, wh, gap),
            Layout::Monocle       => self.arrange_monocle(&tiled[..n], wx, wy, ww, wh),
            Layout::Grid          => self.arrange_grid   (&tiled[..n], wx, wy, ww, wh, gap),
            Layout::BStack        => self.arrange_bstack (&tiled[..n], wx, wy, ww, wh, gap),
            Layout::Spiral        => self.arrange_spiral (&tiled[..n], wx, wy, ww, wh, gap),
            Layout::Dwindle       => self.arrange_dwindle(&tiled[..n], wx, wy, ww, wh, gap),
            Layout::CenteredMaster=> self.arrange_centeredmaster(&tiled[..n], wx, wy, ww, wh, gap),
        }
    }

    fn arrange_tile(&mut self, t: &[usize], wx: i32, wy: i32, ww: i32, wh: i32, gap: i32) {
        let n  = t.len();
        let nm = self.nmaster.min(n);
        let ns = n - nm;

        let mw = if nm == 0 || ns == 0 { ww } else { ww * self.mfact as i32 / 100 };
        let sw = if ns > 0 { ww - mw - gap } else { 0 };

        // Master column.
        let each_mh = if nm > 0 { (wh - gap * (nm as i32 - 1)) / nm as i32 } else { 0 };
        let mut my = wy;
        for (i, &ci) in t[..nm].iter().enumerate() {
            let h = if i + 1 == nm { (wy + wh) - my } else { each_mh };
            let c = &mut self.clients[ci];
            c.x = wx; c.y = my; c.w = mw.max(1) as u32; c.h = h.max(1) as u32;
            my += h + gap;
        }

        // Stack column.
        let each_sh = if ns > 0 { (wh - gap * (ns as i32 - 1)) / ns as i32 } else { 0 };
        let mut sy = wy;
        for (i, &ci) in t[nm..].iter().enumerate() {
            let h = if i + 1 == ns { (wy + wh) - sy } else { each_sh };
            let c = &mut self.clients[ci];
            c.x = wx + mw + gap; c.y = sy;
            c.w = sw.max(1) as u32; c.h = h.max(1) as u32;
            sy += h + gap;
        }
    }

    fn arrange_monocle(&mut self, t: &[usize], wx: i32, wy: i32, ww: i32, wh: i32) {
        for &ci in t {
            let c = &mut self.clients[ci];
            c.x = wx; c.y = wy; c.w = ww.max(1) as u32; c.h = wh.max(1) as u32;
        }
    }

    fn arrange_grid(&mut self, t: &[usize], wx: i32, wy: i32, ww: i32, wh: i32, gap: i32) {
        let n = t.len();
        let mut cols = 1usize;
        while cols * cols < n { cols += 1; }
        let rows = n.div_ceil(cols);

        let cw = (ww - gap * (cols as i32 - 1)) / cols as i32;
        let ch = (wh - gap * (rows as i32 - 1)) / rows as i32;

        for (i, &ci) in t.iter().enumerate() {
            let col = i % cols;
            let row = i / cols;
            let c = &mut self.clients[ci];
            c.x = wx + col as i32 * (cw + gap);
            c.y = wy + row as i32 * (ch + gap);
            c.w = cw.max(1) as u32;
            c.h = ch.max(1) as u32;
        }
    }

    fn arrange_bstack(&mut self, t: &[usize], wx: i32, wy: i32, ww: i32, wh: i32, gap: i32) {
        let n  = t.len();
        let nm = self.nmaster.min(n);
        let ns = n - nm;

        let mh = if nm == 0 || ns == 0 { wh } else { wh * self.mfact as i32 / 100 };
        let sh = if ns > 0 { wh - mh - gap } else { 0 };

        // Master row (horizontal).
        let each_mw = if nm > 0 { (ww - gap * (nm as i32 - 1)) / nm as i32 } else { 0 };
        let mut mx = wx;
        for (i, &ci) in t[..nm].iter().enumerate() {
            let w = if i + 1 == nm { (wx + ww) - mx } else { each_mw };
            let c = &mut self.clients[ci];
            c.x = mx; c.y = wy; c.w = w.max(1) as u32; c.h = mh.max(1) as u32;
            mx += w + gap;
        }

        // Stack row.
        let each_sw = if ns > 0 { (ww - gap * (ns as i32 - 1)) / ns as i32 } else { 0 };
        let mut sx = wx;
        for (i, &ci) in t[nm..].iter().enumerate() {
            let w = if i + 1 == ns { (wx + ww) - sx } else { each_sw };
            let c = &mut self.clients[ci];
            c.x = sx; c.y = wy + mh + gap;
            c.w = w.max(1) as u32; c.h = sh.max(1) as u32;
            sx += w + gap;
        }
    }

    fn arrange_spiral(&mut self, t: &[usize], wx: i32, wy: i32, ww: i32, wh: i32, gap: i32) {
        let n = t.len();
        let mut cx = wx; let mut cy = wy;
        let mut cw = ww; let mut ch = wh;

        for (i, &ci) in t.iter().enumerate() {
            if i == n - 1 {
                let c = &mut self.clients[ci];
                c.x = cx; c.y = cy; c.w = cw.max(1) as u32; c.h = ch.max(1) as u32;
                break;
            }
            if i % 2 == 0 {
                // Split horizontally: top half for current.
                let half = (ch - gap) / 2;
                let c = &mut self.clients[ci];
                c.x = cx; c.y = cy; c.w = cw.max(1) as u32; c.h = half.max(1) as u32;
                cy += half + gap;
                ch -= half + gap;
            } else {
                // Split vertically: left half for current.
                let half = (cw - gap) / 2;
                let c = &mut self.clients[ci];
                c.x = cx; c.y = cy; c.w = half.max(1) as u32; c.h = ch.max(1) as u32;
                cx += half + gap;
                cw -= half + gap;
            }
        }
    }

    /// Dwindle: like spiral but each split takes the dwindling side for the next.
    fn arrange_dwindle(&mut self, t: &[usize], wx: i32, wy: i32, ww: i32, wh: i32, gap: i32) {
        let n = t.len();
        let mut cx = wx; let mut cy = wy;
        let mut cw = ww; let mut ch = wh;

        for (i, &ci) in t.iter().enumerate() {
            if i == n - 1 {
                let c = &mut self.clients[ci];
                c.x = cx; c.y = cy; c.w = cw.max(1) as u32; c.h = ch.max(1) as u32;
                break;
            }
            // Dwindle alternates splits, but keeps the remainder on the same side
            if i % 2 == 0 {
                let half = (ch - gap) / 2;
                let c = &mut self.clients[ci];
                c.x = cx; c.y = cy + half + gap;
                c.w = cw.max(1) as u32; c.h = (ch - half - gap).max(1) as u32;
                ch = half;
            } else {
                let half = (cw - gap) / 2;
                let c = &mut self.clients[ci];
                c.x = cx + half + gap; c.y = cy;
                c.w = (cw - half - gap).max(1) as u32; c.h = ch.max(1) as u32;
                cw = half;
            }
        }
    }

    /// CenteredMaster: master in centre, stack split left and right.
    fn arrange_centeredmaster(&mut self, t: &[usize], wx: i32, wy: i32, ww: i32, wh: i32, gap: i32) {
        let n   = t.len();
        let nm  = self.nmaster.min(n);
        let ns  = n - nm;

        if ns == 0 {
            self.arrange_tile(t, wx, wy, ww, wh, gap);
            return;
        }

        let mw      = ww * self.mfact as i32 / 100;
        let side_w  = (ww - mw - gap * 2) / 2;
        let mx      = wx + side_w + gap;

        // Master column (centre)
        let each_mh = if nm > 0 { (wh - gap * (nm as i32 - 1)) / nm as i32 } else { 0 };
        let mut my = wy;
        for (i, &ci) in t[..nm].iter().enumerate() {
            let h = if i + 1 == nm { (wy + wh) - my } else { each_mh };
            let c = &mut self.clients[ci];
            c.x = mx; c.y = my; c.w = mw.max(1) as u32; c.h = h.max(1) as u32;
            my += h + gap;
        }

        // Stack: alternate left / right
        let left_n  = (ns + 1) / 2;
        let right_n = ns / 2;
        let each_lh = if left_n  > 0 { (wh - gap * (left_n  as i32 - 1)) / left_n  as i32 } else { 0 };
        let each_rh = if right_n > 0 { (wh - gap * (right_n as i32 - 1)) / right_n as i32 } else { 0 };
        let (mut ly, mut ry) = (wy, wy);
        let (mut li, mut ri) = (0usize, 0usize);

        for (i, &ci) in t[nm..].iter().enumerate() {
            let c = &mut self.clients[ci];
            if i % 2 == 0 {
                let h = if li + 1 == left_n { (wy + wh) - ly } else { each_lh };
                c.x = wx; c.y = ly; c.w = side_w.max(1) as u32; c.h = h.max(1) as u32;
                ly += h + gap; li += 1;
            } else {
                let rx = mx + mw + gap;
                let h = if ri + 1 == right_n { (wy + wh) - ry } else { each_rh };
                c.x = rx; c.y = ry; c.w = side_w.max(1) as u32; c.h = h.max(1) as u32;
                ry += h + gap; ri += 1;
            }
        }
    }
}

// ── Pixel font ───────────────────────────────────────────────────────────────
// ── 4×6 pixel font (full printable ASCII 0x20–0x7E) ─────────────────────────
// Encoding: u32 where nibble N (bits 4N+3 .. 4N) = row N pixels.
// Within each nibble: bit 0 = leftmost col, bit 3 = rightmost col.
// 6 rows (nibbles 0–5); bits 24–31 unused.

#[rustfmt::skip]
const FONT: [u32; 95] = [
    // 0x20 ' '  0x21 '!'  0x22 '"'  0x23 '#'  0x24 '$'  0x25 '%'  0x26 '&'  0x27 '\''
    0x000000, 0x020222, 0x00000A, 0x0AFAFA, 0x07861E, 0x094B26, 0x0D6664, 0x000002,
    // 0x28 '('  0x29 ')'  0x2A '*'  0x2B '+'  0x2C ','  0x2D '-'  0x2E '.'  0x2F '/'
    0x042112, 0x021224, 0x000A5A, 0x002720, 0x012000, 0x000600, 0x020000, 0x001248,
    // 0x30 '0'  0x31 '1'  0x32 '2'  0x33 '3'  0x34 '4'  0x35 '5'  0x36 '6'  0x37 '7'
    0b_0110_1001_1011_1101_1001_0110,
    0b_0100_1100_0100_0100_0100_1110,
    0b_0110_1001_0001_0010_0100_1111,
    0b_1110_0001_0110_0001_0001_1110,
    0b_0010_0110_1010_1111_0010_0010,
    0b_1111_1000_1110_0001_0001_1110,
    0b_0110_1000_1110_1001_1001_0110,
    0b_1111_0001_0010_0010_0100_0100,
    // 0x38 '8'  0x39 '9'  0x3A ':'  0x3B ';'  0x3C '<'  0x3D '='  0x3E '>'  0x3F '?'
    0b_0110_1001_0110_1001_1001_0110,
    0b_0110_1001_0111_0001_0001_0110,
    0x006060, 0x012060, 0x042124, 0x00F0F0, 0x042412, 0x020210,
    // 0x40 '@'  0x41 'A'  0x42 'B'  0x43 'C'  0x44 'D'  0x45 'E'  0x46 'F'  0x47 'G'
    0b_0110_1001_1011_1011_1000_0110,
    0x099F96, 0x079797, 0x0E111E, 0x079997, 0x0F171F, 0x01171F, 0x0E9D1E,
    // 0x48 'H'  0x49 'I'  0x4A 'J'  0x4B 'K'  0x4C 'L'  0x4D 'M'  0x4E 'N'  0x4F 'O'
    0x099F99, 0x072227, 0x06544E, 0x095359, 0x0F1111, 0x0999F9, 0x099DB9, 0x069996,
    // 0x50 'P'  0x51 'Q'  0x52 'R'  0x53 'S'  0x54 'T'  0x55 'U'  0x56 'V'  0x57 'W'
    0x011797, 0x0ED996, 0x095797, 0x07861E, 0x02222F, 0x069999, 0x066999, 0x06F999,
    // 0x58 'X'  0x59 'Y'  0x5A 'Z'  0x5B '['  0x5C '\\' 0x5D ']'  0x5E '^'  0x5F '_'
    0x096669, 0x022269, 0x0F124F,
    0b_0110_0100_0100_0100_0100_0110,
    0b_1000_1000_0100_0010_0001_0001,
    0b_0110_0010_0010_0010_0010_0110,
    0x000096, 0x0F0000,
    // 0x60 '`'  0x61–0x7A: lowercase mapped to uppercase (small-caps style)
    0x000004,
    // a–z: same bitmaps as A–Z (indices 33–58 in FONT)
    0x099F96, 0x079797, 0x0E111E, 0x079997, 0x0F171F, 0x01171F, 0x0E9D1E,
    0x099F99, 0x072227, 0x06544E, 0x095359, 0x0F1111, 0x0999F9, 0x099DB9, 0x069996,
    0x011797, 0x0ED996, 0x095797, 0x07861E, 0x02222F, 0x069999, 0x066999, 0x06F999,
    0x096669, 0x022269, 0x0F124F,
    // 0x7B '{'  0x7C '|'  0x7D '}'  0x7E '~'
    0x026226,
    0b_0100_0100_0100_0100_0100_0100,
    0x064426, 0x00050A,
];

const FONT_W: u32 = 4;
const FONT_H: u32 = 6;

#[inline]
fn glyph_bits(ch: u8) -> u32 {
    if ch < 0x20 || ch > 0x7E { return 0; }
    FONT[(ch - 0x20) as usize]
}

/// Draw a single character using row-based blits (1 syscall per run of set pixels per row).
/// This is ~10× fewer syscalls than the old pixel-by-pixel approach.
fn draw_char(px: u32, py: u32, ch: u8, color: u32) {
    let bits = glyph_bits(ch);
    if bits == 0 { return; }
    for row in 0..FONT_H {
        let row_bits = (bits >> (row * FONT_W)) & 0xF;
        if row_bits == 0 { continue; }
        // Emit one fill_rect per contiguous run of set pixels in this row.
        let mut col = 0u32;
        while col < FONT_W {
            if (row_bits >> col) & 1 == 1 {
                let start = col;
                while col < FONT_W && (row_bits >> col) & 1 == 1 { col += 1; }
                let _ = sys_fb_fill_rect(px + start, py + row, col - start, 1, color);
            } else {
                col += 1;
            }
        }
    }
}

/// Draw a text slice at (px, py). Returns the x position after the last char.
fn draw_text(px: u32, py: u32, s: &[u8], color: u32) -> u32 {
    let mut x = px;
    for &ch in s {
        draw_char(x, py, ch, color);
        x += FONT_W + 1; // 1px kerning
    }
    x
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn draw_bar(wm: &Wm) {
    // Background.
    let _ = sys_fb_fill_rect(0, 0, wm.sw, BAR_H, C_BAR_BG);

    let text_y = (BAR_H - FONT_H) / 2; // vertically centred text

    // ── Tag strip ───────────────────────────────────────────────────
    let tag_slot_w: u32 = BAR_H; // square slots
    for t in 0..TAG_CNT {
        let tag_bit = 1u32 << t;
        let tx = t as u32 * tag_slot_w;

        // Classify this tag.
        let is_sel  = (wm.sel_tags() & tag_bit) != 0;
        let is_occ  = (0..MAX_WIN).any(|i| wm.clients[i].alive && (wm.clients[i].tags & tag_bit) != 0);

        let bg_col = if is_sel { C_TAG_ACT } else if is_occ { C_TAG_OCC } else { C_TAG_IN };
        let fg_col = if is_sel { C_BAR_BG  } else { 0xFF_FF_FF_FF };

        let _ = sys_fb_fill_rect(tx, 0, tag_slot_w, BAR_H, bg_col);
        // Draw digit centred in the slot.
        let cx = tx + (tag_slot_w - FONT_W) / 2;
        draw_char(cx, text_y, b'1' + t as u8, fg_col);

        // Occupied dot: 2×2 at bottom-right corner.
        if is_occ && !is_sel {
            let _ = sys_fb_fill_rect(tx + tag_slot_w - 4, BAR_H - 4, 2, 2, C_STATUS);
        }
    }

    // ── Layout symbol ────────────────────────────────────────────────
    let lx = TAG_CNT as u32 * tag_slot_w + 6;
    draw_text(lx, text_y, wm.layout.symbol(), C_LAYOUT_SYM);

    // ── Focused window title ──────────────────────────────────────────
    let sym_end = lx + wm.layout.symbol().len() as u32 * (FONT_W + 1) + 10;
    let title_x = wm.sw / 2; // centred
    if wm.is_visible(wm.focused) {
        let c = &wm.clients[wm.focused];
        let ts = &c.title[..c.title_len as usize];
        // Draw centred: compute width first.
        let tw = ts.len() as u32 * (FONT_W + 1);
        let tx = if title_x > tw / 2 { title_x - tw / 2 } else { sym_end };
        draw_text(tx, text_y, ts, C_TITLE);
        // Floating indicator.
        if c.floating {
            let fx = tx + tw + 4;
            let _ = sys_fb_fill_rect(fx, text_y + 1, 3, 3, C_FLOAT_MARK);
        }
    }

    // ── Status text (right-aligned) ───────────────────────────────────
    let status: &[u8] = b"rogueos";
    let sw = status.len() as u32 * (FONT_W + 1);
    let rx = wm.sw.saturating_sub(sw + 6);
    draw_text(rx, text_y, status, C_STATUS);

    // Separator line below bar.
    let _ = sys_fb_fill_rect(0, BAR_H - 1, wm.sw, 1, 0xFF_29_2E_42);
}

// ── RDP compositor helpers ────────────────────────────────────────────────────

/// Compute the content area dimensions for a client (what the RDP app renders into).
/// Strips away the outer border and per-window title bar.
#[inline]
fn content_dims(w: u32, h: u32) -> (u32, u32) {
    let cw = w.saturating_sub(BORDER * 2);
    let ch = h.saturating_sub(BORDER * 2 + TITLE_H);
    (cw, ch)
}

/// Compute the content area origin for blitting the RDP surface.
#[inline]
fn content_origin(x: i32, y: i32) -> (u32, u32) {
    ((x as u32) + BORDER, (y as u32) + BORDER + TITLE_H)
}

/// Send RdpGrant to a client with content-area dimensions (not window dims).
fn send_kdp_grant(client_pid: u32, surface_id: u32, x: i32, y: i32, w: u32, h: u32, title: &[u8]) {
    let (cw, ch) = content_dims(w, h);
    let mut msg = RwmMsg::ZERO;
    msg.msg_type = RDP_GRANT;
    let kdp = unsafe { &mut msg.payload.rdp };
    kdp.surface_id = surface_id;
    kdp.x     = x;
    kdp.y     = y;
    kdp.width  = cw;
    kdp.height = ch;
    let n = title.len().min(kdp.title.len().saturating_sub(1));
    kdp.title[..n].copy_from_slice(&title[..n]);
    let _ = sys_ipc_send(client_pid, &msg, 0);
}

/// Send RdpResize to a client with updated content-area dimensions.
fn send_kdp_resize(client_pid: u32, surface_id: u32, w: u32, h: u32) {
    let (cw, ch) = content_dims(w, h);
    let mut msg = RwmMsg::ZERO;
    msg.msg_type = RDP_RESIZE;
    let kdp = unsafe { &mut msg.payload.rdp };
    kdp.surface_id = surface_id;
    kdp.width  = cw;
    kdp.height = ch;
    let _ = sys_ipc_send(client_pid, &msg, 0);
}

/// Send RdpPresentDone to a client: the frame has been composited; safe to render next.
fn send_rdp_present_done(client_pid: u32, surface_id: u32, ack_seq: u16) {
    let mut msg = RwmMsg::ZERO;
    msg.msg_type = RDP_PRESENT_DONE;
    msg.seq      = ack_seq;
    let kdp = unsafe { &mut msg.payload.rdp };
    kdp.surface_id = surface_id;
    let _ = sys_ipc_send(client_pid, &msg, 0);
}

/// After arrange(), send RDP_RESIZE to any client whose content dimensions changed.
/// Updates `client.notified_cw/ch` so we only send when something actually moved.
fn notify_layout_changes(wm: &mut Wm) {
    for i in 0..MAX_WIN {
        // Extract everything we need before any mutable borrow.
        let (alive, pid, sid, w, h, nw, nh, tlen) = {
            let c = &wm.clients[i];
            (c.alive, c.pid, c.surface_id, c.w, c.h, c.notified_cw, c.notified_ch, c.title_len)
        };
        if !alive || pid == 0 || sid == 0 { continue; }
        let (cw, ch) = content_dims(w, h);
        if cw != nw || ch != nh {
            send_kdp_resize(pid, sid, w, h);
            wm.clients[i].notified_cw = cw;
            wm.clients[i].notified_ch = ch;
            cog_log(b"[WM] Pardon me, sir - resizing '");
            // copy title slice before log to avoid borrow issues
            let title_copy = wm.clients[i].title;
            cog_log(&title_copy[..tlen as usize]);
            cog_log(b"' to fit the new arrangement.\r\n");
        }
    }
}

/// Set z-order on all live RDP surfaces: focused = 255, others ordered by slot.
fn update_z_order(wm: &Wm) {
    let focused = wm.focused;
    for i in 0..MAX_WIN {
        let c = &wm.clients[i];
        if !c.alive || c.surface_id == 0 { continue; }
        let z: u8 = if i == focused { 255 } else { i as u8 };
        let _ = sys_surface_set_z(c.surface_id, z);
    }
}

/// Forward a key event to a focused RDP client.
fn send_kdp_key(client_pid: u32, keycode: u8, pressed: bool) {
    let mut msg = RwmMsg::ZERO;
    msg.msg_type = RDP_KEY;
    let kdp = unsafe { &mut msg.payload.rdp };
    kdp.key_code = keycode as u32;
    kdp.key_state = if pressed { 1 } else { 0 };
    let _ = sys_ipc_send(client_pid, &msg, 0);
}

/// Send RdpFocus to a client: focused=true or false.
fn send_kdp_focus(client_pid: u32, focused: bool) {
    let mut msg = RwmMsg::ZERO;
    msg.msg_type = RDP_FOCUS;
    let kdp = unsafe { &mut msg.payload.rdp };
    kdp.flags = if focused { 1 } else { 0 };
    let _ = sys_ipc_send(client_pid, &msg, 0);
}

/// Send RdpClose to a client: politely ask it to exit.
fn send_kdp_close(client_pid: u32, surface_id: u32) {
    let mut msg = RwmMsg::ZERO;
    msg.msg_type = RDP_CLOSE;
    let kdp = unsafe { &mut msg.payload.rdp };
    kdp.surface_id = surface_id;
    let _ = sys_ipc_send(client_pid, &msg, 0);
}

/// Handle one incoming IPC message addressed to the compositor.
/// Returns true if the scene needs a redraw.
fn handle_ipc_msg(wm: &mut Wm, msg: &RwmMsg) -> bool {
    let sender = msg.sender_pid;
    let kdp = unsafe { msg.payload.rdp };

    match msg.msg_type {
        RDP_CONNECT => {
            let surface_id = kdp.surface_id;
            let tlen = kdp.title.iter().position(|&b| b == 0).unwrap_or(kdp.title.len());
            let title = &kdp.title[..tlen];
            let idx = wm.add_client(0xFF, title);
            {
                let c = &mut wm.clients[idx];
                c.pid        = sender;
                c.surface_id = surface_id;
            }
            wm.focused = idx;
            wm.arrange();
            let (cx, cy, cw_full, ch_full) = {
                let c = &wm.clients[idx];
                (c.x, c.y, c.w, c.h)
            };
            let (cw, ch) = content_dims(cw_full, ch_full);
            wm.clients[idx].notified_cw = cw;
            wm.clients[idx].notified_ch = ch;
            send_kdp_grant(sender, surface_id, cx, cy, cw_full, ch_full, title);
            update_z_order(wm);
            cog_log(b"[WM] Good to have you, sir. Window '");
            cog_log(title);
            cog_log(b"' has joined the workspace.\r\n");
            true
        }
        RDP_COMMIT => {
            // Client's pixel buffer is ready. Blit at the content area origin
            // (border + title bar offset) and send PresentDone frame callback.
            let surface_id = kdp.surface_id;
            let commit_seq = msg.seq;
            for i in 0..MAX_WIN {
                let c = &wm.clients[i];
                if c.alive && c.pid == sender && c.surface_id == surface_id {
                    let (ox, oy) = content_origin(c.x, c.y);
                    let _ = sys_surface_commit(surface_id, ox, oy);
                    // FIX #3: update z-order so focused client renders on top.
                    update_z_order(wm);
                    // FIX #2: send frame callback so client knows it can render next frame.
                    send_rdp_present_done(sender, surface_id, commit_seq);
                    break;
                }
            }
            true // trigger chrome redraw so title bar stays crisp
        }
        RDP_DISCONNECT => {
            let surface_id = kdp.surface_id;
            for i in 0..MAX_WIN {
                let c = &wm.clients[i];
                if c.alive && c.pid == sender && c.surface_id == surface_id {
                    cog_log(b"[WM] Window '");
                    cog_log(&wm.clients[i].title[..wm.clients[i].title_len as usize]);
                    cog_log(b"' has taken its leave, sir.\r\n");
                    let _ = sys_surface_destroy(surface_id);
                    wm.remove_client(i);
                    break;
                }
            }
            true
        }
        _ => false,
    }
}

// ── Scene rendering ───────────────────────────────────────────────────────────

fn draw_scene(wm: &mut Wm) {
    // Recompute layout geometry then notify clients of any size changes.
    wm.arrange();
    notify_layout_changes(wm);
    // Propagate z-order to display server.
    update_z_order(wm);

    let _ = sys_fb_clear(C_BG);

    if wm.show_bar {
        draw_bar(wm);
    }

    // Two-pass draw: unfocused first, focused last (paints on top of others).
    let focused = wm.focused;
    for pass in 0..2u8 {
        for i in 0..MAX_WIN {
            let c = &wm.clients[i];
            if !c.alive || (c.tags & wm.sel_tags()) == 0 { continue; }

            let is_focused = i == focused;
            if pass == 0 && is_focused  { continue; }
            if pass == 1 && !is_focused { continue; }

            let (x, y, w, h) = (c.x, c.y, c.w, c.h);
            if w == 0 || h == 0 { continue; }

            let border_col   = if is_focused { C_BORDER_ACT } else { C_BORDER_IN };
            let titlebar_col = if is_focused { C_BORDER_ACT } else { 0xFF_1E_20_30 };
            let content_col  = if is_focused { C_WIN_ACT    } else { C_WIN_BG };

            // ── 1. Outer border ──────────────────────────────────────────
            let _ = sys_fb_fill_rect(x as u32, y as u32, w, h, border_col);

            if w <= BORDER * 2 || h <= BORDER * 2 { continue; }
            let inner_x = x as u32 + BORDER;
            let inner_y = y as u32 + BORDER;
            let inner_w = w - BORDER * 2;
            let inner_h = h - BORDER * 2;

            // ── 2. Per-window title bar (FIX #6) ────────────────────────
            let tb_h = TITLE_H.min(inner_h);
            let _ = sys_fb_fill_rect(inner_x, inner_y, inner_w, tb_h, titlebar_col);

            // Title text centred in the title bar.
            if tb_h >= FONT_H {
                let ts = &c.title[..c.title_len as usize];
                let tw = ts.len() as u32 * (FONT_W + 1);
                let tx = if inner_w > tw { inner_x + (inner_w - tw) / 2 } else { inner_x + 2 };
                let ty = inner_y + (tb_h - FONT_H) / 2;
                let fg = if is_focused { 0xFF_FF_FF_FF } else { C_TAG_OCC };
                draw_text(tx, ty, ts, fg);
            }

            // Floating indicator in title bar (orange pill).
            if c.floating {
                let _ = sys_fb_fill_rect(inner_x + 2, inner_y + (tb_h - 3) / 2, 8, 3, C_FLOAT_MARK);
            }

            // ── 3. Content area fill ─────────────────────────────────────
            if inner_h > tb_h {
                let cy = inner_y + tb_h;
                let ch = inner_h - tb_h;
                let _ = sys_fb_fill_rect(inner_x, cy, inner_w, ch, content_col);

                // ── 4. RDP surface blit or placeholder ───────────────────
                if c.surface_id != 0 {
                    // Blit client's rendered pixels at the content area origin (FIX #3 + #5).
                    let _ = sys_surface_commit(c.surface_id, inner_x, cy);
                } else if inner_w > 8 && ch > 8 {
                    draw_window_content(wm, i, is_focused);
                }
            }
        }
    }

    let _ = sys_fb_flush();
}

/// Draw simple content placeholder inside a window.
fn draw_window_content(wm: &Wm, idx: usize, focused: bool) {
    let c = &wm.clients[idx];
    let ix = c.x as u32 + BORDER + 4;
    let iy = c.y as u32 + BORDER + 6;
    let iw = (c.w as i32 - BORDER as i32 * 2 - 8).max(0) as u32;
    let ih = (c.h as i32 - BORDER as i32 * 2 - 12).max(0) as u32;
    if iw == 0 || ih == 0 { return; }

    // Distinct background tint per prog_id.
    let tint: u32 = match c.prog_id {
        0 => 0xFF_20_25_3A, // shell  – slightly blueish
        2 => 0xFF_20_3A_25, // editor – greenish
        3 => 0xFF_3A_20_25, // viewer – reddish
        _ => 0xFF_28_28_38,
    };
    let _ = sys_fb_fill_rect(ix, iy, iw, ih, tint);

    // Title text in the window.
    let ts = &c.title[..c.title_len as usize];
    let fg = if focused { C_TITLE } else { C_TAG_OCC };
    if iw >= (FONT_W + 1) && ih >= FONT_H {
        draw_text(ix + 4, iy + 4, ts, fg);
    }

    // Simulated "cursor" in focused window.
    if focused && iw > 10 && ih > FONT_H + 8 {
        let _ = sys_fb_fill_rect(ix + 4, iy + FONT_H + 8, 2, FONT_H, C_STATUS);
    }
}

// ── Keyboard handling ─────────────────────────────────────────────────────────

fn handle_key(wm: &mut Wm, key: u8, pressed: bool) -> bool {
    // Only act on WM bindings for key press events.
    if !pressed {
        // Forward release events to focused RDP client and return.
        let idx = wm.focused;
        if idx < MAX_WIN && wm.clients[idx].alive {
            let c = &wm.clients[idx];
            if c.pid != 0 && c.surface_id != 0 {
                send_kdp_key(c.pid, key, false);
            }
        }
        return false;
    }

    // Mod+<key> bindings.
    if wm.mod_dn && !wm.shift_dn {
        match key {
            // Tag switch: Mod+1..9
            k @ KEY_1..=KEY_9 => {
                wm.view_tag(1 << (k - KEY_1));
                return true;
            }
            // View all: Mod+0
            KEY_0 => {
                wm.view_tag((1u32 << TAG_CNT) - 1);
                return true;
            }
            // Toggle bar: Mod+B
            KEY_B => { wm.show_bar = !wm.show_bar; return true; }
            // Previous tag: Mod+Tab
            KEY_TAB => { wm.view_prev_tag(); return true; }
            // Focus navigation.
            KEY_J => { wm.focus_next(); return true; }
            KEY_K => { wm.focus_prev(); return true; }
            // Master width.
            KEY_H => {
                if wm.mfact > 10 { wm.mfact -= 5; }
                return true;
            }
            KEY_L => {
                if wm.mfact < 90 { wm.mfact += 5; }
                return true;
            }
            // nmaster.
            KEY_COMMA => {
                if wm.nmaster > 0 { wm.nmaster -= 1; }
                return true;
            }
            KEY_PERIOD => {
                wm.nmaster += 1;
                return true;
            }
            // Layout cycling (save previous for toggle).
            KEY_SPACE => {
                wm.prev_layout = wm.layout;
                wm.layout = wm.layout.next();
                return true;
            }
            // Zoom (move to master).
            KEY_ENTER => { wm.zoom(); return true; }
            // Spawn programs.
            KEY_D => {
                let pid = sys_spawn(PROG_SHELL);
                if pid > 0 {
                    let idx = wm.add_client(PROG_SHELL as u8, b"shell");
                    wm.clients[idx].pid = pid as u32;
                    wm.focused = idx;
                }
                return true;
            }
            KEY_E => {
                let pid = sys_spawn(PROG_EDITOR);
                if pid > 0 {
                    let idx = wm.add_client(PROG_EDITOR as u8, b"editor");
                    wm.clients[idx].pid = pid as u32;
                    wm.focused = idx;
                }
                return true;
            }
            KEY_V => {
                let pid = sys_spawn(PROG_VIEWER);
                if pid > 0 {
                    let idx = wm.add_client(PROG_VIEWER as u8, b"viewer");
                    wm.clients[idx].pid = pid as u32;
                    wm.focused = idx;
                }
                return true;
            }
            // Toggles.
            KEY_F => { wm.toggle_fullscreen(wm.focused); return true; }
            KEY_T => { wm.toggle_floating(wm.focused); return true; }
            KEY_G => { wm.gaps_on = !wm.gaps_on; return true; }
            _ => {}
        }
    }

    // Mod+Ctrl+<key>: toggleview / toggle client tag
    if wm.mod_dn && wm.ctrl_dn && !wm.shift_dn {
        match key {
            // Toggle tag N into/out of current view: Mod+Ctrl+1..9
            k @ KEY_1..=KEY_9 => {
                wm.toggle_view(1 << (k - KEY_1));
                return true;
            }
            _ => {}
        }
    }

    // Mod+Shift+Ctrl+<key>: toggle tag on focused client
    if wm.mod_dn && wm.shift_dn && wm.ctrl_dn {
        match key {
            k @ KEY_1..=KEY_9 => {
                wm.toggle_client_tag(wm.focused, 1 << (k - KEY_1));
                return true;
            }
            _ => {}
        }
    }

    // Mod+Shift+<key> bindings.
    if wm.mod_dn && wm.shift_dn && !wm.ctrl_dn {
        match key {
            // Move focused window to tag N: Mod+Shift+1..9
            k @ KEY_1..=KEY_9 => {
                wm.move_to_tag(wm.focused, 1 << (k - KEY_1));
                return true;
            }
            // Cycle layouts backward.
            KEY_SPACE => {
                wm.prev_layout = wm.layout;
                wm.layout = wm.layout.prev();
                return true;
            }
            // Close focused window: send RdpClose IPC if it's a RDP client; remove otherwise.
            KEY_C => {
                let idx = wm.focused;
                if idx < MAX_WIN && wm.clients[idx].alive {
                    let c = &wm.clients[idx];
                    if c.pid != 0 && c.surface_id != 0 {
                        // Politely ask the RDP client to close; it will send RdpDisconnect back.
                        send_kdp_close(c.pid, c.surface_id);
                    } else {
                        wm.remove_client(idx);
                    }
                }
                return true;
            }
            // Quit / reboot.
            KEY_Q => {
                cog_log(b"[Cogman] Reboot requested, sir. Tidying up and restarting presently.\r\n");
                let _ = userland::sys_reboot(1);
                sys_exit(0);
            }
            _ => {}
        }
    }

    // No WM binding matched — forward the key press to the focused RDP client (if any).
    let idx = wm.focused;
    if idx < MAX_WIN && wm.clients[idx].alive {
        let c = &wm.clients[idx];
        if c.pid != 0 && c.surface_id != 0 {
            send_kdp_key(c.pid, key, true);
        }
    }
    false
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[no_mangle]
fn _start() -> ! {
    cog_log(b"\r\n[Cogman] RogueOS window manager reporting for duty, sir.\r\n");
    cog_log(b"[Cogman] Claiming compositor authority. One moment, please.\r\n");

    let claim_r = sys_claim_compositor();
    if claim_r < 0 {
        cog_log(b"[Cogman] I'm afraid compositor authority is already held, sir. \
                  Another WM may be running. Proceeding regardless.\r\n");
    } else {
        cog_log(b"[Cogman] Compositor authority secured. \
                  I am now the sole arbiter of what reaches the screen, sir.\r\n");
    }

    let mut wm = Wm::new();

    // Pre-create a welcome placeholder (no RDP surface; drawn with fb ops).
    let idx = wm.add_client(0xFF, b"welcome");
    wm.clients[idx].tags = 0x01;
    wm.focused = idx;

    draw_scene(&mut wm);
    cog_log(b"[Cogman] Initial scene rendered, sir. Your workspace is ready.\r\n");

    let mut ev = KeyEvent { keycode: 0, pressed: false };
    let mut ipc_msg = RwmMsg::ZERO;
    let mut idle_ticks: u32 = 0;

    loop {
        idle_ticks = idle_ticks.wrapping_add(1);

        // ── 1. Drain IPC queue (non-blocking) ────────────────────────────
        let mut ipc_changed = false;
        while sys_ipc_recv(&mut ipc_msg, IPC_NONBLOCK) == 0 {
            if handle_ipc_msg(&mut wm, &ipc_msg) {
                ipc_changed = true;
            }
        }

        // ── 2. Poll keyboard (non-blocking) ──────────────────────────────
        let mut key_changed = false;
        let n = sys_poll_input(&mut ev);
        if n > 0 {
            // Track modifier state (both press and release).
            match ev.keycode {
                KEY_MOD   => { wm.mod_dn   = ev.pressed; }
                KEY_SHIFT => { wm.shift_dn = ev.pressed; }
                KEY_CTRL  => { wm.ctrl_dn  = ev.pressed; }
                _ => {
                    if handle_key(&mut wm, ev.keycode, ev.pressed) {
                        key_changed = true;
                    }
                }
            }
        }

        // ── 3. Redraw if anything changed ────────────────────────────────
        if ipc_changed || key_changed || wm.dirty {
            draw_scene(&mut wm);
            wm.dirty = false;
        }

        // Periodic serial heartbeat (every ~200k ticks).
        if idle_ticks % 200_000 == 0 {
            cog_log(b"[Cogman] All quiet on the desktop, sir.\r\n");
        }
    }
}

/// Serial output — used for all Cogman-persona WM messages.
fn cog_log(msg: &[u8]) {
    let _ = sys_write(1, msg.as_ptr(), msg.len());
}

/// Legacy alias kept so internal call-sites compile without mass rename.
#[allow(dead_code)]
fn log(msg: &[u8]) { cog_log(msg); }
