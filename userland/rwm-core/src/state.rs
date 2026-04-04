//! Central WM state — the single source of truth.
//!
//! This is the Rust equivalent of dwm's collection of global variables
//! (`mons`, `selmon`, `scheme`, `running`, `stext`, etc.) consolidated into
//! a single owned struct for safe, explicit state management.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use slotmap::SlotMap;
use crate::client::{Client, ClientId};
use crate::event::EventBus;
use crate::layout::Layout;
use crate::monitor::{Monitor, MonitorId};
use crate::{Rect, TAGMASK, SCRATCHTAG, is_visible};

/// The entire window manager state.
pub struct WmState {
    // ── Client storage (generational arena) ─────────────────────────
    pub clients: SlotMap<ClientId, Client>,

    // ── Monitors ────────────────────────────────────────────────────
    pub monitors: Vec<Monitor>,
    /// Index of the currently active monitor.
    pub sel_mon: MonitorId,

    // ── Layouts ─────────────────────────────────────────────────────
    pub layouts: Vec<Box<dyn Layout>>,

    // ── Bar ─────────────────────────────────────────────────────────
    pub bar_height: u32,
    /// Status text (set by external status script via root WM_NAME).
    pub status_text: String,

    // ── Global state ────────────────────────────────────────────────
    pub running: bool,
    pub screen_w: u32,
    pub screen_h: u32,

    // ── Events ──────────────────────────────────────────────────────
    pub events: EventBus,
}

impl WmState {
    pub fn new() -> Self {
        Self {
            clients: SlotMap::with_key(),
            monitors: Vec::new(),
            sel_mon: 0,
            layouts: Vec::new(),
            bar_height: 0,
            status_text: String::new(),
            running: true,
            screen_w: 0,
            screen_h: 0,
            events: EventBus::new(),
        }
    }

    // ── Monitor helpers ─────────────────────────────────────────────

    /// Get the currently selected monitor.
    pub fn selected_monitor(&self) -> &Monitor {
        &self.monitors[self.sel_mon]
    }

    /// Get the currently selected monitor mutably.
    pub fn selected_monitor_mut(&mut self) -> &mut Monitor {
        &mut self.monitors[self.sel_mon]
    }

    /// Find the monitor with maximum overlap for a rectangle.
    pub fn rect_to_monitor(&self, rect: &Rect) -> MonitorId {
        self.monitors
            .iter()
            .enumerate()
            .max_by_key(|(_, m)| m.geom.intersect_area(rect))
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    // ── Client helpers ──────────────────────────────────────────────

    /// Insert a new client, add to the specified monitor.
    pub fn add_client(&mut self, mut client: Client, mon_id: MonitorId) -> ClientId {
        client.mon_id = mon_id;
        let cid = self.clients.insert(client);
        if let Some(mon) = self.monitors.get_mut(mon_id) {
            mon.clients.push(cid);
            mon.stack.insert(0, cid);
        }
        cid
    }

    /// Remove a client from the state entirely.
    pub fn remove_client(&mut self, cid: ClientId) {
        if let Some(client) = self.clients.remove(cid) {
            let mon_id = client.mon_id;
            if let Some(mon) = self.monitors.get_mut(mon_id) {
                mon.clients.retain(|&id| id != cid);
                mon.stack.retain(|&id| id != cid);
                if mon.focused == Some(cid) {
                    // Focus the next in stack
                    mon.focused = mon.stack.first().copied();
                }
            }
        }
    }

    /// Get visible tiled clients on a monitor (for layout).
    pub fn visible_tiled(&self, mon_id: MonitorId) -> Vec<(ClientId, &Client)> {
        let mon = &self.monitors[mon_id];
        let tags = mon.current_tags();
        mon.clients
            .iter()
            .filter_map(|&cid| {
                let c = self.clients.get(cid)?;
                if is_visible(c.tags, tags) && !c.is_floating && !c.is_fullscreen {
                    Some((cid, c))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Find a client by X11 window id.
    pub fn client_by_window(&self, win: u32) -> Option<ClientId> {
        self.clients
            .iter()
            .find(|(_, c)| c.win == win)
            .map(|(id, _)| id)
    }

    /// Count clients with a given tag bit set.
    pub fn clients_on_tag(&self, mon_id: MonitorId, tag_bit: u32) -> usize {
        let mon = &self.monitors[mon_id];
        mon.clients
            .iter()
            .filter_map(|&cid| self.clients.get(cid))
            .filter(|c| (c.tags & tag_bit) != 0)
            .count()
    }

    // ── Layout helpers ──────────────────────────────────────────────

    /// Get the current layout for a monitor.
    pub fn current_layout(&self, mon_id: MonitorId) -> Option<&dyn Layout> {
        let layout_id = self.monitors[mon_id].current_layout();
        self.layouts.get(layout_id.0).map(|l| l.as_ref())
    }

    /// Get layout symbol for a monitor.
    pub fn layout_symbol(&self, mon_id: MonitorId) -> &str {
        self.current_layout(mon_id)
            .map(|l| l.symbol())
            .unwrap_or("???")
    }

    // ── Tag operations (ported from dwm) ────────────────────────────

    /// Switch view to specific tags (dwm `view()`).
    pub fn view_tags(&mut self, mon_id: MonitorId, tags: u32) {
        let mon = &mut self.monitors[mon_id];
        let new_tags = tags & TAGMASK;
        if new_tags == mon.current_tags() {
            return;
        }
        // Toggle sel_tags to save previous
        mon.sel_tags ^= 1;
        if new_tags != 0 {
            mon.tagset[mon.sel_tags] = new_tags;
        }
    }

    /// Toggle a tag in the current view (dwm `toggleview()`).
    pub fn toggle_view(&mut self, mon_id: MonitorId, tag_bit: u32) {
        let mon = &mut self.monitors[mon_id];
        let new_tags = mon.current_tags() ^ (tag_bit & TAGMASK);
        if new_tags != 0 {
            mon.tagset[mon.sel_tags] = new_tags;
        }
    }

    /// Move focused client to specific tags (dwm `tag()`).
    pub fn tag_client(&mut self, cid: ClientId, tags: u32) {
        if let Some(client) = self.clients.get_mut(cid) {
            let new_tags = tags & TAGMASK;
            if new_tags != 0 {
                client.tags = new_tags;
            }
        }
    }

    /// Toggle a tag on the focused client (dwm `toggletag()`).
    pub fn toggle_client_tag(&mut self, cid: ClientId, tag_bit: u32) {
        if let Some(client) = self.clients.get_mut(cid) {
            let new_tags = client.tags ^ (tag_bit & TAGMASK);
            if new_tags != 0 {
                client.tags = new_tags;
            }
        }
    }

    /// Scratchpad toggle (dwm scratchpad patch).
    pub fn toggle_scratchpad(&mut self, mon_id: MonitorId) -> Option<ClientId> {
        let mon = &self.monitors[mon_id];
        let scratch_client = mon.clients.iter().find(|&&cid| {
            self.clients
                .get(cid)
                .map(|c| (c.tags & SCRATCHTAG) != 0)
                .unwrap_or(false)
        }).copied();

        if let Some(cid) = scratch_client {
            let mon = &mut self.monitors[mon_id];
            mon.tagset[mon.sel_tags] ^= SCRATCHTAG;
            Some(cid)
        } else {
            None // caller should spawn scratchpad command
        }
    }
}

impl Default for WmState {
    fn default() -> Self {
        Self::new()
    }
}
