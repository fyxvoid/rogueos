//! rogue_ds — Rogue Display Server
//!
//! The display server is the **single compositor process** that owns the
//! physical framebuffer on RogueOS.  It replaces the old single-process
//! WM that called `sys_fb_*` directly.
//!
//! ## Responsibilities
//!
//! 1. **Window management** — 9-tag workspace, 7 layout modes (Tile, Monocle,
//!    Grid, BStack, Spiral, Dwindle, CenteredMaster), focus tracking, border/bar
//!    rendering.  Algorithm source: rogueos/userland/rwm-core (ported from
//!    rogue-desktop/dwm-rs).
//!
//! 2. **Surface compositing** — Each client app is assigned a kernel surface
//!    (`SYS_SURFACE_CREATE`).  On each frame the DS blits the bar surface, then
//!    each window surface in z-order via `SYS_SURFACE_COMMIT`, then draws
//!    decorations (borders, bar) via `sys_fb_fill_rect`.
//!
//! 3. **Client IPC** — Apps connect by sending `RwmMsg::Register`.  The DS
//!    assigns a surface ID and geometry, then forwards input events to the
//!    focused window.
//!
//! 4. **Input dispatch** — Polls `sys_poll_input` / `sys_poll_mouse`.  WM
//!    shortcuts (Mod+...) are consumed here; all other key events are forwarded
//!    to the focused client via `SYS_IPC_SEND`.
//!
//! ## Architecture diagram
//!
//! ```
//! Apps ──RwmMsg::Register──► rogue_ds ──RwmMsg::Geometry──► Apps
//! Apps ──RwmMsg::SurfaceCommit──────────►│
//!                                         │ SYS_SURFACE_ATTACH + COMMIT
//!                                         ▼
//!                               Kernel Display Server
//!                                         │
//!                                         ▼
//!                               GOP Framebuffer (AMD SME encrypted)
//! ```
//!
//! ## Why NOT a Wayland compositor
//!
//! Wayland requires Unix domain sockets, shared memory via `memfd`, and a
//! running Linux kernel.  RogueOS uses kernel IPC (RwmMsg over
//! `SYS_IPC_SEND`/`SYS_IPC_RECV`) which is simpler, faster (no socket
//! copies), and avoids the 100 kLoC Wayland protocol machinery.
//!
//! ## Why NOT X11
//!
//! Same reasons plus: X11 requires an X server process, separate window IDs,
//! GraphicsContexts, pixmap allocations, and EWMH/ICCCM handshake.  RogueOS's
//! surface protocol achieves the same result in ~400 lines.

#![no_std]
#![no_main]

use libs::{
    keycodes::*, IPC_NONBLOCK, RwmMsg, RwmPayload, RwmType,
    PayloadEventFocus, PayloadEventKey, PayloadGeometry,
    PayloadRaw, PayloadSurfaceAssign,
    SYSERR_AGAIN,
};
use userland::{
    sys_exit, sys_fb_fill_rect, sys_fb_flush, sys_getpid,
    sys_ipc_recv, sys_ipc_send,
    sys_poll_input, sys_poll_mouse,
    sys_screen_size, sys_surface_attach, sys_surface_commit, sys_surface_create,
    sys_surface_destroy, sys_write,
};

// ── Theme (Tokyo Night) ──────────────────────────────────────────────────────

const C_BG:         u32 = 0xFF_1A_1B_26;
const C_BAR_BG:     u32 = 0xFF_16_17_1F;
const C_WIN_BG:     u32 = 0xFF_1F_20_2E;
const C_WIN_ACT:    u32 = 0xFF_24_28_3D;
const C_BORDER_ACT: u32 = 0xFF_7A_A2_F7; // blue
const C_BORDER_IN:  u32 = 0xFF_29_2E_42;
const C_TAG_ACT:    u32 = 0xFF_7A_A2_F7;
const C_TAG_OCC:    u32 = 0xFF_56_5F_89;
const C_TAG_IN:     u32 = 0xFF_2A_2B_3D;

// ── Constants ────────────────────────────────────────────────────────────────

