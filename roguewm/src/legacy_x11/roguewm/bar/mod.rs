use x11rb::protocol::xproto::{Window, Font, Gcontext};

pub mod init;
pub mod draw;
pub mod input;

pub struct Bar {
    pub window: Window,
    #[allow(dead_code)]
    pub font: Font,
    pub gc: Gcontext,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Copy)]
pub enum BarAction {
    Tag(usize), // 0-8
    Layout,
    #[allow(dead_code)]
    Status,     // Maybe click status to generic action?
    None,
}
