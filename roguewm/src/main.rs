use roguewm::compositor;
#[allow(unused_imports)]
use roguewm::layout;
#[allow(dead_code)]
// mod manager; // Legacy X11 code, to be ported

use anyhow::Result;
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::Display;
use log::info;
use compositor::state::RogueState;

fn main() -> Result<()> {
    if let Ok(env) = std::env::var("RUST_LOG") {
        std::env::set_var("RUST_LOG", env);
    } else {
        std::env::set_var("RUST_LOG", "info,smithay=info");
    }
    env_logger::init();

    info!("Starting RogueWM Compositor...");

    let mut event_loop: EventLoop<RogueState> = EventLoop::try_new()?;
    let mut display: Display = Display::new(); // Assuming new() returns Display, not Result
    
    let mut state = RogueState::new(&mut event_loop, &mut display);

    // For now, run Winit backend
    #[cfg(feature = "backend_winit")]
    crate::compositor::winit::run_winit(&mut event_loop, &mut display, &mut state)?;

    event_loop.run(std::time::Duration::from_millis(16), &mut state, |state| {
        // state.xdg_shell_state.lock().unwrap().cleanup(); 
        display.flush_clients(&mut *state);
    })?;
    Ok(())
}
