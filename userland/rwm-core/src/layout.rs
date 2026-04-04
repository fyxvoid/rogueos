//! Layout system — trait-based replacement for dwm's function-pointer layouts.
//!
//! Each layout algorithm is a struct that implements the [`Layout`] trait.
//! This replaces the C pattern of `void (*arrange)(Monitor *)` function pointers.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use crate::client::{Client, ClientId};
use crate::monitor::Monitor;
use crate::Rect;

/// Stable layout identifier (index into the layout registry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LayoutId(pub usize);

/// Result of a layout arrangement — maps each client to its target geometry.
pub type Arrangement = Vec<(ClientId, Rect)>;

/// The core layout trait. Every tiling algorithm implements this.
pub trait Layout: Send + Sync {
    /// Human-readable symbol (e.g. "[]=", "[M]", "[@]").
    fn symbol(&self) -> &str;

    /// Compute the arrangement of visible tiled clients within the given area.
    ///
    /// # Arguments
    /// * `mon` — the monitor providing mfact, nmaster, gap values
    /// * `clients` — slice of `(ClientId, &Client)` pairs for visible, non-floating clients
    /// * `area` — usable window area (already excludes bar)
    fn arrange(
        &self,
        mon: &Monitor,
        clients: &[(ClientId, &Client)],
        area: Rect,
    ) -> Arrangement;

    /// Short name for config files.
    fn name(&self) -> &str;
}

// ── Effective gaps helper ────────────────────────────────────────────

/// Compute effective gap values respecting smart_gaps (no outer gap with 1 client).
pub fn effective_gaps(
    mon: &Monitor,
    client_count: usize,
    smart_gaps: bool,
) -> (i32, i32, i32, i32) {
    if client_count == 0 {
        return (0, 0, 0, 0);
    }
    let (oh, ov, ih, iv) = (
        mon.gap_outer_h,
        mon.gap_outer_v,
        mon.gap_inner_h,
        mon.gap_inner_v,
    );
    if smart_gaps && client_count == 1 {
        (0, 0, 0, 0) // no gaps for single window
    } else {
        (oh, ov, ih, iv)
    }
}

/// Compute cfact-weighted sizes for a list of clients sharing a dimension.
///
/// Returns a vec of pixel sizes that sum to `total_pixels`, weighted by each
/// client's `cfact` value.
pub fn cfact_sizes(clients: &[&Client], total_pixels: i32) -> Vec<i32> {
    if clients.is_empty() {
        return Vec::new();
    }
    let total_cfact: f32 = clients.iter().map(|c| c.cfact).sum();
    if total_cfact <= 0.0 {
        let each = total_pixels / clients.len() as i32;
        return vec![each; clients.len()];
    }
    let mut sizes = Vec::with_capacity(clients.len());
    let mut used = 0i32;
    for (i, c) in clients.iter().enumerate() {
        if i == clients.len() - 1 {
            // Last client gets the remainder to avoid rounding drift
            sizes.push(total_pixels - used);
        } else {
            let s = ((c.cfact / total_cfact) * total_pixels as f32) as i32;
            sizes.push(s);
            used += s;
        }
    }
    sizes
}

// ══════════════════════════════════════════════════════════════════════
//  Built-in Layout: Tile (master/stack)
// ══════════════════════════════════════════════════════════════════════

/// Classic master/stack tiling — port of dwm's `tile()` + vanitygaps.
pub struct TileLayout {
    pub smart_gaps: bool,
}

impl Layout for TileLayout {
    fn symbol(&self) -> &str { "[]=" }
    fn name(&self) -> &str { "tile" }