const MAX_CLIENTS: usize = 16;
const TAG_CNT:     usize = 9;
const BAR_H:       u32   = 22;
const BORDER:      u32   = 2;
const GAP:         i32   = 6;
const TAG_W:       u32   = 24;

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
    alive:       bool,
    pid:         u32,
    surface_id:  u32,   // kernel surface ID
    tags:        u32,
    x: i32, y: i32, w: u32, h: u32,
    floating:    bool,
    fullscreen:  bool,
    title:       [u8; 20],
    title_len:   u8,
    /// Surface has pending content (app sent SurfaceCommit since last frame).
    dirty:       bool,
}

impl Client {
    const fn empty() -> Self {
        Client {
            alive: false, pid: 0, surface_id: 0,
            tags: 1, x: 0, y: 0, w: 0, h: 0,
            floating: false, fullscreen: false,
            title: [0u8; 20], title_len: 0,
            dirty: false,
        }
    }
}

// ── Display Server state ─────────────────────────────────────────────────────

struct Ds {
    clients:      [Client; MAX_CLIENTS],
    n:            usize,
    focused:      usize,
    tagset:       [u32; 2],
    sel_tag:      usize,
    layout:       Layout,
    nmaster:      usize,
    mfact:        u32,   // master width % (5-95)
    gaps_on:      bool,
    screen_w:     u32,
    screen_h:     u32,
    my_pid:       u32,
    mod_pressed:  bool,
    seq:          u16,
}

impl Ds {
    fn new(w: u32, h: u32, pid: u32) -> Self {
        Ds {
            clients: [Client::empty(); MAX_CLIENTS],
            n: 0,
            focused: 0,
            tagset: [1, 1],
            sel_tag: 0,
            layout: Layout::Tile,
            nmaster: 1,
            mfact: 55,
            gaps_on: true,
            screen_w: w,
            screen_h: h,
            my_pid: pid,
            mod_pressed: false,
            seq: 0,
        }
    }

    fn cur_tags(&self) -> u32 { self.tagset[self.sel_tag] }
    fn work_h(&self) -> u32 { self.screen_h.saturating_sub(BAR_H) }

    // ── Client management ────────────────────────────────────────────────────

    fn add_client(&mut self, pid: u32, surface_id: u32, title: &[u8]) -> Option<usize> {
        for i in 0..MAX_CLIENTS {
            if !self.clients[i].alive {
                let c = &mut self.clients[i];
                c.alive      = true;
                c.pid        = pid;
                c.surface_id = surface_id;
                c.tags       = self.cur_tags();
                c.dirty      = false;
                let n = title.len().min(20);
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
                // Release kernel surface.
                let sid = self.clients[i].surface_id;
                if sid != 0 {
                    sys_surface_destroy(sid);
                }
                self.clients[i] = Client::empty();
                self.n = self.n.saturating_sub(1);
                if self.focused == i && self.n > 0 {
                    self.focused = self.next_visible(i);
                }
                return;
            }
        }
    }

    fn next_visible(&self, from: usize) -> usize {
        let tags = self.cur_tags();
        for delta in 1..=MAX_CLIENTS {
            let i = (from + delta) % MAX_CLIENTS;
            if self.clients[i].alive && (self.clients[i].tags & tags) != 0 {
                return i;
            }
        }
        from
    }

    fn prev_visible(&self, from: usize) -> usize {
        let tags = self.cur_tags();
        for delta in 1..=MAX_CLIENTS {
            let i = (from + MAX_CLIENTS - delta) % MAX_CLIENTS;
            if self.clients[i].alive && (self.clients[i].tags & tags) != 0 {
                return i;
            }
        }
        from
    }

    fn visible_count(&self) -> usize {
        let tags = self.cur_tags();
        self.clients.iter().filter(|c| c.alive && (c.tags & tags) != 0 && !c.floating).count()
    }

    // ── Layout computation ───────────────────────────────────────────────────

