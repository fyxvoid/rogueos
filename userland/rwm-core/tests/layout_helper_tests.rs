//! effective_gaps and cfact_sizes tests

use rwm_core::layout::cfact_sizes;
use rwm_core::layout::effective_gaps;
use rwm_core::Client;
use rwm_core::Monitor;
use rwm_core::Rect;

fn mon(gap: i32) -> Monitor {
    let mut m = Monitor::new(0, Rect::new(0, 0, 100, 100));
    m.gap_outer_h = gap;
    m.gap_outer_v = gap;
    m.gap_inner_h = gap;
    m.gap_inner_v = gap;
    m
}

#[test]
fn effective_gaps_zero_clients() {
    let m = mon(4);
    assert_eq!(effective_gaps(&m, 0, true), (0, 0, 0, 0));
}

#[test]
fn effective_gaps_one_client_smart() {
    let m = mon(4);
    assert_eq!(effective_gaps(&m, 1, true), (0, 0, 0, 0));
}

#[test]
fn effective_gaps_one_client_no_smart() {
    let m = mon(4);
    assert_eq!(effective_gaps(&m, 1, false), (4, 4, 4, 4));
}

#[test]
fn cfact_sizes_empty() {
    let v: Vec<&Client> = vec![];
    assert!(cfact_sizes(&v, 100).is_empty());
}

#[test]
fn cfact_sizes_single() {
    let c = Client::new(0, Rect::new(0, 0, 1, 1), 0);
    let v = vec![&c];
    assert_eq!(cfact_sizes(&v, 100), vec![100]);
}

#[test]
fn cfact_sizes_sum_invariant() {
    let c1 = Client::new(0, Rect::new(0, 0, 1, 1), 0);
    let c2 = Client::new(1, Rect::new(0, 0, 1, 1), 0);
    let v = vec![&c1, &c2];
    let s = cfact_sizes(&v, 100);
    assert_eq!(s[0] + s[1], 100);
}
