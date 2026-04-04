//! Integration test: display server + WM path.
//! Connects to X11, gets root window and screen, interns _NET_WM_NAME, disconnects.
//! Skips (passes) when DISPLAY is not set so CI without Xvfb still passes.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt;

#[test]
fn connect_display_server_and_intern_atom() {
    if std::env::var("DISPLAY").is_err() {
        return; // no X; pass without exercising display server
    }
    let (conn, screen_num) = x11rb::connect(None).expect("x11rb::connect");
    let screen = &conn.setup().roots[screen_num];
    let _root = screen.root;
    let _atom = conn
        .intern_atom(false, b"_NET_WM_NAME")
        .expect("intern_atom")
        .reply()
        .expect("intern_atom reply");
    drop(conn);
}
