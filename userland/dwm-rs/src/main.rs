//! dwm-rs — Direct Rust port of dwm (dynamic window manager).
//!
//! Minimal port: single monitor, tile layout, MapRequest/ConfigureRequest/DestroyNotify,
//! Mod4+Shift+Q quit. Uses x11rb (no Xlib). Reference/parity with C dwm.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

const BORDER_WIDTH: u32 = 2;
const BORDER_FOCUS: u32 = 0xff0000;   // red
const BORDER_NORMAL: u32 = 0x333333;
const XK_Q: u32 = 0x0051;
const MOD4: u16 = 0x40;
const SHIFT: u16 = 0x01;

struct Client {
    win: Window,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    let sw = screen.width_in_pixels as i32;
    let sh = screen.height_in_pixels as i32;

    // Select for WM
    let mask = EventMask::SUBSTRUCTURE_REDIRECT
        | EventMask::SUBSTRUCTURE_NOTIFY
        | EventMask::BUTTON_PRESS
        | EventMask::STRUCTURE_NOTIFY
        | EventMask::PROPERTY_CHANGE;
    conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().event_mask(mask),
    )?;
    conn.flush()?;

    // EWMH: _NET_SUPPORTING_WM_CHECK
    let net_wm_check = conn.intern_atom(false, b"_NET_SUPPORTING_WM_CHECK")?.reply()?.atom;
    let utf8 = conn.intern_atom(false, b"UTF8_STRING")?.reply()?.atom;
    let check_win = conn.generate_id()?;
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        check_win,
        root,
        0, 0, 1, 1, 0,
        WindowClass::INPUT_OUTPUT,
        x11rb::COPY_FROM_PARENT,
        &CreateWindowAux::new().override_redirect(1),
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        root,
        net_wm_check,
        AtomEnum::WINDOW,
        &[check_win],
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        check_win,
        net_wm_check,
        AtomEnum::WINDOW,
        &[check_win],
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        check_win,
        conn.intern_atom(false, b"_NET_WM_NAME")?.reply()?.atom,
        utf8,
        b"dwm-rs",
    )?;

    // Grab Mod4+Shift+Q to quit
    let setup = conn.setup();
    let min_kc = setup.min_keycode;
    let max_kc = setup.max_keycode;
    let mapping = conn.get_keyboard_mapping(min_kc, max_kc - min_kc + 1)?.reply()?;
    let syms_per_code = mapping.keysyms_per_keycode as usize;
    for kc in min_kc..=max_kc {
        let idx = (kc - min_kc) as usize * syms_per_code;
        if idx < mapping.keysyms.len() && mapping.keysyms[idx] == XK_Q {
            for &extra in &[0u16, 2, 16, 18] {
                    let _ = conn.grab_key(
                        false,
                        root,
                        (MOD4 | SHIFT | extra).into(),
                        kc,
                        GrabMode::ASYNC,
                        GrabMode::ASYNC,
                    );
            }
            break;
        }
    } // Mod4 | Shift, Q

    let mut clients: Vec<Client> = Vec::new();
    let mut sel: Option<usize> = None;
    let mut running = true;
    let min_kc = conn.setup().min_keycode;
    let mapping = conn.get_keyboard_mapping(min_kc, conn.setup().max_keycode - min_kc + 1)?.reply()?;
    let syms_per_code = mapping.keysyms_per_keycode as usize;

    while running {
        let event = conn.wait_for_event()?;
        match event {
            x11rb::protocol::Event::MapRequest(ev) => {
                let win = ev.window;
                if clients.iter().any(|c| c.win == win) {
                    conn.map_window(win)?;
                    continue;
                }
                let attrs = conn.get_window_attributes(win)?.reply()?;
                if attrs.override_redirect {
                    conn.map_window(win)?;
                    continue;
                }
                let geom = conn.get_geometry(win)?.reply()?;
                let c = Client {
                    win,
                    x: geom.x as i32,
                    y: geom.y as i32,
                    w: geom.width as u32,
                    h: geom.height as u32,
                };
                conn.change_window_attributes(
                    win,
                    &ChangeWindowAttributesAux::new()
                        .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
                )?;
                clients.push(c);
                tile(&conn, root, &mut clients, sw, sh)?;
                sel = Some(clients.len() - 1);
                conn.map_window(win)?;
                focus(&conn, root, &clients, sel)?;
            }
            x11rb::protocol::Event::ConfigureRequest(ev) => {
                if let Some(i) = clients.iter().position(|c| c.win == ev.window) {
                    let c = &clients[i];
                    conn.configure_window(
                        ev.window,
                        &ConfigureWindowAux::new()
                            .x(c.x)
                            .y(c.y)
                            .width(c.w)
                            .height(c.h)
                            .border_width(BORDER_WIDTH),
                    )?;
                } else {
                    conn.configure_window(
                        ev.window,
                        &ConfigureWindowAux::new()
                            .x(ev.x as i32)
                            .y(ev.y as i32)
                            .width(ev.width as u32)
                            .height(ev.height as u32)
                            .border_width(ev.border_width as u32),
                    )?;
                }
            }
            x11rb::protocol::Event::DestroyNotify(ev) => {
                if let Some(pos) = clients.iter().position(|c| c.win == ev.window) {
                    clients.remove(pos);
                    if sel.map(|s| s >= clients.len()).unwrap_or(false) {
                        sel = if clients.is_empty() { None } else { Some(clients.len() - 1) };
                    } else if sel.map(|s| s > pos).unwrap_or(false) {
                        sel = sel.map(|s| s - 1);
                    }
                    tile(&conn, root, &mut clients, sw, sh)?;
                    focus(&conn, root, &clients, sel)?;
                }
            }
            x11rb::protocol::Event::KeyPress(ev) => {
                let kc = ev.detail;
                let idx = (kc - min_kc) as usize * syms_per_code;
                let keysym = if idx < mapping.keysyms.len() {
                    mapping.keysyms[idx]
                } else {
                    0
                };
                let state_clean: u16 = u16::from(ev.state) & (MOD4 | SHIFT | 2 | 16);
                if keysym == XK_Q && state_clean == MOD4 | SHIFT {
                    running = false;
                }
            }
            _ => {}
        }
        conn.flush()?;
    }

    // Cleanup
    for c in &clients {
        conn.change_window_attributes(
            c.win,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT),
        )?;
        conn.configure_window(
            c.win,
            &ConfigureWindowAux::new().border_width(0),
        )?;
    }
    conn.ungrab_key(Grab::ANY, root, ModMask::ANY)?;
    conn.destroy_window(check_win)?;
    conn.flush()?;

    Ok(())
}

