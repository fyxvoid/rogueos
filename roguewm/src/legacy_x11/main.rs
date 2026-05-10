use anyhow::Result;
use x11rb::rust_connection::RustConnection;
use x11rb::connection::Connection;

mod roguewm;

use roguewm::manager::WindowManager;
use roguewm::plugin::Plugin;
use roguewm::plugins::logger::LoggerPlugin;

use roguewm::plugins::scratchpad::ScratchpadPlugin;

use roguewm::plugins::ipc::IpcPlugin;

fn main() -> Result<()> {
    env_logger::init();
    
    let (conn, screen_num) = RustConnection::connect(None)?;
    
    let mut wm = WindowManager::new(&conn, screen_num)?;
    wm.init()?; // Setup root window events
    
    // Plugin initialization
    let mut plugins: Vec<Box<dyn Plugin>> = Vec::new();
    plugins.push(Box::new(LoggerPlugin::new())); 
    plugins.push(Box::new(ScratchpadPlugin::new())); 
    plugins.push(Box::new(IpcPlugin::new(&conn, screen_num)?)); 

    loop {
        let event = wm.conn.wait_for_event()?;
        
        let mut handled = false;
        for plugin in &mut plugins {
            if plugin.on_event(&mut wm, &event)? {
                handled = true;
                break;
            }
        }
        
        if !handled {
            wm.handle_event(event)?;
        }
    }
}
