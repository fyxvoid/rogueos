use super::{Bar, BarAction};

impl Bar {
    pub fn handle_click(&self, x: i16, _y: i16) -> BarAction {
        let mut current_x = 0;
        
        // Check Tags
        for i in 0..9 {
            if x >= current_x && x < current_x + Self::TAG_WIDTH {
                 return BarAction::Tag(i);
            }
            current_x += Self::TAG_WIDTH;
        }
        
        // Check Layout
        if x >= current_x && x < current_x + Self::LAYOUT_WIDTH {
            return BarAction::Layout;
        }
        
        BarAction::None
    }
}