    fn arrange(&mut self) {
        let sw = self.screen_w as i32;
        let wh = self.work_h() as i32;
        let woffset_y = BAR_H as i32;
        let g  = if self.gaps_on { GAP } else { 0 };
        let b  = BORDER as i32;

        let visible: [usize; MAX_CLIENTS] = {
            let mut arr = [0usize; MAX_CLIENTS];
            let mut k   = 0;
            let tags    = self.cur_tags();
            for i in 0..MAX_CLIENTS {
                let c = &self.clients[i];
                if c.alive && (c.tags & tags) != 0 && !c.floating && !c.fullscreen {
                    arr[k] = i;
                    k += 1;
                }
            }
            arr
        };
        let n = self.visible_count();
        if n == 0 { return; }

        match self.layout {
            Layout::Tile => self.arrange_tile(&visible, n, sw, wh, woffset_y, g, b),
            Layout::Monocle => {
                for k in 0..n {
                    let i = visible[k];
                    let c = &mut self.clients[i];
                    c.x = g; c.y = woffset_y + g;
                    c.w = (sw - 2*g - 2*b).max(1) as u32;
                    c.h = (wh - 2*g - 2*b).max(1) as u32;
                }
            }
            Layout::Grid => self.arrange_grid(&visible, n, sw, wh, woffset_y, g, b),
            Layout::BStack => self.arrange_bstack(&visible, n, sw, wh, woffset_y, g, b),
            Layout::Spiral => self.arrange_spiral(&visible, n, sw, wh, woffset_y, g, b, false),
            Layout::Dwindle => self.arrange_spiral(&visible, n, sw, wh, woffset_y, g, b, true),
            Layout::CenteredMaster => self.arrange_centered(&visible, n, sw, wh, woffset_y, g, b),
        }
    }

    fn arrange_tile(&mut self, vis: &[usize], n: usize, sw: i32, wh: i32, oy: i32, g: i32, b: i32) {
        let nm = self.nmaster.min(n);
        let mfact = self.mfact as i32;
        let master_w = if nm == n { sw - 2*g } else { (sw - 2*g) * mfact / 100 };
        let stack_w  = sw - 2*g - master_w - g;
        for k in 0..n {
            let i = vis[k];
            let c = &mut self.clients[i];
            if k < nm {
                let slot_h = (wh - 2*g - (nm as i32 - 1)*g) / nm as i32;
                c.x = g; c.y = oy + g + k as i32 * (slot_h + g);
                c.w = (master_w - 2*b).max(1) as u32;
                c.h = (slot_h - 2*b).max(1) as u32;
            } else {
                let si = k - nm;
                let sc = n - nm;
                let slot_h = (wh - 2*g - (sc as i32 - 1)*g) / sc as i32;
                c.x = g + master_w + g; c.y = oy + g + si as i32 * (slot_h + g);
                c.w = (stack_w - 2*b).max(1) as u32;
                c.h = (slot_h - 2*b).max(1) as u32;
            }
        }
    }

    fn arrange_grid(&mut self, vis: &[usize], n: usize, sw: i32, wh: i32, oy: i32, g: i32, b: i32) {
        let cols = {
            let mut c = 1;
            while c * c < n { c += 1; }
            c
        } as i32;
        let rows = (n as i32 + cols - 1) / cols;
        for k in 0..n {
            let i = vis[k];
            let col = k as i32 % cols;
            let row = k as i32 / cols;
            let cw = (sw - 2*g - (cols-1)*g) / cols;
            let rh = (wh - 2*g - (rows-1)*g) / rows;
            let c = &mut self.clients[i];
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
            let i = vis[k];
            let c = &mut self.clients[i];
            if k < nm {
                let slot_w = (sw - 2*g - (nm as i32 - 1)*g) / nm as i32;
                c.x = g + k as i32*(slot_w+g); c.y = oy + g;
                c.w = (slot_w - 2*b).max(1) as u32;
                c.h = (master_h - 2*b).max(1) as u32;
            } else {
                let si = k - nm;
                let slot_w = if sc > 0 { (sw - 2*g - (sc as i32 - 1)*g) / sc as i32 } else { sw };
                c.x = g + si as i32*(slot_w+g); c.y = oy + g + master_h + g;
                c.w = (slot_w - 2*b).max(1) as u32;
                c.h = (stack_h - 2*b).max(1) as u32;
            }
        }
    }

