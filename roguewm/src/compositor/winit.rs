use anyhow::Result;
use smithay::{
    backend::{
        renderer::{
            // damage::DamageTrackedRenderer,
            gles2,
        },
        winit::{self},
    },
    reexports::{
        calloop::EventLoop,
        wayland_server::Display,
    },
};


use smithay::reexports::calloop::timer::Timer;
use smithay::backend::input::InputBackend;

use crate::compositor::state::RogueState;

pub fn run_winit(
    event_loop: &mut EventLoop<RogueState>,
    _display: &mut Display,
    state: &mut RogueState,
) -> Result<()> {
    let (graphics_backend, mut input_backend) = winit::init(None)
        .map_err(|e| anyhow::anyhow!("Failed into initialize winit backend: {}", e))?;

    state.backend = Some(graphics_backend);

    // Winit backend in 0.3 might not implement EventSource.
    // Use a timer to poll events (common workaround for winit-on-calloop).
    // Use Timer::new() for calloop 0.9
    let timer: Timer<()> = Timer::new().unwrap();
    
    let timer_handle = timer.handle();
    
    // We need to trigger the first tick? 
    timer_handle.add_timeout(std::time::Duration::from_millis(16), ());

    event_loop.handle().insert_source(timer, move |_, _, state| {
         input_backend.dispatch_new_events(|event| {
             // Handle input
         }).unwrap();
         
         if let Some(backend) = state.backend.as_mut() {
             backend.render(|renderer, frame| {
                 use smithay::backend::renderer::{Frame, ImportShm};
                 frame.clear([0.1, 0.1, 0.1, 1.0]).unwrap();
                 
                 for window in &state.windows {
                     if let Some(buffer) = &window.buffer {
                         match renderer.import_shm_buffer(buffer, None, &[]) {
                             Ok(texture) => {
                                 let x = window.x as f64;
                                 let y = window.y as f64;
                                 frame.render_texture_at(
                                     &texture,
                                     (x, y).into(),
                                     1, // scale
                                     1.0, // output scale
                                     smithay::backend::renderer::Transform::Normal,
                                     1.0, // alpha
                                 ).unwrap();
                             },
                             Err(e) => {
                                 log::error!("Failed to import buffer: {}", e);
                             }
                         }
                     }
                 }
             }).unwrap();
         }

         // Reschedule
         timer_handle.add_timeout(std::time::Duration::from_millis(16), ());
    }).map_err(|e| anyhow::anyhow!("Failed timer: {}", e))?;

    Ok(())
}

impl RogueState {
    // Input processing stubs
}
