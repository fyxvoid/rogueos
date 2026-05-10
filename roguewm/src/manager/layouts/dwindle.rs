use super::geometry::{Rect, LayoutState, Gaps};

pub fn dwindle(state: &LayoutState) -> Vec<Rect> {
    let n = state.client_count;
    if n == 0 { return Vec::new(); }

    let mut rects = Vec::with_capacity(n);
    let Gaps { oh, ov, ih, iv } = state.gaps;
    let m = &state.container;
    
    // Initial area
    let mut x = m.x + ov;
    let mut y = m.y + oh;
    let mut w = (m.width as i32 - 2 * ov) as u32;
    let mut h = (m.height as i32 - 2 * oh) as u32;

    for i in 0..n {
        if i == n - 1 {
            // Last client takes remaining space
            rects.push(Rect { x, y, width: w, height: h });
        } else {
            if i % 2 == 0 {
                // Split width
                let split_w = (w as i32 - iv) / 2;
                let rem_w = w as i32 - split_w - iv;
                
                rects.push(Rect { x, y, width: split_w as u32, height: h });
                
                x += split_w + iv;
                w = rem_w as u32;
            } else {
                // Split height
                let split_h = (h as i32 - ih) / 2;
                let rem_h = h as i32 - split_h - ih;
                
                rects.push(Rect { x, y, width: w, height: split_h as u32 });
                
                y += split_h + ih;
                h = rem_h as u32;
            }
        }
    }
    
    rects
}
