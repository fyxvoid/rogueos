use anyhow::Result;
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use log::info;

use super::{WindowManager, DragMode};
use crate::roguewm::atoms::Atoms;
use crate::roguewm::config::*;
use crate::roguewm::cursors::Cursors;
use crate::roguewm::bar::Bar;
use crate::roguewm::layouts::Rect;
use crate::roguewm::monitor::Monitor;

pub fn init_wm(conn: &RustConnection, screen_num: usize) -> Result<WindowManager<'_>> {
    info!("Initializing RogueWM on screen {}", screen_num);
    
    let screen = &conn.setup().roots[screen_num];
    let _root = screen.root;
    let width = screen.width_in_pixels;
    let height = screen.height_in_pixels;
    
    // Initial Monitor (Full Screen)
    // TODO: Query XRandR for real monitors
    let rect = Rect { x: 0, y: 0, width: width as u32, height: height as u32 };
    let bar = Bar::new(conn, screen_num)?;
    let monitor = Monitor::new(0, rect, bar);

    let wm = WindowManager {
        conn,
        screen_num,
        monitors: vec![monitor],
        sel_mon: 0,
        clients: HashMap::new(),
        keybindings: get_keybindings(),
        atoms: Atoms::new(conn)?,
        cursors: Cursors::new(conn, screen_num)?,
        status_text: String::from("RogueWM"),
        drag_mode: DragMode::None,
    };
    Ok(wm)
}

impl<'a> WindowManager<'a> {
    pub fn init(&mut self) -> Result<()> {
        let screen = &self.conn.setup().roots[self.screen_num];
        let root = screen.root;
        let width = screen.width_in_pixels;
        let height = screen.height_in_pixels;

        info!("Connected to X server. Root window: {}, Size: {}x{}", root, width, height);

        // Select events on root window
        let values = ChangeWindowAttributesAux::new()
            .event_mask(EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY | EventMask::BUTTON_PRESS | EventMask::KEY_PRESS);
        
        self.conn.change_window_attributes(root, &values)?;
        self.grab_keys()?;
        
        // EWMH Support: Set _NET_SUPPORTED
        let supported_atoms = [
            self.atoms.net_supported,
            self.atoms.net_client_list,
            self.atoms.net_number_of_desktops,
            self.atoms.net_current_desktop,
            self.atoms.net_active_window,
            self.atoms.net_wm_name,
            self.atoms.net_wm_state,
        ];
        
        self.conn.change_property(
            PropMode::REPLACE,
            root,
            self.atoms.net_supported,
            AtomEnum::ATOM,
            32,
            supported_atoms.len() as u32,
            bytemuck::cast_slice(&supported_atoms),
        )?;
        
        // Set _NET_NUMBER_OF_DESKTOPS (9 tags)
        self.conn.change_property(
            PropMode::REPLACE,
            root,
            self.atoms.net_number_of_desktops,
            AtomEnum::CARDINAL,
            32,
            1,
            &9u32.to_ne_bytes(),
        )?;
        
        self.conn.flush()?;
        
        self.run_autostart();

        info!("Waiting for events...");
        Ok(())
    }

    pub(crate) fn run_autostart(&self) {
        // Look for ~/.config/roguewm/autostart.sh
        if let Ok(home) = std::env::var("HOME") {
            let path = std::path::Path::new(&home).join(".config/roguewm/autostart.sh");
            if path.exists() {
                info!("Executing autostart script: {:?}", path);
                std::process::Command::new("sh")
                    .arg(&path)
                    .spawn()
                    .ok();
            } else {
                 info!("Autostart script not found at {:?}", path);
            }
        }
    }
}
