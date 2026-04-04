//! WmState operations tests

use rwm_core::{Client, Rect, WmState, TAGMASK};
use slotmap::Key;

fn rect(w: u32, h: u32) -> Rect {
    Rect::new(0, 0, w, h)
}

#[test]
fn state_new_defaults() {
    let s = WmState::new();
    assert!(s.clients.is_empty());
    assert!(s.monitors.is_empty());
    assert_eq!(s.sel_mon, 0);
    assert!(s.running);
    assert_eq!(s.screen_w, 0);
    assert_eq!(s.screen_h, 0);
}

#[test]
fn state_add_client_returns_id() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    let c = Client::new(1, rect(50, 50), 0);
    let id = s.add_client(c, 0);
    assert!(!id.is_null());
}

#[test]
fn state_add_client_adds_to_monitor() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    let c = Client::new(1, rect(50, 50), 0);
    let id = s.add_client(c, 0);
    assert_eq!(s.monitors[0].clients.len(), 1);
    assert_eq!(s.monitors[0].clients[0], id);
    assert_eq!(s.monitors[0].stack[0], id);
}

#[test]
fn state_remove_client_removes_from_map() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    let id = s.add_client(Client::new(1, rect(50, 50), 0), 0);
    s.remove_client(id);
    assert!(s.clients.get(id).is_none());
}

#[test]
fn state_remove_client_removes_from_monitor() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    let id = s.add_client(Client::new(1, rect(50, 50), 0), 0);
    s.remove_client(id);
    assert!(s.monitors[0].clients.is_empty());
}

#[test]
fn state_client_by_window_found() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    let id = s.add_client(Client::new(42, rect(50, 50), 0), 0);
    assert_eq!(s.client_by_window(42), Some(id));
}

#[test]
fn state_client_by_window_not_found() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    s.add_client(Client::new(1, rect(50, 50), 0), 0);
    assert!(s.client_by_window(999).is_none());
}

#[test]
fn state_visible_tiled_empty() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    let v = s.visible_tiled(0);
    assert!(v.is_empty());
}

#[test]
fn state_visible_tiled_excludes_floating() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    let id = s.add_client(Client::new(1, rect(50, 50), 0), 0);
    s.clients.get_mut(id).unwrap().is_floating = true;
    let v = s.visible_tiled(0);
    assert!(v.is_empty());
}

#[test]
fn state_view_tags_changes_current() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    s.view_tags(0, 0b100);
    assert_eq!(s.monitors[0].current_tags(), 0b100);
}

#[test]
fn state_view_tags_applies_tagmask() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    s.view_tags(0, !0);
    assert_eq!(s.monitors[0].current_tags(), TAGMASK);
}

#[test]
fn state_tag_client() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    let id = s.add_client(Client::new(1, rect(50, 50), 0), 0);
    s.tag_client(id, 0b010);
    assert_eq!(s.clients.get(id).unwrap().tags, 0b010);
}

#[test]
fn state_toggle_view() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    s.monitors[0].tagset[0] = 0b001;
    s.toggle_view(0, 0b010);
    assert_eq!(s.monitors[0].current_tags(), 0b011);
}

#[test]
fn state_toggle_client_tag() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    let id = s.add_client(Client::new(1, rect(50, 50), 0), 0);
    s.clients.get_mut(id).unwrap().tags = 0b001;
    s.toggle_client_tag(id, 0b010);
    assert_eq!(s.clients.get(id).unwrap().tags, 0b011);
}

#[test]
fn state_clients_on_tag() {
    let mut s = WmState::new();
    s.monitors.push(rwm_core::Monitor::new(0, rect(100, 100)));
    let mut c1 = Client::new(1, rect(50, 50), 0);
    c1.tags = 0b001;
    let mut c2 = Client::new(2, rect(50, 50), 0);
    c2.tags = 0b001;
    s.add_client(c1, 0);
    s.add_client(c2, 0);
    assert_eq!(s.clients_on_tag(0, 0b001), 2);
}
