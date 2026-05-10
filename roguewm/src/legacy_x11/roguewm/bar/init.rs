use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::COPY_FROM_PARENT;
use crate::roguewm::config::*;
use super::Bar;

impl Bar {
    pub const TAG_WIDTH: i16 = 20;
    pub const LAYOUT_WIDTH: i16 = 30;

    pub fn new(conn: &RustConnection, screen_num: usize) -> Result<Self> {
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let width = screen.width_in_pixels;
        let height = 18; // Fixed height for now

        let window = conn.generate_id()?;
        let font = conn.generate_id()?;
        let gc = conn.generate_id()?;

        if let Err(_) = conn.open_font(font, b"fixed") {
             // Fallback or ignore
        }

        let values = CreateWindowAux::new()
            .background_pixel(COLOR_NORM_BORDER)
            .override_redirect(1)
            .event_mask(EventMask::EXPOSURE | EventMask::BUTTON_PRESS);

        conn.create_window(
            COPY_FROM_PARENT as u8,
            window,
            root,
            0,
            0,
            width,
            height,
            0,
            WindowClass::INPUT_OUTPUT,
            COPY_FROM_PARENT,
            &values,
        )?;

        let gc_values = CreateGCAux::new()
            .foreground(screen.white_pixel)
            .background(COLOR_NORM_BORDER)
            .font(font);
            
        conn.create_gc(gc, window, &gc_values)?;

        conn.map_window(window)?;

        Ok(Self {
            window,
            font,
            gc,
            width,
            height,
        })
    }
}
