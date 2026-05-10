use anyhow::Result;
use x11rb::protocol::xproto::{ClientMessageEvent, EventMask, ClientMessageData, CLIENT_MESSAGE_EVENT, ConnectionExt};
use x11rb::CURRENT_TIME;

use crate::roguewm::manager::WindowManager;

impl<'a> WindowManager<'a> {
    pub fn kill_client(&mut self) -> Result<()> {
        let mon_idx = self.sel_mon;
        if let Some(win) = self.monitors[mon_idx].selected_client {
             // Send WM_DELETE_WINDOW
             let event = ClientMessageEvent {
                 response_type: CLIENT_MESSAGE_EVENT,
                 format: 32,
                 sequence: 0,
                 window: win,
                 type_: self.atoms.wm_protocols,
                 data: ClientMessageData::from([self.atoms.wm_delete_window, CURRENT_TIME, 0, 0, 0]),
             };
             self.conn.send_event(false, win, EventMask::NO_EVENT, event)?;
        }
        Ok(())
    }

    pub fn view_tag(&mut self, tag_mask: u32) -> Result<()> {
        let mon_idx = self.sel_mon;
        let mask = if tag_mask == !0 { !0 } else { tag_mask & !0 }; // Simple mask for now
        self.monitors[mon_idx].tags = mask;
        self.focus(None)?; 
        self.arrange()?;
        Ok(())
    }

    pub fn tag_client(&mut self, tag_mask: u32) -> Result<()> {
        let mon_idx = self.sel_mon;
        if let Some(focused) = self.monitors[mon_idx].selected_client {
            if let Some(client) = self.clients.get_mut(&focused) {
                client.tags = tag_mask;
            }
            self.arrange()?;
        }
        Ok(())
    }

    pub fn toggle_floating(&mut self) -> Result<()> {
        let mon_idx = self.sel_mon;
        if let Some(focused) = self.monitors[mon_idx].selected_client {
            if let Some(client) = self.clients.get_mut(&focused) {
                client.is_floating = !client.is_floating;
            }
            self.arrange()?;
        }
        Ok(())
    }

    pub fn move_stack(&mut self, dir: i32) -> Result<()> {
        let mon_idx = self.sel_mon;
        // Need mutable access to monitor's client list
        
        let len = self.monitors[mon_idx].clients.len();
        if len < 2 { return Ok(()); }
        
        let current_idx = if let Some(focused) = self.monitors[mon_idx].selected_client {
            self.monitors[mon_idx].clients.iter().position(|&w| w == focused).unwrap_or(0)
        } else {
            0
        };
        
        let new_idx = if dir > 0 {
            (current_idx + 1) % len
        } else {
             if current_idx == 0 { len - 1 } else { current_idx - 1 }
        };
        
        self.monitors[mon_idx].clients.swap(current_idx, new_idx);
        self.arrange()?;
        
        Ok(())
    }
}
