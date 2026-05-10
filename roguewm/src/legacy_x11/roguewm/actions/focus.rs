use anyhow::Result;
use crate::roguewm::manager::WindowManager;

impl<'a> WindowManager<'a> {
    pub fn cycle_focus(&mut self, next: bool) -> Result<()> {
        let mon = &self.monitors[self.sel_mon];
        if mon.clients.is_empty() { return Ok(()); }
        
        let clients = &mon.clients;
        let current_idx = if let Some(focused) = mon.selected_client {
            clients.iter().position(|&w| w == focused).unwrap_or(0)
        } else {
            0
        };
        
        let next_idx = if next {
            (current_idx + 1) % clients.len()
        } else {
            if current_idx == 0 { clients.len() - 1 } else { current_idx - 1 }
        };
        
        self.focus(Some(clients[next_idx]))?;
        Ok(())
    }
}
