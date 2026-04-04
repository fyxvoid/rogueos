//! Unified session binary: server + compositor + WM in one process.
//! Uses kernel backend (sys_fb_*). Handles input and shortcuts.

#![no_std]
#![no_main]

use libs::keycodes;
use userland::backend_kernel::KernelBackend;
use userland::{
    sys_exit, sys_poll_input, sys_write,
};
use userland_compositor::{composite, Compositor};
use userland_core::{Config, ShortcutAction};
use userland_server::Server;
use userland_utils;
use userland_wm::{key_to_action, Window, Wm};

const BG_COLOR: u32 = 0xFF202020;
const WINDOW_INACTIVE: u32 = 0xFF303040;
const WINDOW_ACTIVE: u32 = 0xFF505080;
const BORDER_INACTIVE: u32 = 0xFF101010;
const BORDER_ACTIVE: u32 = 0xFFAAAAFF;

static WINDOWS: [Window; 3] = [
    Window::new(80, 80, 320, 200, WINDOW_INACTIVE, BORDER_INACTIVE, 2),
    Window::new(440, 80, 320, 200, WINDOW_INACTIVE, BORDER_INACTIVE, 2),
    Window::new(800, 80, 320, 200, WINDOW_INACTIVE, BORDER_INACTIVE, 2),
];

fn default_config() -> Config {
    let mut c = Config::default();
    c.shortcuts[ShortcutAction::FocusLeft as usize] = keycodes::KEY_LEFT;
    c.shortcuts[ShortcutAction::FocusRight as usize] = keycodes::KEY_RIGHT;
    c.shortcuts[ShortcutAction::Confirm as usize] = keycodes::KEY_ENTER;
    c.shortcuts[ShortcutAction::Exit as usize] = keycodes::KEY_ESC;
    c
}

#[no_mangle]
fn _start() -> ! {
    log(b"[session] started\r\n");

    let config = default_config();
    let backend = KernelBackend::new();
    let mut server = Server::new(backend);
    let mut compositor = Compositor::new(&config);
    let mut wm = Wm::new(&WINDOWS);

    let mut rect_buf = [userland_compositor::WindowRect {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
        fill_color: 0,
        border_color: 0,
        border_px: 0,
    }; 16];

    fn draw(
        server: &mut Server<KernelBackend>,
        compositor: &Compositor,
        wm: &Wm,
        rect_buf: &mut [userland_compositor::WindowRect],
    ) {
        let n = wm.window_rects(WINDOW_ACTIVE, BORDER_ACTIVE, rect_buf);
        composite(
            server.backend_mut(),
            BG_COLOR,
            compositor,
            &rect_buf[..n],
        );
    }

    draw(&mut server, &compositor, &wm, &mut rect_buf);

    let mut ev = libs::KeyEvent {
        keycode: 0,
        pressed: false,
    };

    loop {
        let n = sys_poll_input(&mut ev);
        if n <= 0 {
            continue;
        }
        if !ev.pressed {
            continue;
        }

        let keycode = ev.keycode;
        if let Some(action) = key_to_action(&config, keycode) {
            match action {
                ShortcutAction::FocusLeft => {
                    wm.focus_left();
                    draw(&mut server, &compositor, &wm, &mut rect_buf);
                }
                ShortcutAction::FocusRight => {
                    wm.focus_right();
                    draw(&mut server, &compositor, &wm, &mut rect_buf);
                }
                ShortcutAction::IncreaseTransparency => {
                    compositor.increase_transparency(&config);
                    draw(&mut server, &compositor, &wm, &mut rect_buf);
                }
                ShortcutAction::DecreaseTransparency => {
                    compositor.decrease_transparency(&config);
                    draw(&mut server, &compositor, &wm, &mut rect_buf);
                }
                ShortcutAction::IncreaseCornerRadius => {
                    compositor.increase_corner_radius(&config);
                    draw(&mut server, &compositor, &wm, &mut rect_buf);
                }
                ShortcutAction::DecreaseCornerRadius => {
                    compositor.decrease_corner_radius(&config);
                    draw(&mut server, &compositor, &wm, &mut rect_buf);
                }
                ShortcutAction::Screenshot => {
                    userland_utils::screenshot();
                }
                ShortcutAction::Lock => {
                    userland_utils::lock();
                }
                ShortcutAction::ClipboardPaste => {
                    userland_utils::clipboard_paste();
                }
                ShortcutAction::Confirm => {
                    draw(&mut server, &compositor, &wm, &mut rect_buf);
                }
                ShortcutAction::Exit => {
                    log(b"[session] exit\r\n");
                    sys_exit(0);
                }
            }
        }
    }
}

fn log(msg: &[u8]) {
    let _ = sys_write(1, msg.as_ptr(), msg.len());
}
