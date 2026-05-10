
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u32);

#[derive(Debug, Clone, Copy)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct Workspace {
    pub windows: Vec<WindowId>,
    pub master_ratio: f32, // 0.05 to 0.95
    pub geometry: Geometry, // Screen dimensions
}

impl Workspace {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            windows: Vec::new(),
            master_ratio: 0.55,
            geometry: Geometry { x: 0, y: 0, width, height },
        }
    }

    pub fn add_window(&mut self, id: WindowId) {
        self.windows.push(id);
    }
}

pub fn tile(workspace: &Workspace) -> Vec<(WindowId, Geometry)> {
    let window_count = workspace.windows.len();
    if window_count == 0 {
        return Vec::new();
    }

    let g = workspace.geometry;
    let mut results = Vec::new();

    if window_count == 1 {
        results.push((workspace.windows[0], g));
        return results;
    }

    let master_width = (g.width as f32 * workspace.master_ratio) as u32;
    let stack_width = g.width - master_width;
    let stack_rows = (window_count - 1) as u32;
    let stack_height = g.height / stack_rows;

    // Master window
    results.push((
        workspace.windows[0],
        Geometry {
            x: g.x,
            y: g.y,
            width: master_width,
            height: g.height,
        },
    ));

    // Stack windows
    for (i, window_id) in workspace.windows.iter().skip(1).enumerate() {
        let y_offset = g.y + (i as u32 * stack_height) as i32;
        // Adjust height for last window to fill remaining space if division wasn't clean
        let height = if i == (stack_rows - 1) as usize {
            g.height - (i as u32 * stack_height)
        } else {
            stack_height
        };

        results.push((
            *window_id,
            Geometry {
                x: g.x + master_width as i32,
                y: y_offset,
                width: stack_width,
                height,
            },
        ));
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_window_fills_screen() {
        let mut ws = Workspace::new(1920, 1080);
        ws.add_window(WindowId(1));

        let res = tile(&ws);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].1.width, 1920);
        assert_eq!(res[0].1.height, 1080);
    }

    #[test]
    fn test_two_windows_split() {
        let mut ws = Workspace::new(1000, 1000);
        ws.add_window(WindowId(1));
        ws.add_window(WindowId(2));
        ws.master_ratio = 0.5;

        let res = tile(&ws);
        assert_eq!(res.len(), 2);
        
        // Master
        assert_eq!(res[0].1.width, 500);
        assert_eq!(res[0].1.height, 1000); // Full height
        
        // Stack
        assert_eq!(res[1].1.x, 500);
        assert_eq!(res[1].1.width, 500);
        assert_eq!(res[1].1.height, 1000);
    }
}
