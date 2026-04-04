//! RogueWM — Kingdom's window manager powered by rwm-core.
//!
//! Runs bare-metal (no_std + alloc via userland bump allocator).
//! Uses rwm-core for window/tag/layout state and Kingdom framebuffer syscalls for rendering.
//!
//! Boot sequence:
//!   kernel_main → init → sys_spawn(1) → rwm (_start)
//!
//! Keybindings (Super = KEY_MOD):
//!   Mod+1..9         — view tag N
//!   Mod+Shift+1..9   — move focused client to tag N
//!   Mod+j            — focus next in stack
//!   Mod+k            — focus prev in stack
//!   Mod+h            — decrease master factor
//!   Mod+l            — increase master factor
//!   Mod+t            — tile layout
//!   Mod+m            — monocle layout
//!   Mod+f            — spiral layout
//!   Mod+g            — grid layout
//!   Mod+Enter        — spawn shell (program 0)
//!   Mod+Shift+q      — quit

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use libs::keycodes::*;
use rwm_core::{
    client::Client,
    layout::{builtin_layouts, Arrangement},
    monitor::Monitor,
    state::WmState,
    Rect,
};
use userland::{
    sys_exit, sys_poll_input, sys_screen_size, sys_spawn, sys_write,
};
use userland::backend_kernel::KernelBackend;
use userland_compositor::{Compositor, WindowRect};
use userland_core::{Config, DisplayBackend};
use userland_server::Server;

// ── Theme (rogue-website palette) ────────────────────────────────────────────

const COLOR_VOID:       u32 = 0xFF_05_05_05; // background
const COLOR_PANEL:      u32 = 0xFF_11_11_11; // bar bg
const COLOR_ROGUE_RED:  u32 = 0xFF_FF_00_00; // active accent
const COLOR_BORDER_NRM: u32 = 0xFF_11_11_11; // unfocused window border
const COLOR_BORDER_SEL: u32 = 0xFF_FF_00_00; // focused window border
const COLOR_WIN_SEL:    u32 = 0xFF_22_22_28; // focused window fill

const BAR_H: u32 = 20;
const TAG_W: u32 = 18;
const TAG_PAD: u32 = 2;
const BORDER: u32 = 2;
const MFACT_STEP: f32 = 0.05;

// Virtual "window" slots — label colors for placeholder content
const CLIENT_COLORS: [u32; 9] = [
    0xFF_1A_1A_2E, // terminal — deep navy
    0xFF_16_21_3E, // editor   — dark blue
    0xFF_0F_3460, // browser  — navy blue
    0xFF_1B_1B_2F, // security — dark purple
    0xFF_16_1C_2E, // monitor  — dark slate
    0xFF_1E_1E_24, // docs     — dark grey-blue
    0xFF_1A_1A_1A, // comms    — near-black
    0xFF_0D_0D_0D, // media    — black
    0xFF_1E_1E_1E, // config   — dark grey
];

fn log(msg: &[u8]) {
    let _ = sys_write(1, msg.as_ptr(), msg.len());
}

// ── Rendering ────────────────────────────────────────────────────────────────

/// Draw the status bar at y=0..BAR_H.
fn draw_bar(backend: &mut KernelBackend, state: &WmState, sw: u32) {
    // Bar background
    backend.fill_rect(0, 0, sw, BAR_H, COLOR_PANEL);

    let mon = &state.monitors[0];
    let cur_tags = mon.current_tags();

    // Tag buttons
    for i in 0u32..9 {
        let tag_bit = 1u32 << i;
        let active = (cur_tags & tag_bit) != 0;
        let occupied = state.clients_on_tag(0, tag_bit) > 0;
        let x = TAG_PAD + i * (TAG_W + TAG_PAD);
        let y = (BAR_H - (BAR_H - 4)) / 2;
        let color = if active {
            COLOR_ROGUE_RED
        } else if occupied {
            0xFF_33_33_33
        } else {
            0xFF_1A_1A_1A
        };
        backend.fill_rect(x, y, TAG_W, BAR_H - 4, color);
    }

    // Layout symbol: a small rectangle whose shape hints the layout
    let sym_x = 9 * (TAG_W + TAG_PAD) + TAG_PAD + 4;
    backend.fill_rect(sym_x, 4, 24, BAR_H - 8, 0xFF_2A_2A_2A);

    // Divider
    backend.fill_rect(sym_x + 26, 4, 1, BAR_H - 8, 0xFF_33_33_33);
}

