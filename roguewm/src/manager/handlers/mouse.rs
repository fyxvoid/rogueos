use anyhow::Result;
use x11rb::protocol::xproto::*;
use x11rb::connection::Connection;
use x11rb::CURRENT_TIME;
use crate::roguewm::manager::{WindowManager, DragMode};
use crate::roguewm::config::*;
use crate::roguewm::bar::BarAction; // Ensure BarAction is exported properly from bar/mod.rs

impl<'a> WindowManager<'a> {
   pub fn handle_button_press(&mut self, event: ButtonPressEvent) -> Result<()> {
        let win = event.event;
        
        // Check for Bar Click
        let mut clicked_mon = None;
        for (i, mon) in self.monitors.iter().enumerate() {
            if win == mon.bar.window {
                clicked_mon = Some(i);
                break;
            }
        }

        if let Some(i) = clicked_mon {
             // Handle Bar Click
             self.sel_mon = i;
             let action = self.monitors[i].bar.handle_click(event.event_x, event.event_y);
             
             match action {
                 BarAction::Tag(tx) => {
                     let mask = 1 << tx;
                     if event.detail == 1 {
                         self.monitors[i].tags = mask;
                         self.arrange()?;
                         self.focus(None)?; 
                         self.draw_bar(i)?;
                     }
                 },
                 BarAction::Layout => {
                     if event.detail == 1 {
                         let mon = &mut self.monitors[i];
                         mon.layout = match mon.layout {
                             LayoutSymbol::Tile => LayoutSymbol::Monocle,
                             LayoutSymbol::Monocle => LayoutSymbol::Dwindle,
                             LayoutSymbol::Dwindle => LayoutSymbol::Floating,
                             LayoutSymbol::Floating => LayoutSymbol::Tile,
                         };
                         self.arrange()?;
                         self.draw_bar(i)?;
                     }
                 },
                 _ => {}
             }
             return Ok(());
        }

        // Check if window is managed
        if !self.clients.contains_key(&win) {
             return Ok(());
        }
        
        // Focus click
        // Find monitor for window
        for (i, mon) in self.monitors.iter().enumerate() {
            if mon.clients.contains(&win) {
                self.sel_mon = i;
                break;
            }
        }
        self.focus(Some(win))?;
        
        let mod_mask = u16::from(MOD_KEY);
        let state = u16::from(event.state);
        
        if state & mod_mask != 0 {
            match event.detail {
                1 => self.start_mouse_move(win, event.root_x, event.root_y)?,
                2 => self.process_action(crate::roguewm::config::Action::ToggleFloating)?,
                3 => self.start_mouse_resize(win, event.root_x, event.root_y)?,
                _ => {}
            }
        }
        
        Ok(())
    }

    pub(crate) fn start_mouse_move(&mut self, win: Window, root_x: i16, root_y: i16) -> Result<()> {
        let root = self.conn.setup().roots[self.screen_num].root;
        
        let attr = self.conn.get_geometry(win)?.reply()?;
        let (win_x, win_y) = (attr.x, attr.y);

        self.conn.grab_pointer(
            false,
            root,
            EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            x11rb::NONE,
            x11rb::NONE,
            CURRENT_TIME,
        )?.reply()?;

        if let Some(c) = self.clients.get_mut(&win) {
            if !c.is_floating {
                 c.is_floating = true;
                 self.arrange()?;
            }
        }
        
        self.drag_mode = DragMode::Moving(win, root_x, root_y, win_x, win_y);
        Ok(())
    }

    pub(crate) fn start_mouse_resize(&mut self, win: Window, root_x: i16, root_y: i16) -> Result<()> {
         let root = self.conn.setup().roots[self.screen_num].root;
        
         let attr = self.conn.get_geometry(win)?.reply()?;
         let (win_w, win_h) = (attr.width, attr.height);

        self.conn.grab_pointer(
            false,
            root,
            EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            x11rb::NONE, 
            x11rb::NONE,
            CURRENT_TIME,
        )?.reply()?;

        if let Some(c) = self.clients.get_mut(&win) {
            if !c.is_floating {
                 c.is_floating = true;
                 self.arrange()?;
            }
        }
        
        self.drag_mode = DragMode::Resizing(win, root_x, root_y, win_w, win_h);
        Ok(())
    }

    pub fn handle_motion_notify(&mut self, event: MotionNotifyEvent) -> Result<()> {
        let (mx, my) = (event.root_x, event.root_y);
        
        match self.drag_mode {
            DragMode::Moving(win, start_mx, start_my, start_wx, start_wy) => {
                 let dx = mx as i16 - start_mx;
                 let dy = my as i16 - start_my;
                 
                 let values = ConfigureWindowAux::new()
                     .x((start_wx as i16 + dx) as i32)
                     .y((start_wy as i16 + dy) as i32);
                 self.conn.configure_window(win, &values)?;
            },
            DragMode::Resizing(win, start_mx, start_my, start_w, start_h) => {
                 let dx = mx as i16 - start_mx;
                 let dy = my as i16 - start_my;

                 let new_w = (start_w as i32 + dx as i32).max(1);
                 let new_h = (start_h as i32 + dy as i32).max(1);
                 
                 let values = ConfigureWindowAux::new()
                     .width(new_w as u32)
                     .height(new_h as u32);
                 self.conn.configure_window(win, &values)?;
            },
            DragMode::None => {}
        }
        Ok(())
    }

    pub fn handle_button_release(&mut self, _event: ButtonReleaseEvent) -> Result<()> {
        if self.drag_mode != DragMode::None {
             self.conn.ungrab_pointer(CURRENT_TIME)?;
             self.drag_mode = DragMode::None;
        }
        Ok(())
    }
}
