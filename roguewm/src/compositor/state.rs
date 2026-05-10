use std::sync::{Arc, Mutex};
use smithay::{
    reexports::{
        calloop::{EventLoop},
        wayland_server::{Display, Global, DispatchData, protocol::{wl_surface::WlSurface, wl_compositor, wl_subcompositor, wl_shm}},
        wayland_protocols::xdg_shell::server::xdg_wm_base::XdgWmBase,
    },
    wayland::{
        compositor,
        shm,
        seat::Seat,
        shell::xdg::{self, XdgRequest, ShellState},
    },
};

pub struct RogueState {
    pub windows: Vec<crate::compositor::window::WindowElement>, 
    
    pub compositor_global: Global<wl_compositor::WlCompositor>,
    pub subcompositor_global: Global<wl_subcompositor::WlSubcompositor>,
    pub shm_global: Global<wl_shm::WlShm>,
    pub xdg_shell_state: Arc<Mutex<ShellState>>,
    pub xdg_shell_global: Global<XdgWmBase>,
    pub seat: Seat,
    
    pub backend: Option<smithay::backend::winit::WinitGraphicsBackend>,
    
    pub start_time: std::time::Instant,
}

impl RogueState {
    pub fn new(
        _event_loop: &mut EventLoop<RogueState>,
        display: &mut Display,
    ) -> Self {
        let (compositor_global, subcompositor_global) = compositor::compositor_init(
            display,
            |surface, mut dispatch_data| {
                if let Some(state) = dispatch_data.get::<RogueState>() {
                    if let Some(window) = state.windows.iter_mut().find(|w| w.surface == surface) {
                        compositor::with_states(&surface, |states| {
                            let mut attributes = states.cached_state.current::<compositor::SurfaceAttributes>();
                            if let Some(assignment) = attributes.buffer.take() {
                                if let compositor::BufferAssignment::NewBuffer { buffer, .. } = assignment {
                                    window.buffer = Some(buffer);
                                } else {
                                    window.buffer = None;
                                }
                            }
                        }).unwrap();
                    }
                }
            },
            None,
        );

        let shm_global = shm::init_shm_global(
            display,
            vec![], 
            None,
        );

        let (xdg_shell_state, xdg_shell_global, _) = xdg::xdg_shell_init(
            display,
            |request, mut dispatch_data| {
                match request {
                    XdgRequest::NewToplevel { surface } => {
                        surface.send_configure();
                        if let Some(state) = dispatch_data.get::<RogueState>() {
                            state.windows.push(crate::compositor::window::WindowElement {
                                surface: surface.get_surface().unwrap().clone(),
                                buffer: None,
                                x: 0,
                                y: 0,
                            });
                        }
                    },
                    _ => {}
                }
            },
            None,
        );

        let (seat, _) = Seat::new(
            display,
            "rogue-seat".into(),
            None,
        );

        Self {
            windows: Vec::new(),
            compositor_global,
            subcompositor_global,
            shm_global,
            xdg_shell_state,
            xdg_shell_global,
            seat,
            backend: None,
            start_time: std::time::Instant::now(),
        }
    }
}
