use anyhow::Result;
use x11rb::protocol::xproto::KeyPressEvent;
use crate::roguewm::manager::WindowManager;

impl<'a> WindowManager<'a> {
    pub fn handle_key_press(&mut self, event: KeyPressEvent) -> Result<()> {
        let action = self.keybindings.iter().find_map(|binding| {
            if binding.key == event.detail && u16::from(binding.mod_mask) == u16::from(event.state) {
                Some(binding.action.clone())
            } else {
                None
            }
        });

        if let Some(act) = action {
            self.process_action(act)?;
        }
        Ok(())
    }
}