    fn arrange(
        &self,
        mon: &Monitor,
        clients: &[(ClientId, &Client)],
        area: Rect,
    ) -> Arrangement {
        let n = clients.len();
        if n == 0 {
            return Vec::new();
        }

        let (oh, ov, ih, iv) = effective_gaps(mon, n, self.smart_gaps);
        let mut result = Vec::with_capacity(n);

        let nm = (mon.nmaster as usize).min(n);
        let ns = n - nm; // stack count

        // Effective area after outer gaps
        let ax = area.x + ov;
        let ay = area.y + oh;
        let aw = area.w as i32 - 2 * ov;
        let ah = area.h as i32 - 2 * oh;

        // Master width, stack width
        let mw = if nm == 0 || ns == 0 {
            aw
        } else {
            (aw as f32 * mon.mfact) as i32
        };
        let sw = aw - mw - if ns > 0 && nm > 0 { iv } else { 0 };

        // Master clients
        let master_clients: Vec<&Client> = clients[..nm].iter().map(|(_, c)| *c).collect();
        let master_sizes = cfact_sizes(&master_clients, ah - ih * (nm as i32 - 1).max(0));
        let mut my = ay;
        for (i, &(cid, _)) in clients[..nm].iter().enumerate() {
            let h = master_sizes[i];
            result.push((cid, Rect::new(ax, my, mw.max(1) as u32, h.max(1) as u32)));
            my += h + ih;
        }

        // Stack clients
        if ns > 0 {
            let sx = ax + mw + if nm > 0 { iv } else { 0 };
            let stack_clients: Vec<&Client> = clients[nm..].iter().map(|(_, c)| *c).collect();
            let stack_sizes = cfact_sizes(&stack_clients, ah - ih * (ns as i32 - 1).max(0));
            let mut sy = ay;
            for (i, &(cid, _)) in clients[nm..].iter().enumerate() {
                let h = stack_sizes[i];
                result.push((cid, Rect::new(sx, sy, sw.max(1) as u32, h.max(1) as u32)));
                sy += h + ih;
            }
        }

        result
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Built-in Layout: Monocle
// ══════════════════════════════════════════════════════════════════════

/// All clients occupy the full area (stacked, only the focused one visible).
pub struct MonocleLayout;

impl Layout for MonocleLayout {
    fn symbol(&self) -> &str { "[M]" }
    fn name(&self) -> &str { "monocle" }

    fn arrange(
        &self,
        _mon: &Monitor,
        clients: &[(ClientId, &Client)],
        area: Rect,
    ) -> Arrangement {
        clients
            .iter()
            .map(|&(cid, _)| (cid, area))
            .collect()
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Built-in Layout: Spiral (Fibonacci)
// ══════════════════════════════════════════════════════════════════════

/// Fibonacci spiral layout — port of vanitygaps `fibonacci(m, 0)`.
pub struct SpiralLayout {
    pub smart_gaps: bool,
}

impl Layout for SpiralLayout {
    fn symbol(&self) -> &str { "[@]" }
    fn name(&self) -> &str { "spiral" }

    fn arrange(
        &self,
        mon: &Monitor,
        clients: &[(ClientId, &Client)],
        area: Rect,
    ) -> Arrangement {
        fibonacci_arrange(mon, clients, area, false, self.smart_gaps)
    }
}

/// Dwindle layout — port of vanitygaps `fibonacci(m, 1)`.
pub struct DwindleLayout {
    pub smart_gaps: bool,
}

impl Layout for DwindleLayout {
    fn symbol(&self) -> &str { "[\\]" }
    fn name(&self) -> &str { "dwindle" }

    fn arrange(
        &self,
        mon: &Monitor,
        clients: &[(ClientId, &Client)],
        area: Rect,
    ) -> Arrangement {
        fibonacci_arrange(mon, clients, area, true, self.smart_gaps)
    }
}

fn fibonacci_arrange(
    mon: &Monitor,
    clients: &[(ClientId, &Client)],
    area: Rect,
    dwindle: bool,
    smart_gaps: bool,
) -> Arrangement {
    let n = clients.len();
    if n == 0 {
        return Vec::new();
    }

    let (oh, ov, ih, iv) = effective_gaps(mon, n, smart_gaps);
    let mut result = Vec::with_capacity(n);

    let mut cx = area.x + ov;
    let mut cy = area.y + oh;
    let mut cw = area.w as i32 - 2 * ov;
    let mut ch = area.h as i32 - 2 * oh;

    for (i, &(cid, _)) in clients.iter().enumerate() {
        if i < n - 1 {
            if (if dwindle { i } else { i + 1 }) % 2 == 0 {
                // horizontal split
                let half = (ch - ih) / 2;
                result.push((cid, Rect::new(cx, cy, cw.max(1) as u32, half.max(1) as u32)));
                cy += half + ih;
                ch -= half + ih;
            } else {
                // vertical split
                let half = (cw - iv) / 2;
                result.push((cid, Rect::new(cx, cy, half.max(1) as u32, ch.max(1) as u32)));
                cx += half + iv;
                cw -= half + iv;
            }
        } else {
            result.push((cid, Rect::new(cx, cy, cw.max(1) as u32, ch.max(1) as u32)));
        }
    }

    result
}

// ══════════════════════════════════════════════════════════════════════
//  Built-in Layout: Bottom Stack
// ══════════════════════════════════════════════════════════════════════

/// Master on top, stack on bottom — port of vanitygaps `bstack()`.
pub struct BStackLayout {
    pub smart_gaps: bool,
}

impl Layout for BStackLayout {
    fn symbol(&self) -> &str { "TTT" }
    fn name(&self) -> &str { "bstack" }

    fn arrange(
        &self,
        mon: &Monitor,
        clients: &[(ClientId, &Client)],
        area: Rect,
    ) -> Arrangement {
        let n = clients.len();
        if n == 0 {
            return Vec::new();
        }

        let (oh, ov, ih, iv) = effective_gaps(mon, n, self.smart_gaps);
        let mut result = Vec::with_capacity(n);

        let nm = (mon.nmaster as usize).min(n);
        let ns = n - nm;

        let ax = area.x + ov;
        let ay = area.y + oh;
        let aw = area.w as i32 - 2 * ov;
        let ah = area.h as i32 - 2 * oh;

        // Master height, stack height
        let mh = if nm == 0 || ns == 0 {
            ah
        } else {
            (ah as f32 * mon.mfact) as i32
        };
        let sh = ah - mh - if ns > 0 && nm > 0 { ih } else { 0 };

        // Master row (horizontal)
        let master_clients: Vec<&Client> = clients[..nm].iter().map(|(_, c)| *c).collect();
        let master_sizes = cfact_sizes(&master_clients, aw - iv * (nm as i32 - 1).max(0));
        let mut mx = ax;
        for (i, &(cid, _)) in clients[..nm].iter().enumerate() {
            let w = master_sizes[i];
            result.push((cid, Rect::new(mx, ay, w.max(1) as u32, mh.max(1) as u32)));
            mx += w + iv;
        }

        // Stack row (horizontal, below master)
        if ns > 0 {
            let sy = ay + mh + if nm > 0 { ih } else { 0 };
            let stack_clients: Vec<&Client> = clients[nm..].iter().map(|(_, c)| *c).collect();
            let stack_sizes = cfact_sizes(&stack_clients, aw - iv * (ns as i32 - 1).max(0));
            let mut sx = ax;
            for (i, &(cid, _)) in clients[nm..].iter().enumerate() {
                let w = stack_sizes[i];
                result.push((cid, Rect::new(sx, sy, w.max(1) as u32, sh.max(1) as u32)));
                sx += w + iv;
            }
        }

        result
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Built-in Layout: Grid
// ══════════════════════════════════════════════════════════════════════

/// Square grid — port of vanitygaps `grid()`.
pub struct GridLayout {
    pub smart_gaps: bool,
}

impl Layout for GridLayout {
    fn symbol(&self) -> &str { "HHH" }
    fn name(&self) -> &str { "grid" }

    fn arrange(
        &self,
        mon: &Monitor,
        clients: &[(ClientId, &Client)],
        area: Rect,
    ) -> Arrangement {
        let n = clients.len();
        if n == 0 {
            return Vec::new();
        }

        let (oh, ov, ih, iv) = effective_gaps(mon, n, self.smart_gaps);
        let mut result = Vec::with_capacity(n);

        // Calculate grid dimensions
        let mut rows = 1usize;
        while rows * rows < n {
            rows += 1;
        }
        let cols = if rows > 0 { n.div_ceil(rows) } else { 1 };

        let ax = area.x + ov;
        let ay = area.y + oh;
        let aw = area.w as i32 - 2 * ov;
        let ah = area.h as i32 - 2 * oh;

        let cw = (aw - iv * (cols as i32 - 1)) / cols as i32;
        let ch = (ah - ih * (rows as i32 - 1)) / rows as i32;

        for (i, &(cid, _)) in clients.iter().enumerate() {
            let row = i / cols;
            let col = i % cols;
            let x = ax + col as i32 * (cw + iv);
            let y = ay + row as i32 * (ch + ih);
            result.push((cid, Rect::new(x, y, cw.max(1) as u32, ch.max(1) as u32)));
        }

        result
    }
}

// ══════════════════════════════════════════════════════════════════════
//  Built-in Layout: Centered Master
// ══════════════════════════════════════════════════════════════════════

/// Master centered, stack on left and right — port of `centeredmaster()`.
pub struct CenteredMasterLayout {
    pub smart_gaps: bool,
}

impl Layout for CenteredMasterLayout {
    fn symbol(&self) -> &str { "|M|" }
    fn name(&self) -> &str { "centeredmaster" }

    fn arrange(
        &self,
        mon: &Monitor,
        clients: &[(ClientId, &Client)],
        area: Rect,
    ) -> Arrangement {
        let n = clients.len();
        if n == 0 {
            return Vec::new();
        }

        let (oh, ov, ih, iv) = effective_gaps(mon, n, self.smart_gaps);
        let mut result = Vec::with_capacity(n);

        let nm = (mon.nmaster as usize).min(n);
        let ns = n - nm;

        let ax = area.x + ov;
        let ay = area.y + oh;
        let aw = area.w as i32 - 2 * ov;
        let ah = area.h as i32 - 2 * oh;

        if ns == 0 {
            // No stack — master fills all
            let master_clients: Vec<&Client> = clients.iter().map(|(_, c)| *c).collect();
            let sizes = cfact_sizes(&master_clients, ah - ih * (n as i32 - 1).max(0));
            let mut my = ay;
            for (i, &(cid, _)) in clients.iter().enumerate() {
                let h = sizes[i];
                result.push((cid, Rect::new(ax, my, aw.max(1) as u32, h.max(1) as u32)));
                my += h + ih;
            }
        } else {
            let mw = (aw as f32 * mon.mfact) as i32;
            let side_w = (aw - mw - 2 * iv) / 2;
            let mx = ax + side_w + iv;

            // Master column (center)
            let master_clients: Vec<&Client> = clients[..nm].iter().map(|(_, c)| *c).collect();
            let m_sizes = cfact_sizes(&master_clients, ah - ih * (nm as i32 - 1).max(0));
            let mut my = ay;
            for (i, &(cid, _)) in clients[..nm].iter().enumerate() {
                let h = m_sizes[i];
                result.push((cid, Rect::new(mx, my, mw.max(1) as u32, h.max(1) as u32)));
                my += h + ih;
            }

            // Stack (split left/right)
            let left_count = ns.div_ceil(2);
            let right_count = ns / 2;

            // Left stack
            if left_count > 0 {
                let left_clients: Vec<&Client> = clients[nm..]
                    .iter()
                    .step_by(2)
                    .map(|(_, c)| *c)
                    .collect();
                let l_sizes = cfact_sizes(&left_clients, ah - ih * (left_count as i32 - 1).max(0));
                let mut ly = ay;
                let mut li = 0;
                for (i, &(cid, _)) in clients[nm..].iter().enumerate() {
                    if i % 2 == 0 {
                        let h = l_sizes[li];
                        result.push((cid, Rect::new(ax, ly, side_w.max(1) as u32, h.max(1) as u32)));
                        ly += h + ih;
                        li += 1;
                    }
                }
            }

            // Right stack
            if right_count > 0 {
                let rx = mx + mw + iv;
                let right_clients: Vec<&Client> = clients[nm..]
                    .iter()
                    .skip(1)
                    .step_by(2)
                    .map(|(_, c)| *c)
                    .collect();
                let r_sizes = cfact_sizes(&right_clients, ah - ih * (right_count as i32 - 1).max(0));
                let mut ry = ay;
                let mut ri = 0;
                for (i, &(cid, _)) in clients[nm..].iter().enumerate() {
                    if i % 2 == 1 {
                        let h = r_sizes[ri];
                        result.push((cid, Rect::new(rx, ry, side_w.max(1) as u32, h.max(1) as u32)));
                        ry += h + ih;
                        ri += 1;
                    }
                }
            }
        }

        result
    }
}

// ── Layout registry ──────────────────────────────────────────────────

/// All built-in layouts, ready to register.
pub fn builtin_layouts(smart_gaps: bool) -> Vec<Box<dyn Layout>> {
    vec![
        Box::new(TileLayout { smart_gaps }),
        Box::new(MonocleLayout),
        Box::new(SpiralLayout { smart_gaps }),
        Box::new(DwindleLayout { smart_gaps }),
        Box::new(BStackLayout { smart_gaps }),
        Box::new(GridLayout { smart_gaps }),
        Box::new(CenteredMasterLayout { smart_gaps }),
    ]
}
