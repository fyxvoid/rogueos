pub mod window;
pub mod mouse;
pub mod keyboard;

use anyhow::Result;
use x11rb::protocol::Event;
use log::debug;
use crate::roguewm::manager::WindowManager;

impl<'a> WindowManager<'a> {
    pub fn handle_event(&mut self, event: Event) -> Result<()> {
        debug!("Received event: {:?}", event);
        match event {
            Event::MapRequest(event) => self.handle_map_request(event)?,
            Event::KeyPress(event) => self.handle_key_press(event)?,
            Event::ButtonPress(event) => self.handle_button_press(event)?,
            Event::UnmapNotify(event) => self.handle_unmap_notify(event)?,
            Event::DestroyNotify(event) => self.handle_destroy_notify(event)?,
            Event::EnterNotify(event) => self.handle_enter_notify(event)?,
            Event::PropertyNotify(event) => self.handle_property_notify(event)?,
            Event::MotionNotify(event) => self.handle_motion_notify(event)?,
            Event::ButtonRelease(event) => self.handle_button_release(event)?,
            _ => {}
        }
        Ok(())
    }
}
