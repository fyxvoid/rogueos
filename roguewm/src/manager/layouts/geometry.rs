use crate::roguewm::config::*;

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub struct LayoutState {
    pub container: Rect,
    pub client_count: usize,
    pub nmaster: usize,
    pub mfact: f32,
    pub gaps: Gaps,
}

#[derive(Clone, Copy, Debug)]
pub struct Gaps {
    pub oh: i32, // outer horizontal
    pub ov: i32, // outer vertical
    pub ih: i32, // inner horizontal
    pub iv: i32, // inner vertical
}

impl Default for Gaps {
    fn default() -> Self {
        Self {
            oh: GAPP_OH,
            ov: GAPP_OV,
            ih: GAPP_IH,
            iv: GAPP_IV,
        }
    }
}
