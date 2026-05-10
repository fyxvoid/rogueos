use anyhow::Result;
use std::collections::HashMap;
use x11rb::rust_connection::RustConnection;
use x11rb::protocol::xproto::Window;

use crate::roguewm::atoms::Atoms;
use crate::roguewm::config::*;
use crate::roguewm::cursors::Cursors;
use crate::roguewm::monitor::Monitor;
use crate::roguewm::client::Client;

pub mod state;
pub mod control;
pub mod helpers;

pub struct WindowManager<'a> {
    pub conn: &'a RustConnection,
    pub screen_num: usize,
    pub(crate) monitors: Vec<Monitor>,
    pub(crate) sel_mon: usize, // Index of selected monitor
    pub(crate) keybindings: Vec<KeyBinding>,
    pub(crate) atoms: Atoms,
    #[allow(dead_code)]
    pub(crate) cursors: Cursors,
    pub(crate) status_text: String,
    // Global client map mapping Window -> Client data (independent of monitor)
    pub clients: HashMap<Window, Client>,
    // Mouse Dragging State
    pub(crate) drag_mode: DragMode,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum DragMode {
    None,
    Moving(Window, i16, i16, i16, i16), // Window, start_mx, start_my, win_x, win_y
    Resizing(Window, i16, i16, u16, u16), // Window, start_mx, start_my, win_w, win_h
}

impl<'a> WindowManager<'a> {
    pub fn new(conn: &'a RustConnection, screen_num: usize) -> Result<Self> {
        state::init_wm(conn, screen_num)
    }
}