/// Convert rwm-core Rect → WindowRect for the compositor.
fn to_window_rect(r: Rect, focused: bool, client_color: u32) -> WindowRect {
    let fill = if focused { COLOR_WIN_SEL } else { client_color };
    let border = if focused { COLOR_BORDER_SEL } else { COLOR_BORDER_NRM };
    WindowRect {
        x: r.x.max(0) as u32,
        y: r.y.max(0) as u32,
        w: r.w,
        h: r.h,
        fill_color: fill,
        border_color: border,
        border_px: BORDER,
    }
}

/// Full repaint: background → bar → tiled windows.
fn redraw(
    server: &mut Server<KernelBackend>,
    compositor: &Compositor,
    state: &WmState,
    sw: u32,
) {
    let backend = server.backend_mut();

    // Background
    backend.clear(COLOR_VOID);

    // Compute layout for visible tiled clients on monitor 0.
    let visible: Vec<(rwm_core::client::ClientId, &Client)> = state.visible_tiled(0);
    let area = state.monitors[0].window_area;

    let arrangement: Arrangement = if let Some(layout) = state.current_layout(0) {
        layout.arrange(&state.monitors[0], &visible, area)
    } else {
        visible.iter().map(|&(cid, _)| (cid, area)).collect()
    };

    // Draw windows bottom-up (focused last so it's on top).
    let focused_cid = state.monitors[0].focused;
    let mut rects: [WindowRect; 16] = [WindowRect {
        x: 0, y: 0, w: 0, h: 0,
        fill_color: 0, border_color: 0, border_px: 0,
    }; 16];
    let mut n = 0usize;

    for (cid, geom) in &arrangement {
        if n >= 16 { break; }
        let focused = Some(*cid) == focused_cid;
        // Use client win id as color index.
        let win = state.clients.get(*cid).map(|c| c.win as usize).unwrap_or(0);
        let color = CLIENT_COLORS[win % CLIENT_COLORS.len()];
        rects[n] = to_window_rect(*geom, focused, color);
        n += 1;
    }

    // Composite the windows (clear already done above).
    // Re-use the compositor just for rounded-corner windows.
    let r = compositor.corner_radius;
    for w in &rects[..n] {
        if w.w > 0 && w.h > 0 {
            backend.fill_rect_rounded(w.x, w.y, w.w, w.h, r, w.fill_color);
            if w.border_px > 0 && w.w > 2 * w.border_px && w.h > 2 * w.border_px {
                backend.fill_rect(w.x, w.y, w.w, w.border_px, w.border_color);
                backend.fill_rect(w.x, w.y + w.h - w.border_px, w.w, w.border_px, w.border_color);
                backend.fill_rect(w.x, w.y, w.border_px, w.h, w.border_color);
                backend.fill_rect(w.x + w.w - w.border_px, w.y, w.border_px, w.h, w.border_color);
            }
        }
    }

    // Draw bar on top of everything.
    draw_bar(backend, state, sw);

    backend.flush();
}

// ── WM action helpers ─────────────────────────────────────────────────────────

fn focus_stack(state: &mut WmState, dir: i32) {
    let mon = &state.monitors[0];
    let visible: Vec<_> = {
        let tags = mon.current_tags();
        mon.clients
            .iter()
            .filter_map(|&cid| {
                let c = state.clients.get(cid)?;
                if rwm_core::is_visible(c.tags, tags) { Some(cid) } else { None }
            })
            .collect()
    };
    if visible.is_empty() { return; }
    let current = state.monitors[0].focused;
    let pos = current
        .and_then(|cur| visible.iter().position(|&id| id == cur))
        .unwrap_or(0);
    let next = if dir > 0 {
        (pos + 1) % visible.len()
    } else {
        (pos + visible.len() - 1) % visible.len()
    };
    let next_cid = visible[next];
    state.monitors[0].focused = Some(next_cid);
    state.monitors[0].raise_in_stack(next_cid);
}

