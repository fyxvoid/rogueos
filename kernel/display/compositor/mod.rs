//! Minimal compositor: single tiling layout only.
//! 1 window = full screen; 2 = vertical split; 3+ = even tiling.
//! No floating, decorations, transparency, shadows, animations, themes, plugins, scripting.
//! No GPU logic here; consumes display_server and input events. See plan Section 9.

/// Compute tile layout: (x, y, w, h) for each window. 1=fullscreen, 2=vertical split, 3+=even.
#[inline]
pub fn tile_layout(screen_w: u32, screen_h: u32, num_windows: usize) -> [(u32, u32, u32, u32); 8] {
    let mut out = [(0u32, 0u32, 0u32, 0u32); 8];
    if num_windows == 0 {
        return out;
    }
    if num_windows == 1 {
        out[0] = (0, 0, screen_w, screen_h);
        return out;
    }
    if num_windows == 2 {
        let w = screen_w / 2;
        out[0] = (0, 0, w, screen_h);
        out[1] = (w, 0, screen_w - w, screen_h);
        return out;
    }
    // 3+: even tiling (simplified: row of tiles). Last tile takes remainder so no overlap.
    let n = num_windows.min(8);
    let tile_w = screen_w / (n as u32);
    for i in 0..n {
        let x = i as u32 * tile_w;
        let w = if i + 1 == n {
            screen_w.saturating_sub(x)
        } else {
            tile_w
        };
        out[i] = (x, 0, w, screen_h);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiling splits evenly; no overlap; no gap at right.
    #[test]
    fn tile_layout_no_overlap() {
        let (w, h) = (1920u32, 1080u32);
        for n in 1..=8 {
            let layout = tile_layout(w, h, n);
            let mut right = 0u32;
            for i in 0..n {
                let (x, _y, tw, th) = layout[i];
                assert!(tw > 0 && th > 0, "tile {} zero size", i);
                assert!(x >= right, "tile {} overlaps previous", i);
                right = x + tw;
            }
            assert_eq!(right, w, "n={} layout does not cover width", n);
        }
    }
}
