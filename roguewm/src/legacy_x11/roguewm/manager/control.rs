use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::CURRENT_TIME;
use log::info;

use super::WindowManager;
use crate::roguewm::config::*;
use crate::roguewm::client::Client;
use crate::roguewm::layouts::{calculate_layout, LayoutState, Gaps};

impl<'a> WindowManager<'a> {
    pub fn manage(&mut self, window: Window, _attr: &GetWindowAttributesReply) -> Result<()> {
        let geom = self.conn.get_geometry(window)?.reply()?;
        info!("Managing window: {} ({}x{})", window, geom.width, geom.height);
        
        let values = ChangeWindowAttributesAux::new()
            .event_mask(EventMask::ENTER_WINDOW | EventMask::FOCUS_CHANGE | EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY);
        self.conn.change_window_attributes(window, &values)?;

        let mon_idx = self.sel_mon;
        let default_tags = self.monitors[mon_idx].tags;

        let (tags, is_floating, class) = self.apply_rules(window).unwrap_or((default_tags, false, String::from("unknown")));
        info!("Assigned tags: {}, is_floating: {}, class: {}", tags, is_floating, class);

        let client = Client {
            window,
            tags,
            is_floating,
            class,
        };
        
        self.clients.insert(window, client);
        self.monitors[mon_idx].clients.push(window);

        self.grab_buttons_for_client(window)?;
        self.update_client_list()?; 
        self.configure_border(window)?;
        self.arrange()?;
        self.conn.map_window(window)?;
        
        if let Some(focused) = self.monitors[mon_idx].selected_client {
             self.unfocus(focused)?;
        }
        
        self.focus(Some(window))?;
        self.arrange()?; 
        Ok(())
    }

    #[allow(dead_code)]
    pub fn unmanage(&mut self, window: Window) -> Result<()> {
        // unmanage logic... mapped to remove_window
        self.remove_window(window)
    }
    
    pub(crate) fn remove_window(&mut self, window: Window) -> Result<()> {
        if self.clients.remove(&window).is_some() {
            for mon in &mut self.monitors {
                mon.clients.retain(|&w| w != window);
                if mon.selected_client == Some(window) {
                     mon.selected_client = mon.clients.last().copied();
                }
            }
            
            self.update_client_list()?;
            
            let mon_idx = self.sel_mon;
             if self.monitors[mon_idx].selected_client.is_some() {
                 let win = self.monitors[mon_idx].selected_client;
                 self.focus(win)?;
             }

            self.arrange()?;
        }
        Ok(())
    }

    pub fn arrange(&mut self) -> Result<()> {
        for mon_idx in 0..self.monitors.len() {
            let mon = &self.monitors[mon_idx];
            let container = mon.window_rect; 

            let mut visible_clients = Vec::new();
            for &win in &mon.clients {
                if let Some(c) = self.clients.get(&win) {
                    if c.tags & mon.tags != 0 {
                        visible_clients.push(win);
                    }
                }
            }
            
            if visible_clients.is_empty() {
                self.draw_bar(mon_idx)?;
                continue;
            }

            let use_gaps = if mon.show_gaps && visible_clients.len() > 1 {
                mon.gaps
            } else {
                Gaps { oh: 0, ov: 0, ih: 0, iv: 0 }
            };
            
            // Calculate layout for current monitor's layout symbol
            let state = LayoutState {
                 container,
                 client_count: visible_clients.len(),
                 nmaster: mon.nmaster,
                 mfact: mon.mfact,
                 gaps: use_gaps,
            };
            
            let rects = calculate_layout(mon.layout, &state);
            for (i, &win) in visible_clients.iter().enumerate() {
                if let Some(&rect) = rects.get(i) {
                    if mon.layout != LayoutSymbol::Floating {
                         // Only configure if not floating? Or tile logic handles floating windows?
                         // Ideally floating windows are skipped by tiling logic.
                         // But our calculating logic currently assumes all visible clients get rects.
                         // Let's assume tile/monocle/dwindle handle all passed clients.
                         // Check is_floating?
                         if let Some(c) = self.clients.get(&win) {
                             if !c.is_floating {
                                  self.configure_client(win, rect)?;
                             }
                         }
                    }
                }
            }
            
            self.draw_bar(mon_idx)?;
        }
        Ok(())
    }
    
