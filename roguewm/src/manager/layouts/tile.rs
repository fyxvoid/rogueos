use super::geometry::{Rect, LayoutState, Gaps};

pub fn tile(state: &LayoutState) -> Vec<Rect> {
    let n = state.client_count;
    if n == 0 { return Vec::new(); }

    let mut rects = Vec::with_capacity(n);
    let Gaps { oh, ov, ih, iv } = state.gaps;
    let m = &state.container;
    
    let mx = m.x + ov;
    let my = m.y + oh;
    let mh = m.height as i32 - 2 * oh; // Total available height
    let mw = m.width as i32 - 2 * ov;  // Total available width
    
    let nmaster = state.nmaster;
    let effective_nmaster = std::cmp::min(n, nmaster);
    
    if nmaster > 0 && n > nmaster {
        let master_width = ((mw - iv) as f32 * state.mfact) as i32;
        let stack_width = mw - master_width - iv;
        
        let master_x = mx;
        let stack_x = mx + master_width + iv;
        
        // Master area layout
        let h_master_total = mh - ih * (effective_nmaster as i32 - 1);
        let h_master_each = if effective_nmaster > 0 { h_master_total / effective_nmaster as i32 } else { 0 };
        
        for i in 0..effective_nmaster {
            rects.push(Rect {
                x: master_x,
                y: my + i as i32 * (h_master_each + ih),
                width: master_width as u32,
                height: h_master_each as u32,
            });
        }
        
        // Stack area layout
        let nstack = n - nmaster;
        let h_stack_total = mh - ih * (nstack as i32 - 1);
        let h_stack_each = if nstack > 0 { h_stack_total / nstack as i32 } else { 0 };
        
        for i in 0..nstack {
            rects.push(Rect {
                x: stack_x,
                y: my + i as i32 * (h_stack_each + ih),
                width: stack_width as u32,
                height: h_stack_each as u32,
            });
        }
        
    } else {
        // Only master (vertical stack)
        let h_total = mh - ih * (n as i32 - 1);
        let h_each = if n > 0 { h_total / n as i32 } else { 0 };
        
        for i in 0..n {
            rects.push(Rect {
                x: mx,
                y: my + i as i32 * (h_each + ih),
                width: mw as u32,
                height: h_each as u32,
            });
        }
    }
    
    rects
}
