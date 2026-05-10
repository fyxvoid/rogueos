use super::manager::WindowManager;
use x11rb::protocol::xproto::Window;
use x11rb::protocol::Event;
use anyhow::Result;

#[allow(dead_code)]
pub trait Plugin {
    fn name(&self) -> &str;
    
    // Return true if the event was consumed/handled and should not be processed by WM
    fn on_event(&mut self, _wm: &mut WindowManager, _event: &Event) -> Result<bool> { Ok(false) }
    
    // Hook called after WM manages a window
    fn on_manage(&mut self, _wm: &mut WindowManager, _window: Window) -> Result<()> { Ok(()) }
    
    // Hook called before unmanaging
    fn on_unmanage(&mut self, _wm: &mut WindowManager, _window: Window) -> Result<()> { Ok(()) }
}
