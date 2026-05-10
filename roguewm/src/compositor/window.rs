use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;

pub struct WindowElement {
    pub surface: smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    pub buffer: Option<smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer>,
    pub x: i32,
    pub y: i32,
}
