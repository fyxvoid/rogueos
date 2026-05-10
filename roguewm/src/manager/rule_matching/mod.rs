use crate::roguewm::config::*;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, Window};

use anyhow::Result;

impl<'a> super::manager::WindowManager<'a> {
    pub fn apply_rules(&self, win: Window) -> Result<(u32, bool, String)> {
        // Fetch WM_CLASS
        let reply = self.conn.get_property(
            false,
            win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            0,
            1024
        )?.reply()?;

        let mut class = String::new();
        let mut instance = String::new();

        if let Some(val) = reply.value8() {
            // WM_CLASS is "instance\0class\0"
            let v: Vec<u8> = val.collect();
            let parts: Vec<&[u8]> = v.split(|&b| b == 0).collect();
            
            if let Some(inst) = parts.get(0) {
                instance = String::from_utf8_lossy(inst).to_string();
            }
            if let Some(cls) = parts.get(1) {
                class = String::from_utf8_lossy(cls).to_string();
            }
        }
        
        // Fetch WM_NAME (title)
        let reply_name = self.conn.get_property(
            false,
            win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            0,
            1024
        )?.reply()?;
        
        let title = if let Some(val) = reply_name.value8() {
            String::from_utf8_lossy(&val.collect::<Vec<u8>>()).to_string()
        } else {
            String::new()
        };

        for rule in RULES {
            let matches_class = rule.class.map_or(true, |c| class.contains(c));
            let matches_instance = rule.instance.map_or(true, |i| instance.contains(i));
            let matches_title = rule.title.map_or(true, |t| title.contains(t));

            if matches_class && matches_instance && matches_title {
                return Ok((rule.tags, rule.is_floating, class)); // Return class
            }
        }
        
        Ok((self.monitors[self.sel_mon].tags, false, class)) // Return class even if no rule matched
    }
}