    fn arrange_spiral(&mut self, vis: &[usize], n: usize, sw: i32, wh: i32, oy: i32, g: i32, b: i32, dwindle: bool) {
        let mut rx = g; let mut ry = oy + g;
        let mut rw = sw - 2*g; let mut rh = wh - 2*g;
        for k in 0..n {
            let i = vis[k];
            let c = &mut self.clients[i];
            let last = k + 1 == n;
            if last {
                c.x = rx; c.y = ry;
                c.w = (rw - 2*b).max(1) as u32;
                c.h = (rh - 2*b).max(1) as u32;
                break;
            }
            // Alternating split direction.
            let even = if dwindle { k % 2 == 0 } else { k % 2 == 0 };
            if even {
                let half = rw / 2;
                c.x = rx; c.y = ry;
                c.w = (half - g/2 - 2*b).max(1) as u32;
                c.h = (rh - 2*b).max(1) as u32;
                rx += half + g/2;
                rw -= half + g/2;
            } else {
                let half = rh / 2;
                c.x = rx; c.y = ry;
                c.w = (rw - 2*b).max(1) as u32;
                c.h = (half - g/2 - 2*b).max(1) as u32;
                ry += half + g/2;
                rh -= half + g/2;
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
        let mut li = 0;
        let mut ri = 0;
        for k in 0..n {
            let i = vis[k];
            let c = &mut self.clients[i];
            if k < nm {
                let slot_h = (wh - 2*g - (nm as i32 - 1)*g) / nm as i32;
                c.x = master_x; c.y = oy + g + k as i32*(slot_h+g);
                c.w = (master_w - 2*b).max(1) as u32;
                c.h = (slot_h - 2*b).max(1) as u32;
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

    /// Send the current geometry to a client via IPC.
    fn send_geometry(&mut self, idx: usize) {
        let c = &self.clients[idx];
        if !c.alive { return; }
        let msg = make_msg(
            RwmType::Geometry as u8,
            self.my_pid,
            self.seq,
            RwmPayload {
                geometry: PayloadGeometry {
                    x: c.x, y: c.y, w: c.w, h: c.h,
                    _pad: [0; 40],
                },
            },
        );
        self.seq = self.seq.wrapping_add(1);
        sys_ipc_send(c.pid, &msg, 0);
    }

    /// Send surface assignment to a client.
    fn send_surface_assign(&mut self, idx: usize) {
        let c = &self.clients[idx];
        if !c.alive { return; }
        let msg = make_msg(
            RwmType::SurfaceAssign as u8,
            self.my_pid,
            self.seq,
            RwmPayload {
                surface_assign: PayloadSurfaceAssign {
                    surface_id: c.surface_id,
                    _pad: [0; 52],
                },
            },
        );
        self.seq = self.seq.wrapping_add(1);
        sys_ipc_send(c.pid, &msg, 0);
    }

    /// Draw the status bar.
    fn draw_bar(&self) {
        let sw = self.screen_w;
        let h  = BAR_H;
        let sh = self.screen_h;

        // Bar background.
        sys_fb_fill_rect(0, sh - h, sw, h, C_BAR_BG);

        // Tag strip.
        let tags = self.cur_tags();
        for t in 0..TAG_CNT {
            let tag_bit = 1u32 << t;
            let has_win = self.clients.iter().any(|c| c.alive && (c.tags & tag_bit) != 0);
            let col = if (tags & tag_bit) != 0 {
                C_TAG_ACT
            } else if has_win {
                C_TAG_OCC
            } else {
                C_TAG_IN
            };
            let x = t as u32 * TAG_W;
            sys_fb_fill_rect(x, sh - h, TAG_W, h, col);
            // Tag number glyph — we write a tiny coloured square as a marker.
            sys_fb_fill_rect(x + TAG_W/2 - 2, sh - h + h/2 - 2, 4, 4,
                if (tags & tag_bit) != 0 { 0xFF_FF_FF_FF } else { 0xFF_60_60_80 });
        }

        // Layout symbol.
        let sym_x = TAG_CNT as u32 * TAG_W + 4;
        let sym = self.layout.symbol();
        // Draw coloured marker for layout (no font renderer — use coloured dot per char).
        for (si, _byte) in sym.iter().enumerate() {
            sys_fb_fill_rect(sym_x + si as u32 * 5, sh - h + 6, 4, 10, 0xFF_BB_9A_F7);
        }

        // Active window title bar — draw small accent strip.
        if let Some(c) = self.clients.get(self.focused) {
            if c.alive {
                let title_x = sym_x + 24;
                let title_w = sw.saturating_sub(title_x + 4);
                sys_fb_fill_rect(title_x, sh - h, 2, h, C_TAG_ACT);
                // Title glyph hint (dot per char).
                for i in 0..(c.title_len as u32).min(title_w / 6) {
                    sys_fb_fill_rect(title_x + 6 + i*6, sh - h + 8, 4, 6, 0xFF_C0_CA_F5);
                }
            }
        }
    }

    /// Draw all window decorations (borders).
    fn draw_decorations(&self) {
        let tags = self.cur_tags();
        for (i, c) in self.clients.iter().enumerate() {
            if !c.alive || (c.tags & tags) == 0 { continue; }
            let col = if i == self.focused { C_BORDER_ACT } else { C_BORDER_IN };
            let b = BORDER;
            // Top / bottom / left / right borders.
            sys_fb_fill_rect(c.x as u32, c.y as u32, c.w + 2*b, b, col);
            sys_fb_fill_rect(c.x as u32, (c.y as u32 + c.h + b), c.w + 2*b, b, col);
            sys_fb_fill_rect(c.x as u32, c.y as u32, b, c.h + 2*b, col);
            sys_fb_fill_rect((c.x as u32 + c.w + b), c.y as u32, b, c.h + 2*b, col);
        }
    }

    /// Composite one frame: blit all committed surfaces, draw decorations + bar.
    fn composite_frame(&mut self) {
        // 1. Clear background.
        sys_fb_fill_rect(0, 0, self.screen_w, self.screen_h, C_BG);

        // 2. Commit all dirty surfaces in z-order (lower index = lower z for now).
        let tags = self.cur_tags();
        for i in 0..MAX_CLIENTS {
            let c = &mut self.clients[i];
            if !c.alive || (c.tags & tags) == 0 { continue; }
            if c.surface_id != 0 {
                // Commit whatever buffer the app last attached.
                sys_surface_commit(c.surface_id, c.x as u32, c.y as u32);
            } else {
                // Fallback: solid fill for apps that haven't attached a buffer yet.
                let col = if i == self.focused { C_WIN_ACT } else { C_WIN_BG };
                sys_fb_fill_rect(c.x as u32, c.y as u32, c.w, c.h, col);
            }
        }

        // 3. Decorations on top of app content.
        self.draw_decorations();

        // 4. Status bar.
        self.draw_bar();

        sys_fb_flush();
    }

    // ── Input / event handling ───────────────────────────────────────────────

    fn focus_event(&mut self, idx: usize) {
        // Send Unfocus to previously focused.
        if self.focused != idx {
            let old = self.focused;
            if self.clients[old].alive {
                let msg = make_msg(
                    RwmType::EventFocus as u8,
                    self.my_pid,
                    self.seq,
                    RwmPayload { event_focus: PayloadEventFocus { focused: 0, _pad: [0;55] } },
                );
                self.seq = self.seq.wrapping_add(1);
                sys_ipc_send(self.clients[old].pid, &msg, 0);
            }
        }
        self.focused = idx;
        // Send Focus to new target.
        if self.clients[idx].alive {
            let msg = make_msg(
                RwmType::EventFocus as u8,
                self.my_pid,
                self.seq,
                RwmPayload { event_focus: PayloadEventFocus { focused: 1, _pad: [0;55] } },
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
            RwmType::EventKey as u8,
            self.my_pid,
            self.seq,
            RwmPayload {
                event_key: PayloadEventKey {
                    keycode,
                    pressed: pressed as u8,
                    _pad: [0; 54],
                },
            },
        );
        self.seq = self.seq.wrapping_add(1);
        sys_ipc_send(pid, &msg, 0);
    }

    fn handle_shortcut(&mut self, keycode: u8, pressed: bool) -> bool {
        if !pressed { return false; }
        match keycode {
            KEY_1..=KEY_9 => {
                let t = (keycode - KEY_1) as usize;
                self.tagset[self.sel_tag] = 1 << t;
                self.arrange();
                return true;
            }
            KEY_0 => {
                self.tagset[self.sel_tag] = (1 << TAG_CNT) - 1;
                self.arrange();
                return true;
            }
            KEY_J => {
                let nxt = self.next_visible(self.focused);
                self.focus_event(nxt);
                return true;
            }
            KEY_K => {
                let prv = self.prev_visible(self.focused);
                self.focus_event(prv);
                return true;
            }
            KEY_SPACE => {
                self.layout = self.layout.next();
                self.arrange();
                return true;
            }
            KEY_G => {
                self.gaps_on = !self.gaps_on;
                self.arrange();
                return true;
            }
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
            KEY_COMMA => {
                if self.nmaster > 0 { self.nmaster -= 1; self.arrange(); }
                return true;
            }
            KEY_PERIOD => {
                self.nmaster += 1;
                self.arrange();
                return true;
            }
            _ => {}
        }
        false
    }
}

// ── IPC message handling ─────────────────────────────────────────────────────

fn handle_ipc(ds: &mut Ds) -> bool {
    let mut msg = RwmMsg::ZERO;
    let r = sys_ipc_recv(&mut msg, IPC_NONBLOCK);
    if r == SYSERR_AGAIN as isize || r < 0 {
        return false;
    }
    let t = msg.msg_type;
    if t == RwmType::Register as u8 {
        // New client wants to connect.
        let pid = msg.sender_pid;
        let title = unsafe { &msg.payload.register.title };
        let title_len = title.iter().position(|&b| b == 0).unwrap_or(title.len());

        // Allocate a kernel surface.
        let sid_raw = sys_surface_create();
        let sid = if sid_raw > 0 { sid_raw as u32 } else { 0 };

        if let Some(idx) = ds.add_client(pid, sid, &title[..title_len]) {
            ds.arrange();
            ds.send_geometry(idx);
            if sid != 0 {
                ds.send_surface_assign(idx);
            }
            ds.composite_frame();
        }
        return true;
    }
    if t == RwmType::Unregister as u8 {
        ds.remove_client(msg.sender_pid);
        ds.arrange();
        ds.composite_frame();
        return true;
    }
    if t == RwmType::SurfaceCommit as u8 {
        let sc = unsafe { msg.payload.surface_commit };
        // Mark client dirty; the surface buffer is already attached by the app.
        for c in ds.clients.iter_mut() {
            if c.alive && c.pid == msg.sender_pid {
                c.dirty = true;
                // Update position from app's commit (in case app self-positioned).
                if sc.x != 0 || sc.y != 0 { c.x = sc.x; c.y = sc.y; }
                break;
            }
        }
        ds.composite_frame();
        return true;
    }
    if t == RwmType::SetTitle as u8 {
        let title = unsafe { &msg.payload.set_title.title };
        let n = title.iter().position(|&b| b == 0).unwrap_or(title.len()).min(20);
        for c in ds.clients.iter_mut() {
            if c.alive && c.pid == msg.sender_pid {
                c.title[..n].copy_from_slice(&title[..n]);
                c.title_len = n as u8;
                break;
            }
        }
        return true;
    }
    false
}

// ── Helper: build a RwmMsg ────────────────────────────────────────────────────

fn make_msg(msg_type: u8, sender_pid: u32, seq: u16, payload: RwmPayload) -> RwmMsg {
    RwmMsg { msg_type, flags: 0, seq, sender_pid, payload }
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[no_mangle]
fn _start() -> ! {
    log(b"[rogue_ds] starting\r\n");

    let mut sw: u32 = 1920;
    let mut sh: u32 = 1080;
    sys_screen_size(&mut sw, &mut sh);
    let my_pid = sys_getpid();

    log(b"[rogue_ds] screen_size ok\r\n");

    let mut ds = Ds::new(sw, sh, my_pid);

    // Initial blank frame.
    ds.composite_frame();

    let mut ev  = libs::KeyEvent  { keycode: 0, pressed: false };
    let mut mev = libs::MouseEvent { dx: 0, dy: 0, buttons: 0 };

    log(b"[rogue_ds] event loop\r\n");

    loop {
        // Drain all pending IPC messages first.
        while handle_ipc(&mut ds) {}

        // Keyboard.
        let n = sys_poll_input(&mut ev);
        if n > 0 {
            let pressed = ev.pressed;
            let kc = ev.keycode;
            if kc == KEY_LSUPER || kc == KEY_RSUPER {
                ds.mod_pressed = pressed;
            } else if ds.mod_pressed {
                if !ds.handle_shortcut(kc, pressed) {
                    // Not a WM shortcut — forward to focused app.
                    ds.forward_key(kc, pressed);
                }
                ds.composite_frame();
            } else {
                ds.forward_key(kc, pressed);
            }
        }

        // Mouse (consumed for now; future: move floating windows).
        sys_poll_mouse(&mut mev);
    }
}

fn log(msg: &[u8]) {
    let _ = sys_write(1, msg.as_ptr(), msg.len());
}
