//! Monitor struct tests (~40)

use rwm_core::{Monitor, Rect};
use slotmap::Key;

fn mon(id: usize, w: u32, h: u32) -> Monitor {
    Monitor::new(id, Rect::new(0, 0, w, h))
}

#[test]
fn monitor_new_defaults() {
    let m = mon(0, 1920, 1080);
    assert_eq!(m.id, 0);
    assert_eq!(m.geom.w, 1920);
    assert_eq!(m.geom.h, 1080);
    assert_eq!(m.window_area, m.geom);
    assert_eq!(m.tagset[0], 1);
    assert_eq!(m.tagset[1], 1);
    assert_eq!(m.sel_tags, 0);
    assert_eq!(m.mfact, 0.55);
    assert_eq!(m.nmaster, 1);
    assert_eq!(m.gap_inner_h, 4);
    assert!(m.show_bar);
    assert!(m.top_bar);
    assert!(m.clients.is_empty());
    assert!(m.stack.is_empty());
    assert!(m.focused.is_none());
}

#[test]
fn monitor_current_tags_default() {
    let m = mon(0, 100, 100);
    assert_eq!(m.current_tags(), 1);
}

#[test]
fn monitor_current_layout_default() {
    let m = mon(0, 100, 100);
    assert_eq!(m.current_layout().0, 0);
}

#[test]
fn monitor_update_bar_pos_top_bar_visible() {
    let mut m = mon(0, 1000, 800);
    m.top_bar = true;
    m.update_bar_pos(24);
    assert_eq!(m.window_area.y, 24);
    assert_eq!(m.window_area.h, 776);
    assert_eq!(m.bar_y, 0);
}

#[test]
fn monitor_update_bar_pos_bottom_bar() {
    let mut m = mon(0, 1000, 800);
    m.top_bar = false;
    m.update_bar_pos(24);
    assert_eq!(m.window_area.y, 0);
    assert_eq!(m.window_area.h, 776);
    assert_eq!(m.bar_y, 776);
}

#[test]
fn monitor_update_bar_pos_hidden() {
    let mut m = mon(0, 1000, 800);
    m.show_bar = false;
    m.update_bar_pos(24);
    assert_eq!(m.window_area, m.geom);
    assert_eq!(m.bar_y, -24);
}

#[test]
fn monitor_raise_in_stack_single() {
    let mut m = mon(0, 100, 100);
    use rwm_core::ClientId;
    let id = ClientId::null();
    m.stack.push(id);
    m.raise_in_stack(id);
    assert_eq!(m.stack.len(), 1);
}

#[test]
fn monitor_raise_in_stack_already_front_no_change() {
    let mut m = mon(0, 100, 100);
    use rwm_core::ClientId;
    let id = ClientId::null();
    m.stack.push(id);
    m.raise_in_stack(id);
    assert_eq!(m.stack[0], id);
}

#[test]
fn monitor_layout_symbol_default() {
    let m = mon(0, 100, 100);
    assert_eq!(m.layout_symbol, "[]=");
}

#[test]
fn monitor_current_tags_after_modify() {
    let mut m = mon(0, 100, 100);
    m.tagset[0] = 0b101;
    assert_eq!(m.current_tags(), 0b101);
}

#[test]
fn monitor_current_tags_sel_tags_toggle() {
    let mut m = mon(0, 100, 100);
    m.tagset[0] = 1;
    m.tagset[1] = 2;
    assert_eq!(m.current_tags(), 1);
    m.sel_tags = 1;
    assert_eq!(m.current_tags(), 2);
}

#[test]
fn monitor_current_layout_sel_layout_toggle() {
    let mut m = mon(0, 100, 100);
    use rwm_core::LayoutId;
    m.layout[0] = LayoutId(0);
    m.layout[1] = LayoutId(1);
    m.sel_layout = 1;
    assert_eq!(m.current_layout().0, 1);
}

#[test]
fn monitor_update_bar_pos_height_32() {
    let mut m = mon(0, 800, 600);
    m.update_bar_pos(32);
    assert_eq!(m.window_area.h, 568);
}

#[test]
fn monitor_gap_defaults() {
    let m = mon(0, 100, 100);
    assert_eq!(m.gap_inner_h, 4);
    assert_eq!(m.gap_inner_v, 4);
    assert_eq!(m.gap_outer_h, 4);
    assert_eq!(m.gap_outer_v, 4);
}

#[test]
fn monitor_bar_win_default_zero() {
    let m = mon(0, 100, 100);
    assert_eq!(m.bar_win, 0);
}