fn view_tag(state: &mut WmState, tag: u32) {
    let tag_bit = 1u32 << (tag.saturating_sub(1));
    state.view_tags(0, tag_bit);
    // If no focused client is visible on new tag, focus first visible.
    let tags = state.monitors[0].current_tags();
    let first_visible = state.monitors[0].clients.iter().find(|&&cid| {
        state.clients.get(cid).map(|c| rwm_core::is_visible(c.tags, tags)).unwrap_or(false)
    }).copied();
    state.monitors[0].focused = first_visible;
}

fn move_to_tag(state: &mut WmState, tag: u32) {
    let tag_bit = 1u32 << (tag.saturating_sub(1));
    if let Some(cid) = state.monitors[0].focused {
        state.tag_client(cid, tag_bit);
        // Refocus
        let tags = state.monitors[0].current_tags();
        let first = state.monitors[0].clients.iter().find(|&&id| {
            state.clients.get(id).map(|c| rwm_core::is_visible(c.tags, tags)).unwrap_or(false)
        }).copied();
        state.monitors[0].focused = first;
    }
}

fn set_layout(state: &mut WmState, name: &str) {
    if let Some(idx) = state.layouts.iter().position(|l| l.name() == name) {
        let mon = &mut state.monitors[0];
        mon.layout[mon.sel_layout] = rwm_core::layout::LayoutId(idx);
        mon.layout_symbol = alloc::string::String::from(state.layouts[idx].symbol());
    }
}

