pub mod general;
pub mod focus;
pub mod layout;
pub mod client;

use anyhow::Result;
use crate::roguewm::manager::WindowManager;
use crate::roguewm::config::Action;

impl<'a> WindowManager<'a> {
    pub fn process_action(&mut self, act: Action) -> Result<()> {
        match act {
            Action::Quit => self.quit(),
            Action::Spawn(cmd) => self.spawn(cmd)?,
            Action::FocusNext => self.cycle_focus(true)?,
            Action::FocusPrev => self.cycle_focus(false)?,
            Action::SetLayout(layout) => self.set_layout(layout)?,
            Action::SetMFact(f) => self.set_mfact(f)?,
            Action::MoveStack(dir) => self.move_stack(dir)?,
            Action::KillClient => self.kill_client()?,
            Action::ToggleGaps => self.toggle_gaps()?,
            Action::ViewTag(tag_mask) => self.view_tag(tag_mask)?,
            Action::TagClient(tag_mask) => self.tag_client(tag_mask)?,
            Action::ToggleFloating => self.toggle_floating()?,
            _ => {}
        }
        Ok(())
    }
}
