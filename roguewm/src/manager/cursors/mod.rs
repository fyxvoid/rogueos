#![allow(dead_code)]
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use anyhow::Result;

pub struct Cursors {
    pub normal: Cursor,
    pub move_: Cursor,
    pub resize: Cursor,
}

impl Cursors {
    pub fn new(conn: &RustConnection, screen_num: usize) -> Result<Self> {
        let font = conn.generate_id()?;
        conn.open_font(font, b"cursor")?;

        let normal = conn.generate_id()?;
        let move_ = conn.generate_id()?;
        let resize = conn.generate_id()?;

        // XC_left_ptr = 68
        // XC_fleur = 52
        // XC_sizing = 120 (bottom_right_corner?)
        
        conn.create_glyph_cursor(normal, font, font, 68, 69, 0, 0, 0, 65535, 65535, 65535)?;
        conn.create_glyph_cursor(move_, font, font, 52, 53, 0, 0, 0, 65535, 65535, 65535)?;
        conn.create_glyph_cursor(resize, font, font, 120, 121, 0, 0, 0, 65535, 65535, 65535)?;

        conn.close_font(font)?;

        // Set root cursor
        let root = conn.setup().roots[screen_num].root;
        conn.change_window_attributes(root, &ChangeWindowAttributesAux::new().cursor(normal))?;

        Ok(Self {
            normal,
            move_,
            resize,
        })
    }
}
