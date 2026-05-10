use x11rb::protocol::xproto::Window;

#[derive(Clone, Debug)]
pub struct Client {
    #[allow(dead_code)]
    pub window: Window,
    pub tags: u32,
    pub is_floating: bool,
    pub class: String,
    // Store size hints/floating rect if needed
}

impl Client {
    #[allow(dead_code)]
    pub fn new(window: Window, tags: u32, is_floating: bool, class: String) -> Self {
        Self {
            window,
            tags,
            is_floating,
            class,
        }
    }
}
