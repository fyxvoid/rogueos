use anyhow::Result;

use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use crate::roguewm::config::*;
use super::Bar;

impl Bar {
    pub fn draw(
        &self,
        conn: &RustConnection,
        tags: u32,
        current_tags: u32,
        layout_symbol: &str,
        title: &str,
        status: &str,
        urgent_tags: u32,
    ) -> Result<()> {
        let mut x = 0;
        
        // 1. Draw Tags
        let tag_names = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];
        let tag_width = Self::TAG_WIDTH;
        
        for (i, name) in tag_names.iter().enumerate() {
            let mask = 1 << i;
            let is_selected = (current_tags & mask) != 0;
            let is_occupied = (tags & mask) != 0;
            let is_urgent = (urgent_tags & mask) != 0;

            let bg = if is_selected { COLOR_SEL_BORDER } else { COLOR_NORM_BORDER };
            let fg = if is_selected { COLOR_NORM_BORDER } else { 0xbbbbbb };
            let (bg, fg) = if is_urgent { (0xff0000, 0xffffff) } else { (bg, fg) };

            self.draw_text_box(conn, x, 0, tag_width as u16, self.height, bg, fg, name)?;
            
            if is_occupied && !is_selected {
                 let rect = Rectangle {
                    x: x + 2,
                    y: 2,
                    width: 4,
                    height: 4,
                 };
                 let gc_val = ChangeGCAux::new().foreground(0xeeeeee);
                 conn.change_gc(self.gc, &gc_val)?;
                 conn.poly_fill_rectangle(self.window, self.gc, &[rect])?;
            }
            
            x += tag_width as i16;
        }

        // 2. Draw Layout Symbol
        let layout_width = Self::LAYOUT_WIDTH;
        self.draw_text_box(conn, x, 0, layout_width as u16, self.height, COLOR_NORM_BORDER, 0xeeeeee, layout_symbol)?;
        x += layout_width as i16;

        // 3. Draw Status (Right aligned)
        let char_width = 7; 
        let status_width = (status.len() as i16 * char_width) + 10;
        let status_x = (self.width as i16 - status_width).max(x);
        
        let clear_rect = Rectangle {
             x: status_x,
             y: 0,
             width: status_width as u16,
             height: self.height
        };
        let bg_gc = ChangeGCAux::new().foreground(COLOR_NORM_BORDER);
        conn.change_gc(self.gc, &bg_gc)?;
        conn.poly_fill_rectangle(self.window, self.gc, &[clear_rect])?;
        
        self.draw_text(conn, status_x + 5, 13, 0xeeeeee, status)?;

        // 4. Draw Title (Remaining space)
        if status_x > x {
             let title_width = (status_x - x) as u16;
             let title_bg = COLOR_NORM_BORDER;
             let title_fg = 0xeeeeee;
             self.draw_text_box(conn, x, 0, title_width, self.height, title_bg, title_fg, title)?;
        }
        
        Ok(())
    }

    fn draw_text_box(
        &self,
        conn: &RustConnection,
        x: i16,
        y: i16,
        w: u16,
        h: u16,
        bg: u32,
        fg: u32,
        text: &str,
    ) -> Result<()> {
        let rect = Rectangle { x, y, width: w, height: h };
        let bg_val = ChangeGCAux::new().foreground(bg);
        conn.change_gc(self.gc, &bg_val)?;
        conn.poly_fill_rectangle(self.window, self.gc, &[rect])?;

        self.draw_text(conn, x + 5, y + 13, fg, text)?;
        Ok(())
    }

    fn draw_text(&self, conn: &RustConnection, x: i16, y: i16, fg: u32, text: &str) -> Result<()> {
        let fg_val = ChangeGCAux::new().foreground(fg).background(0);
        conn.change_gc(self.gc, &fg_val)?;
        conn.image_text8(self.window, self.gc, x, y, text.as_bytes())?;
        Ok(())
    }
}
