use super::geometry::{Rect, LayoutState, Gaps};

pub fn monocle(state: &LayoutState) -> Vec<Rect> {
    let n = state.client_count;
    if n == 0 { return Vec::new(); }
    
    let Gaps { oh: _oh, ov: _ov, .. } = state.gaps;
    let m = &state.container;
    
    let rect = Rect {
        x: m.x, 
        y: m.y, 
        width: m.width, 
        height: m.height,
    };
    
    vec![rect; n] // All clients get same rect (stacked)
}
