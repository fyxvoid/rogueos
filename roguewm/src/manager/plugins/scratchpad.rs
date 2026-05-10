use crate::roguewm::plugin::Plugin;
use crate::roguewm::manager::WindowManager;
use crate::roguewm::config::{MOD_KEY, SHIFT_KEY}; // Reuse config constants
use x11rb::protocol::Event;
use x11rb::protocol::xproto::ModMask;
use anyhow::Result;
use log::info;
use std::process::Command;

pub struct ScratchpadPlugin {
    cmd: String,
    class: String,
    key: u8,
    mod_mask: ModMask,
}

impl ScratchpadPlugin {
    pub fn new() -> Self {
        Self {
            cmd: "st -c scratchpad".to_string(), // Ensure your terminal supports -c class
            class: "scratchpad".to_string(),
            key: 0x27, // 's' key (approximate, hardcoded for now or import keycode mapping)
            // 0x27 is notoriously unreliable across keymaps. Let's use 0x39 (space) or similar?
            // Actually config.rs uses hardcoded keys. 
            // 0x27 is 's' on US layout.
            mod_mask: MOD_KEY | SHIFT_KEY,
        }
    }
}

impl Plugin for ScratchpadPlugin {
    fn name(&self) -> &str {
        "Scratchpad"
    }

    fn on_event(&mut self, wm: &mut WindowManager, event: &Event) -> Result<bool> {
        if let Event::KeyPress(e) = event {
            if e.detail == self.key && (u16::from(e.state) & u16::from(self.mod_mask)) == u16::from(self.mod_mask) {
                info!("[ScratchpadPlugin] Toggle scratchpad triggered");
                
                // Toggle logic
                // 1. Find client with matching class
                let found_window = wm.clients.values().find(|c| c.class == self.class).map(|c| c.window);
                
                if let Some(window) = found_window {
                    info!("[ScratchpadPlugin] Found existing scratchpad: {}", window);
                    // Toggle visibility
                    let mut should_focus = false;
                    let current_tags = wm.monitors[wm.sel_mon].tags;
                    
                    if let Some(client) = wm.clients.get_mut(&window) {
                        if (client.tags & current_tags) != 0 {
                            // Hide
                            client.tags = 0; 
                        } else {
                            // Show
                            client.tags = current_tags;
                            client.is_floating = true;
                            should_focus = true;
                        }
                    }
                    
                    wm.arrange()?;
                    if should_focus {
                       wm.focus(Some(window))?;
                    }
                } else {
                    info!("[ScratchpadPlugin] Spawning scratchpad: {}", self.cmd);
                    let mut parts = self.cmd.split_whitespace();
                    if let Some(bin) = parts.next() {
                         let args: Vec<&str> = parts.collect();
                         Command::new(bin).args(args).spawn().ok();
                    }
                }
                return Ok(true); // Consume event
            }
        }
        Ok(false)
    }
}
