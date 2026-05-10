use crate::roguewm::plugin::Plugin;
use crate::roguewm::manager::WindowManager;
use x11rb::protocol::Event;
use anyhow::Result;
use log::info;

pub struct LoggerPlugin;

impl LoggerPlugin {
    pub fn new() -> Self {
        Self {}
    }
}

impl Plugin for LoggerPlugin {
    fn name(&self) -> &str {
        "Logger"
    }

    fn on_event(&mut self, _wm: &mut WindowManager, event: &Event) -> Result<bool> {
        // Log specific events to reduce noise if needed, or just all
        // debug!("LoggerPlugin: {:?}", event);
        match event {
            Event::MapRequest(e) => info!("[LoggerPlugin] MapRequest window: {}", e.window),
            Event::KeyPress(e) => info!("[LoggerPlugin] KeyPress detail: {}", e.detail),
            _ => {}
        }
        Ok(false) // Don't consume
    }
}
