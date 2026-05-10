use x11rb::protocol::xproto::Window;
use crate::roguewm::config::LayoutSymbol;
use crate::roguewm::bar::Bar;
use super::layouts::{Gaps, Rect};

#[allow(dead_code)]
pub struct Monitor {
    pub num: usize,
    pub rect: Rect,         // Screen geometry (full)
    pub window_rect: Rect,  // Start with full, subtract bar height
    pub tags: u32,
    pub layout: LayoutSymbol,
    pub bar: Bar,
    pub nmaster: usize,
    pub mfact: f32,
    pub gaps: Gaps,
    pub show_gaps: bool,
    pub clients: Vec<Window>,       // Clients on this monitor (stack order)
    pub selected_client: Option<Window>,
}

impl Monitor {
    pub fn new(num: usize, rect: Rect, bar: Bar) -> Self {
        let window_rect = Rect {
            x: rect.x,
            y: rect.y + bar.height as i32,
            width: rect.width,
            height: rect.height.saturating_sub(bar.height as u32),
        };

        Self {
            num,
            rect,
            window_rect,
            tags: 1,
            layout: LayoutSymbol::Tile,
            bar,
            nmaster: 1,
            mfact: 0.55,
            gaps: Gaps::default(),
            show_gaps: true,
            clients: Vec::new(),
            selected_client: None,
        }
    }
}