fn tile(
    conn: &impl Connection,
    _root: Window,
    clients: &mut [Client],
    sw: i32,
    sh: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    let n = clients.len();
    if n == 0 {
        return Ok(());
    }
    let mfact = 0.55f32;
    let n_master = (n as f32 * mfact) as usize;
    let n_master = n_master.min(n).max(1);
    let bw = BORDER_WIDTH as i32;
    let master_w = (sw / n_master as i32).max(1);
    let stack_count = n - n_master;
    let stack_w = if stack_count > 0 {
        (sw / stack_count as i32).max(1)
    } else {
        sw
    };
    for (i, c) in clients.iter_mut().enumerate() {
        if i < n_master {
            c.x = i as i32 * master_w;
            c.y = 0;
            c.w = (master_w - 2 * bw).max(1) as u32;
            c.h = (sh - 2 * bw).max(1) as u32;
        } else {
            let j = i - n_master;
            c.x = j as i32 * stack_w;
            c.y = 0;
            c.w = (stack_w - 2 * bw).max(1) as u32;
            c.h = (sh - 2 * bw).max(1) as u32;
        }
        conn.configure_window(
            c.win,
            &ConfigureWindowAux::new()
                .x(c.x)
                .y(c.y)
                .width(c.w)
                .height(c.h)
                .border_width(BORDER_WIDTH),
        )?;
    }
    Ok(())
}

fn focus(
    conn: &impl Connection,
    _root: Window,
    clients: &[Client],
    sel: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    for (i, c) in clients.iter().enumerate() {
        let border = if sel == Some(i) {
            BORDER_FOCUS
        } else {
            BORDER_NORMAL
        };
        conn.change_window_attributes(
            c.win,
            &ChangeWindowAttributesAux::new().border_pixel(border),
        )?;
    }
    if let Some(i) = sel.and_then(|i| clients.get(i)) {
        conn.set_input_focus(
            InputFocus::POINTER_ROOT,
            i.win,
            x11rb::CURRENT_TIME,
        )?;
    }
    conn.flush()?;
    Ok(())
}

