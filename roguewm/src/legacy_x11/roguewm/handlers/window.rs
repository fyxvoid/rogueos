use anyhow::Result;
use x11rb::protocol::xproto::*;
use x11rb::connection::Connection;
use log::info;
use crate::roguewm::manager::WindowManager;

impl<'a> WindowManager<'a> {
    pub fn handle_map_request(&mut self, event: MapRequestEvent) -> Result<()> {
        let win = event.window;
        info!("MapRequest for window: {}", win);
        
        let attr = self.conn.get_window_attributes(win)?.reply()?;
        if attr.override_redirect {
            return Ok(());
        }

        if !self.clients.contains_key(&win) {
             self.manage(win, &attr)?;
        }
        self.conn.map_window(win)?;
        Ok(())
    }

    pub fn handle_enter_notify(&mut self, event: EnterNotifyEvent) -> Result<()> {
        let win = event.event;
        if self.clients.contains_key(&win) {
             let mut found_mon = None;
             for (i, mon) in self.monitors.iter().enumerate() {
                 if mon.clients.contains(&win) {
                     found_mon = Some(i);
                     break;
                 }
             }
             
             if let Some(idx) = found_mon {
                 self.sel_mon = idx;
                 if self.monitors[idx].selected_client != Some(win) {
                     self.focus(Some(win))?;
                 }
             }
        }
        Ok(())
    }

    pub fn handle_unmap_notify(&mut self, event: UnmapNotifyEvent) -> Result<()> {
        self.remove_window(event.window)?;
        Ok(())
    }

    pub fn handle_destroy_notify(&mut self, event: DestroyNotifyEvent) -> Result<()> {
        self.remove_window(event.window)?;
        Ok(())
    }

    pub fn handle_property_notify(&mut self, event: PropertyNotifyEvent) -> Result<()> {
        let root = self.conn.setup().roots[self.screen_num].root;
        
        if event.window == root && event.atom == AtomEnum::WM_NAME.into() {
             let reply = self.conn.get_property(
                false,
                root,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                0,
                1024,
             )?.reply()?;
             
             let bytes = reply.value8().map(|v| v.collect::<Vec<u8>>());
             
             if let Some(b) = bytes {
                 if let Ok(name) = String::from_utf8(b) {
                     self.status_text = name;
                     for i in 0..self.monitors.len() {
                         self.draw_bar(i)?;
                     }
                 }
             }
        }
        Ok(())
    }
}
