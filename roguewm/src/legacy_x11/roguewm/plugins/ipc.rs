use crate::roguewm::plugin::Plugin;
use crate::roguewm::manager::WindowManager;
use crate::roguewm::config::{LayoutSymbol, Action}; 
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{ClientMessageEvent, EventMask, ConnectionExt};
use x11rb::connection::Connection;
use anyhow::Result;
use log::{info, error};
use std::os::unix::net::UnixListener;
use std::io::{BufRead, BufReader};
use std::thread;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

// Shared queue for IPC commands
type CommandQueue = Arc<Mutex<VecDeque<String>>>;

pub struct IpcPlugin {
    queue: CommandQueue,
}

impl IpcPlugin {
    pub fn new(conn: &impl Connection, screen_num: usize) -> Result<Self> {
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        
        let path = "/tmp/roguewm.sock";
        if std::fs::metadata(path).is_ok() {
            std::fs::remove_file(path)?;
        }
        
        let listener = UnixListener::bind(path)?;
        let queue: CommandQueue = Arc::new(Mutex::new(VecDeque::new()));
        let queue_clone = queue.clone();
        
        // We need a way to wake up the main loop.
        // We can send a ClientMessage to the root window.
        // But we need a connection to do that. 
        // We can create a new connection in the thread? Or pass a clone?
        // RustConnection is cheaply cloneable? No, it wraps an XCB connection.
        // x11rb::rust_connection::RustConnection::connect(None) in the thread is safest.
        
        thread::spawn(move || {
            info!("[IpcPlugin] Listening on {}", path);
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let mut reader = BufReader::new(stream);
                        let mut line = String::new();
                        if let Ok(_) = reader.read_line(&mut line) {
                            let cmd = line.trim().to_string();
                            info!("[IpcPlugin] Received command: {}", cmd);
                            
                            // Push to queue
                            {
                                let mut q = queue_clone.lock().unwrap();
                                q.push_back(cmd);
                            }
                            
                            // Wake up main thread
                            // Open separate connection for thread
                            if let Ok((conn, _)) = x11rb::rust_connection::RustConnection::connect(None) {
                                // Send ClientMessage
                                // Format: 32, window=root, type=ATOM_NULL (or specific), data=...
                                // We just need ANY event to wake wait_for_event()
                                let event = ClientMessageEvent {
                                    response_type: x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT,
                                    format: 32,
                                    sequence: 0,
                                    window: root,
                                    type_: x11rb::protocol::xproto::AtomEnum::INTEGER.into(), // Arbitrary
                                    data: [0, 0, 0, 0, 0].into(),
                                };
                                
                                conn.send_event(false, root, EventMask::NO_EVENT, event).ok();
                                conn.flush().ok();
                            }
                        }
                    }
                    Err(e) => error!("[IpcPlugin] Connection failed: {}", e),
                }
            }
        });

        Ok(Self {
            queue,
        })
    }
}

impl Plugin for IpcPlugin {
    fn name(&self) -> &str {
        "IPC"
    }

    fn on_event(&mut self, wm: &mut WindowManager, event: &Event) -> Result<bool> {
        // Check if we have commands in queue
        // We check on EVERY event loop wakeup.
        // If the wakeup was caused by our ClientMessage, this will run.
        // If it was another event, this will also run, which is fine.
        
        let cmd = {
            let mut q = self.queue.lock().unwrap();
            q.pop_front()
        };
        
        if let Some(command) = cmd {
            info!("[IpcPlugin] Executing: {}", command);
            let parts: Vec<&str> = command.split_whitespace().collect();
            match parts.as_slice() {
                ["layout", name] => {
                    match *name {
                        "tile" => wm.process_action(Action::SetLayout(LayoutSymbol::Tile))?,
                        "monocle" => wm.process_action(Action::SetLayout(LayoutSymbol::Monocle))?,
                        "floating" => wm.process_action(Action::SetLayout(LayoutSymbol::Floating))?,
                        "dwindle" => wm.process_action(Action::SetLayout(LayoutSymbol::Dwindle))?,
                        _ => error!("[IpcPlugin] Unknown layout: {}", name),
                    }
                },
                ["quit"] => wm.process_action(Action::Quit)?,
                ["focus", "next"] => wm.process_action(Action::FocusNext)?,
                ["focus", "prev"] => wm.process_action(Action::FocusPrev)?,
                ["gaps", "toggle"] => wm.process_action(Action::ToggleGaps)?,
                _ => error!("[IpcPlugin] Unrecognized command: {}", command),
            }
            // If it was a ClientMessage targeting us, we should technically consume it? 
            // But our 'type' was generic.
            if let Event::ClientMessage(e) = event {
                 if e.type_ == x11rb::protocol::xproto::AtomEnum::INTEGER.into() {
                     return Ok(true); // Consume our wakeup event
                 }
            }
        }
        
        Ok(false)
    }
}
