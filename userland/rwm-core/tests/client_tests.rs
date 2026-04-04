//! Client struct tests

use rwm_core::{Client, Rect};

#[test]
fn client_new_defaults() {
    let r = Rect::new(10, 20, 100, 50);
    let c = Client::new(42, r, 0);
    assert_eq!(c.win, 42);
    assert_eq!(c.geom, r);
    assert_eq!(c.old_geom, r);
    assert_eq!(c.tags, 0);
    assert_eq!(c.mon_id, 0);
    assert_eq!(c.border_width, 0);
    assert_eq!(c.cfact, 1.0);
    assert!(!c.is_floating);
    assert!(!c.is_fullscreen);
}

#[test]
fn client_full_width_height() {
    let c = Client::new(0, Rect::new(0, 0, 100, 50), 0);
    assert_eq!(c.full_width(), 100);
    assert_eq!(c.full_height(), 50);
}

#[test]
fn client_full_width_with_border() {
    let mut c = Client::new(0, Rect::new(0, 0, 100, 50), 0);
    c.border_width = 2;
    assert_eq!(c.full_width(), 104);
    assert_eq!(c.full_height(), 54);
}

#[test]
fn client_size_hints_default() {
    let c = Client::new(0, Rect::new(0, 0, 1, 1), 0);
    assert_eq!(c.base_w, 0);
    assert!(!c.hints_valid);
}

#[test]
fn client_mon_id_stored() {
    let c = Client::new(0, Rect::new(0, 0, 1, 1), 3);
    assert_eq!(c.mon_id, 3);
}
