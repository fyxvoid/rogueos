//! rogue-lock — Futuristic X11 lock screen for Rogue Linux.
//! Fullscreen overlay, clock, unlock indicator, PAM auth. Rogue brand aesthetic.

use cairo::Context;
use chrono::Local;
use std::os::raw::{c_char, c_int, c_void};
use std::time::{Duration, Instant};
use xcb::x;

const VOID_BLACK: (f64, f64, f64) = (2.0 / 255.0, 2.0 / 255.0, 5.0 / 255.0);
const HOLO_BLUE: (f64, f64, f64) = (0.0, 243.0 / 255.0, 1.0);
const WHITE_SMOKE: (f64, f64, f64) = (224.0 / 255.0, 230.0 / 255.0, 237.0 / 255.0);
const MUTED_BLUE: (f64, f64, f64) = (136.0 / 255.0, 146.0 / 255.0, 176.0 / 255.0);
const CYBERPUNK_RED: (f64, f64, f64) = (1.0, 0.0, 60.0 / 255.0);

const FONT: &str = "JetBrains Mono";
const CLOCK_SIZE: f64 = 72.0;
const INDICATOR_RADIUS: f64 = 80.0;
const RING_WIDTH: f64 = 6.0;

fn main() {
    if let Err(e) = run() {
        eprintln!("rogue-lock: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let (conn, screen_num) = xcb::Connection::connect(None).map_err(|e| e.to_string())?;
    let setup = conn.get_setup();
    let screen = setup.roots().nth(screen_num as usize).ok_or("No screen")?;
    let width = screen.width_in_pixels() as u32;
    let height = screen.height_in_pixels() as u32;
    let depth = screen.root_depth();

    let win = conn.generate_id();
    conn.send_request(&x::CreateWindow {
        depth: 0u8,
        wid: win,
        parent: screen.root(),
        x: 0,
        y: 0,
        width: width as u16,
        height: height as u16,
        border_width: 0,
        class: x::WindowClass::InputOutput,
        visual: screen.root_visual(),
        value_list: &[
            x::Cw::OverrideRedirect(true),
            x::Cw::EventMask(
                x::EventMask::KEY_PRESS
                    | x::EventMask::KEY_RELEASE
                    | x::EventMask::EXPOSURE
                    | x::EventMask::STRUCTURE_NOTIFY,
            ),
            x::Cw::BackPixel(screen.black_pixel()),
        ],
    });

    let gc = conn.generate_id();
    conn.send_request(&x::CreateGc {
        cid: gc,
        drawable: x::Drawable::Window(win),
        value_list: &[],
    });

    conn.send_request(&x::MapWindow { window: win });
    let _ = conn.flush();

    let _grab_k = conn.send_request(&x::GrabKeyboard {
        owner_events: false,
        grab_window: win,
        time: x::CURRENT_TIME,
        pointer_mode: x::GrabMode::Async,
        keyboard_mode: x::GrabMode::Async,
    });
    let _ = conn.send_request(&x::GrabPointer {
        owner_events: false,
        grab_window: win,
        event_mask: x::EventMask::empty(),
        pointer_mode: x::GrabMode::Async,
        keyboard_mode: x::GrabMode::Async,
        confine_to: x::WINDOW_NONE,
        cursor: x::CURSOR_NONE,
        time: x::CURRENT_TIME,
    });
    let _ = conn.flush();

    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width as i32, height as i32)
        .map_err(|e| e.to_string())?;
    let cr = cairo::Context::new(&surface).map_err(|e| e.to_string())?;

    let mut password = String::new();
    let mut wrong = false;
    let mut wrong_until = Instant::now();
    let mut authenticated = false;

    while !authenticated {
        draw_frame(&cr, width as f64, height as f64, password.len(), wrong)?;

        let data = surface.data().map_err(|e| e.to_string())?;
        let slice: &[u8] = data.as_ref();
        let mut x11_data = Vec::with_capacity((width * height * 4) as usize);
        for chunk in slice.chunks(4) {
            let (a, r, g, b) = (chunk[0], chunk[1], chunk[2], chunk[3]);
            x11_data.push(b);
            x11_data.push(g);
            x11_data.push(r);
            x11_data.push(a);
        }
        let depth_use = if depth >= 24 { 24 } else { depth };
        conn.send_request(&x::PutImage {
            format: x::ImageFormat::ZPixmap,
            drawable: x::Drawable::Window(win),
            gc,
            width: width as u16,
            height: height as u16,
            dst_x: 0,
            dst_y: 0,
            left_pad: 0,
            depth: depth_use,
            data: &x11_data,
        });
        let _ = conn.flush();

        if wrong && Instant::now() > wrong_until {
            wrong = false;
        }

        let mut event_ready = false;
        while let Ok(Some(ev)) = conn.poll_for_event() {
            event_ready = true;
            match ev {
                xcb::Event::X(x::Event::Expose(_)) => {}
                xcb::Event::X(x::Event::KeyPress(ke)) => {
                    let keycode = ke.detail();
                    let keysym = keycode_to_keysym(&conn, keycode)?;
                    if keysym == 0xff0d || keysym == 0xff8d {
                        if authenticate(&password) {
                            authenticated = true;
                            break;
                        }
                        password.clear();
                        wrong = true;
                        wrong_until = Instant::now() + Duration::from_secs(2);
                    } else if keysym == 0xff08 {
                        password.pop();
                    } else if let Some(c) = keysym_to_char(keysym) {
                        if password.len() < 256 {
                            password.push(c);
                        }
                    }
                }
                _ => {}
            }
        }

        if !event_ready {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    conn.send_request(&x::UngrabKeyboard { time: x::CURRENT_TIME });
    conn.send_request(&x::UngrabPointer { time: x::CURRENT_TIME });
    conn.send_request(&x::UnmapWindow { window: win });
    let _ = conn.flush();
    Ok(())
}

fn draw_frame(
    cr: &Context,
    width: f64,
    height: f64,
    input_len: usize,
    wrong: bool,
) -> Result<(), String> {
    cr.set_source_rgb(VOID_BLACK.0, VOID_BLACK.1, VOID_BLACK.2);
    cr.paint().map_err(|e| e.to_string())?;

    cr.select_font_face(FONT, cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(CLOCK_SIZE);
    let time_str = Local::now().format("%H:%M").to_string();
    let date_str = Local::now().format("%Y-%m-%d").to_string();
    let te_time = cr.text_extents(&time_str).map_err(|e| e.to_string())?;
    let te_date = cr.text_extents(&date_str).map_err(|e| e.to_string())?;
    let tx = te_time.x_advance();
    let dx = te_date.x_advance();
    cr.set_source_rgb(WHITE_SMOKE.0, WHITE_SMOKE.1, WHITE_SMOKE.2);
    cr.move_to((width - tx) / 2.0, height * 0.35);
    cr.show_text(&time_str).map_err(|e| e.to_string())?;
    cr.set_font_size(24.0);
    cr.set_source_rgb(MUTED_BLUE.0, MUTED_BLUE.1, MUTED_BLUE.2);
    cr.move_to((width - dx) / 2.0, height * 0.35 + 36.0);
    cr.show_text(&date_str).map_err(|e| e.to_string())?;

    let cx = width / 2.0;
    let cy = height * 0.55;
    cr.set_source_rgba(HOLO_BLUE.0, HOLO_BLUE.1, HOLO_BLUE.2, 0.4);
    cr.set_line_width(RING_WIDTH);
    cr.arc(cx, cy, INDICATOR_RADIUS, 0.0, 2.0 * std::f64::consts::PI);
    cr.stroke().map_err(|e| e.to_string())?;
    let segments = 32;
    let filled = (input_len % segments).min(segments);
    if filled > 0 {
        cr.set_source_rgb(HOLO_BLUE.0, HOLO_BLUE.1, HOLO_BLUE.2);
        cr.set_line_width(RING_WIDTH - 2.0);
        for i in 0..filled {
            let a0 = (i as f64 / segments as f64) * 2.0 * std::f64::consts::PI - std::f64::consts::PI / 2.0;
            let a1 = ((i + 1) as f64 / segments as f64) * 2.0 * std::f64::consts::PI - std::f64::consts::PI / 2.0;
            cr.arc(cx, cy, INDICATOR_RADIUS - 2.0, a0, a1);
            cr.stroke().map_err(|e| e.to_string())?;
        }
    }
    cr.set_font_size(14.0);
    let hint = if wrong { "Wrong password" } else { "Enter password" };
    let te_hint = cr.text_extents(hint).map_err(|e| e.to_string())?;
    let hx = te_hint.x_advance();
    if wrong {
        cr.set_source_rgb(CYBERPUNK_RED.0, CYBERPUNK_RED.1, CYBERPUNK_RED.2);
    } else {
        cr.set_source_rgb(MUTED_BLUE.0, MUTED_BLUE.1, MUTED_BLUE.2);
    }
    cr.move_to((width - hx) / 2.0, cy + INDICATOR_RADIUS + 28.0);
    cr.show_text(hint).map_err(|e| e.to_string())?;

    Ok(())
}

fn keycode_to_keysym(conn: &xcb::Connection, keycode: u8) -> Result<u32, String> {
    let cookie = conn.send_request(&x::GetKeyboardMapping {
        first_keycode: keycode,
        count: 1,
    });
    let reply = conn.wait_for_reply(cookie).map_err(|e| e.to_string())?;
    let keysyms = reply.keysyms();
    Ok(keysyms.first().copied().unwrap_or(0))
}

fn keysym_to_char(keysym: u32) -> Option<char> {
    if (0x20..=0x7e).contains(&keysym) {
        return char::from_u32(keysym);
    }
    if (0xa0..=0xff).contains(&keysym) {
        return char::from_u32(keysym);
    }
    None
}

fn authenticate(password: &str) -> bool {
    let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
    let service = match std::ffi::CString::new("rogue-lock") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let user_c = match std::ffi::CString::new(user.as_str()) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let mut conv = PamConv {
        password: password.as_bytes().to_vec(),
        pos: 0,
    };
    unsafe {
        let mut pam_handle: *mut libpam_sys::pam_handle = std::ptr::null_mut();
        let mut conv_c = libpam_sys::pam_conv {
            conv: pam_conv_cb,
            appdata_ptr: &mut conv as *mut PamConv as *mut c_void,
        };
        if libpam_sys::pam_start(
            service.as_ptr(),
            user_c.as_ptr(),
            &mut conv_c,
            &mut pam_handle,
        ) != libpam_sys::PAM_SUCCESS
        {
            return false;
        }
        let ret = libpam_sys::pam_authenticate(pam_handle, 0);
        let _ = libpam_sys::pam_end(pam_handle, 0);
        ret == libpam_sys::PAM_SUCCESS
    }
}

struct PamConv {
    password: Vec<u8>,
    pos: usize,
}

extern "C" fn pam_conv_cb(
    num_msg: c_int,
    msg: *const *const libpam_sys::pam_message,
    resp: *mut *mut libpam_sys::pam_response,
    appdata_ptr: *mut c_void,
) -> c_int {
    if num_msg <= 0 || msg.is_null() || resp.is_null() || appdata_ptr.is_null() {
        return libpam_sys::PAM_CONV_ERR;
    }
    let conv = unsafe { &mut *(appdata_ptr as *mut PamConv) };
    let n = num_msg as usize;
    let out = unsafe {
        libc::malloc(std::mem::size_of::<libpam_sys::pam_response>() * n) as *mut libpam_sys::pam_response
    };
    if out.is_null() {
        return libpam_sys::PAM_CONV_ERR;
    }
    for i in 0..n {
        let m = unsafe { *msg.add(i) };
        let resp_slot = unsafe { &mut *out.add(i) };
        resp_slot.resp = std::ptr::null_mut();
        resp_slot.resp_retcode = 0;
        if m.is_null() {
            continue;
        }
        let msg_type = unsafe { (*m).msg_style };
        if msg_type == libpam_sys::PAM_PROMPT_ECHO_OFF
            || msg_type == libpam_sys::PAM_PROMPT_ECHO_ON
        {
            let pass_with_nul: Vec<u8> = conv.password[conv.pos..].iter().cloned().chain(std::iter::once(0)).collect();
            conv.pos = conv.password.len();
            let ptr = unsafe { libc::malloc(pass_with_nul.len()) as *mut c_char };
            if ptr.is_null() {
                for j in 0..=i {
                    let r = unsafe { &*out.add(j) };
                    if !r.resp.is_null() {
                        unsafe { libc::free(r.resp as *mut c_void) };
                    }
                }
                unsafe { libc::free(out as *mut c_void) };
                return libpam_sys::PAM_CONV_ERR;
            }
            unsafe {
                std::ptr::copy_nonoverlapping(pass_with_nul.as_ptr() as *const c_char, ptr, pass_with_nul.len());
            }
            resp_slot.resp = ptr;
        }
    }
    unsafe {
        *resp = out;
    }
    libpam_sys::PAM_SUCCESS
}
