use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::COPY_FROM_PARENT;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (conn, screen_num) = RustConnection::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let win_id = conn.generate_id()?;

    let width = 300;
    let height = 200;

    conn.create_window(
        COPY_FROM_PARENT as u8,
        win_id,
        screen.root,
        10,
        10,
        width,
        height,
        0,
        WindowClass::INPUT_OUTPUT,
        screen.root_visual,
        &CreateWindowAux::new().background_pixel(0xFF0000), // Red background
    )?;

    conn.map_window(win_id)?;
    conn.flush()?;
    
    // Set WM_CLASS so dwm-rs can filter it if needed
    // Instance: test, Class: TestClient
    let wm_class = b"test\0TestClient\0";
    conn.change_property(
        PropMode::REPLACE,
        win_id,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        8,
        wm_class.len() as u32,
        wm_class,
    )?;

    loop {
        conn.flush()?;
        thread::sleep(Duration::from_millis(100));
    }
}