fn adjust_mfact(state: &mut WmState, delta: f32) {
    let mon = &mut state.monitors[0];
    let new = (mon.mfact + delta).clamp(0.1, 0.9);
    mon.mfact = new;
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[no_mangle]
fn _start() -> ! {
    log(b"[rwm] starting roguewm\r\n");

    // Query screen dimensions.
    let mut sw = 1280u32;
    let mut sh = 800u32;
    let _ = sys_screen_size(&mut sw, &mut sh);

    // Set up display backend + compositor.
    let backend = KernelBackend::new();
    let mut server = Server::new(backend);
    let config = Config::default();
    let compositor = Compositor::new(&config);

    // Build WM state.
    let mut state = WmState::new();
    state.screen_w = sw;
    state.screen_h = sh;
    state.bar_height = BAR_H;

    // Register all built-in layouts.
    state.layouts = builtin_layouts(true /* smart_gaps */);

    // Initialise one monitor covering the full screen minus bar.
    let mon_geom = Rect::new(0, 0, sw, sh);
    let mut mon = Monitor::new(0, mon_geom);
    mon.update_bar_pos(BAR_H);
    mon.mfact = 0.55;
    mon.nmaster = 1;
    state.monitors.push(mon);
    state.sel_mon = 0;

    // Pre-populate 9 virtual clients — one per tag — so the WM has something to
    // manage immediately. Each client is pinned to its matching tag.
    for i in 0u32..9 {
        let tag_bit = 1u32 << i;
        let mut c = Client::new(i as u32, Rect::new(0, 0, 100, 100), 0);
        c.tags = tag_bit;
        c.border_width = BORDER;
        let cid = state.add_client(c, 0);
        if i == 0 {
            state.monitors[0].focused = Some(cid);
        }
    }

    // Initial draw.
    redraw(&mut server, &compositor, &state, sw);
    log(b"[rwm] initial draw done\r\n");

    // ── Event loop ────────────────────────────────────────────────────────────
    let mut ev = libs::KeyEvent { keycode: 0, pressed: false };
    let mut mod_held = false;
    let mut shift_held = false;
    let mut needs_redraw = false;

    loop {
        let n = sys_poll_input(&mut ev);
        if n <= 0 {
            if needs_redraw {
                redraw(&mut server, &compositor, &state, sw);
                needs_redraw = false;
            }
            continue;
        }

        let kc = ev.keycode;
        let pressed = ev.pressed;

        // Track modifier state.
        if kc == KEY_MOD   { mod_held   = pressed; continue; }
        if kc == KEY_SHIFT { shift_held = pressed; continue; }
        if !pressed { continue; }

        if mod_held {
            match kc {
                // ── Tag switching ────────────────────────────────
                KEY_1 if !shift_held => { view_tag(&mut state, 1); needs_redraw = true; }
                KEY_2 if !shift_held => { view_tag(&mut state, 2); needs_redraw = true; }
                KEY_3 if !shift_held => { view_tag(&mut state, 3); needs_redraw = true; }
                KEY_4 if !shift_held => { view_tag(&mut state, 4); needs_redraw = true; }
                KEY_5 if !shift_held => { view_tag(&mut state, 5); needs_redraw = true; }
                KEY_6 if !shift_held => { view_tag(&mut state, 6); needs_redraw = true; }
                KEY_7 if !shift_held => { view_tag(&mut state, 7); needs_redraw = true; }
                KEY_8 if !shift_held => { view_tag(&mut state, 8); needs_redraw = true; }
                KEY_9 if !shift_held => { view_tag(&mut state, 9); needs_redraw = true; }

                // ── Move client to tag ───────────────────────────
                KEY_1 if shift_held => { move_to_tag(&mut state, 1); needs_redraw = true; }
                KEY_2 if shift_held => { move_to_tag(&mut state, 2); needs_redraw = true; }
                KEY_3 if shift_held => { move_to_tag(&mut state, 3); needs_redraw = true; }
                KEY_4 if shift_held => { move_to_tag(&mut state, 4); needs_redraw = true; }
                KEY_5 if shift_held => { move_to_tag(&mut state, 5); needs_redraw = true; }
                KEY_6 if shift_held => { move_to_tag(&mut state, 6); needs_redraw = true; }
                KEY_7 if shift_held => { move_to_tag(&mut state, 7); needs_redraw = true; }
                KEY_8 if shift_held => { move_to_tag(&mut state, 8); needs_redraw = true; }
                KEY_9 if shift_held => { move_to_tag(&mut state, 9); needs_redraw = true; }

                // ── Focus ────────────────────────────────────────
                KEY_J => { focus_stack(&mut state, 1);  needs_redraw = true; }
                KEY_K => { focus_stack(&mut state, -1); needs_redraw = true; }

                // ── Master factor ─────────────────────────────────
                KEY_H => { adjust_mfact(&mut state, -MFACT_STEP); needs_redraw = true; }
                KEY_L => { adjust_mfact(&mut state,  MFACT_STEP); needs_redraw = true; }

                // ── Layouts ──────────────────────────────────────
                KEY_T => { set_layout(&mut state, "tile");          needs_redraw = true; }
                KEY_M => { set_layout(&mut state, "monocle");       needs_redraw = true; }
                KEY_F => { set_layout(&mut state, "spiral");        needs_redraw = true; }
                KEY_G => { set_layout(&mut state, "grid");          needs_redraw = true; }
                KEY_B => { set_layout(&mut state, "bstack");        needs_redraw = true; }
                KEY_C => { set_layout(&mut state, "centeredmaster"); needs_redraw = true; }

                // ── Spawn shell ───────────────────────────────────
                KEY_ENTER => {
                    let _ = sys_spawn(0); // program 0 = shell
                    log(b"[rwm] spawned shell\r\n");
                }

                // ── Quit ─────────────────────────────────────────
                KEY_Q if shift_held => {
                    log(b"[rwm] quit\r\n");
                    sys_exit(0);
                }

                _ => {}
            }
        }

        if needs_redraw {
            redraw(&mut server, &compositor, &state, sw);
            needs_redraw = false;
        }
    }
}