    pub fn draw_bar(&self, mon_idx: usize) -> Result<()> {
        let mon = &self.monitors[mon_idx];
        let occupied_tags = self.clients.values().fold(0, |acc, c| acc | c.tags); 
        let urgent_tags = 0; 
        
        let title = if let Some(_win) = mon.selected_client { "Active Window" } else { "" };
        
        let layout_str = match mon.layout {
            LayoutSymbol::Tile => "[]=",
            LayoutSymbol::Monocle => "[M]",
            LayoutSymbol::Floating => "><>",
            LayoutSymbol::Dwindle => "[@]",
        };
        
        mon.bar.draw(
            self.conn,
            occupied_tags,
            mon.tags,
            layout_str,
            title,
            &self.status_text,
            urgent_tags,
        )?;
        Ok(())
    }

    pub(crate) fn update_client_list(&self) -> Result<()> {
        let all_clients: Vec<Window> = self.monitors.iter().flat_map(|m| m.clients.clone()).collect();

        self.conn.change_property(
            PropMode::REPLACE,
            self.conn.setup().roots[self.screen_num].root,
            self.atoms.net_client_list,
            AtomEnum::WINDOW,
            32,
            all_clients.len() as u32,
            bytemuck::cast_slice(&all_clients),
        )?;
        Ok(())
    }
    
    pub(crate) fn focus(&mut self, win: Option<Window>) -> Result<()> {
        let root = self.conn.setup().roots[self.screen_num].root;
        let mon_idx = self.sel_mon;
        
        if let Some(w) = win {
            self.conn.set_input_focus(InputFocus::POINTER_ROOT, w, CURRENT_TIME)?;
            self.conn.change_property(
                PropMode::REPLACE,
                root,
                self.atoms.net_active_window,
                AtomEnum::WINDOW,
                32,
                1,
                &w.to_ne_bytes(),
            )?;
            self.monitors[mon_idx].selected_client = Some(w);
            let values = ChangeWindowAttributesAux::new().border_pixel(COLOR_SEL_BORDER); 
            self.conn.change_window_attributes(w, &values)?;
        } else {
            self.conn.set_input_focus(InputFocus::POINTER_ROOT, root, CURRENT_TIME)?;
            self.conn.change_property(
                PropMode::REPLACE,
                root,
                self.atoms.net_active_window,
                AtomEnum::WINDOW,
                32,
                0, // None
                &[], // Empty data
            )?;
            self.monitors[mon_idx].selected_client = None;
        }
        self.draw_bar(mon_idx)?;
        Ok(())
    }
    
    pub(crate) fn grab_keys(&self) -> Result<()> {
        let screen = &self.conn.setup().roots[self.screen_num];
        let root = screen.root;

        self.conn.ungrab_key(Grab::ANY, root, ModMask::ANY)?;

        for key in &self.keybindings {
            self.conn.grab_key(
                true,
                root,
                key.mod_mask,
                key.key,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?;
        }
        Ok(())
    }
    
    pub(crate) fn grab_buttons_for_client(&self, win: Window) -> Result<()> {
        let _modifiers = [ModMask::from(MOD_KEY)]; 
        for button in [ButtonIndex::M1, ButtonIndex::M2, ButtonIndex::M3] {
            self.conn.grab_button(
                true,
                win,
                EventMask::BUTTON_PRESS,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                x11rb::NONE,
                x11rb::NONE,
                button,
                MOD_KEY,
            )?;
        }
         self.conn.grab_button(
            true,
            win,
            EventMask::BUTTON_PRESS,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            x11rb::NONE,
            x11rb::NONE,
            ButtonIndex::ANY,
            ModMask::ANY,
         )?;
        Ok(())
    }
}
