//! Rect geometry tests (~40)

use rwm_core::Rect;

#[test]
fn rect_new_valid() {
    let r = Rect::new(10, 20, 100, 50);
    assert_eq!(r.x, 10);
    assert_eq!(r.y, 20);
    assert_eq!(r.w, 100);
    assert_eq!(r.h, 50);
}

#[test]
fn rect_new_zero_origin() {
    let r = Rect::new(0, 0, 1, 1);
    assert_eq!(r.x, 0);
    assert_eq!(r.y, 0);
    assert_eq!(r.w, 1);
    assert_eq!(r.h, 1);
}

#[test]
fn rect_new_zero_size() {
    let r = Rect::new(5, 5, 0, 0);
    assert_eq!(r.w, 0);
    assert_eq!(r.h, 0);
}

#[test]
fn rect_new_large_values() {
    let r = Rect::new(1000, 2000, 3840, 2160);
    assert_eq!(r.x, 1000);
    assert_eq!(r.y, 2000);
    assert_eq!(r.w, 3840);
    assert_eq!(r.h, 2160);
}

#[test]
fn rect_default() {
    let r = Rect::default();
    assert_eq!(r.x, 0);
    assert_eq!(r.y, 0);
    assert_eq!(r.w, 0);
    assert_eq!(r.h, 0);
}

#[test]
fn rect_clone_copy() {
    let a = Rect::new(1, 2, 3, 4);
    let b = a;
    assert_eq!(a, b);
    let c = a.clone();
    assert_eq!(a, c);
}

#[test]
fn rect_partial_eq() {
    let a = Rect::new(0, 0, 10, 10);
    let b = Rect::new(0, 0, 10, 10);
    let c = Rect::new(1, 0, 10, 10);
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn intersect_area_no_overlap() {
    let a = Rect::new(0, 0, 10, 10);
    let b = Rect::new(20, 20, 10, 10);
    assert_eq!(a.intersect_area(&b), 0);
}

#[test]
fn intersect_area_partial() {
    let a = Rect::new(0, 0, 10, 10);
    let b = Rect::new(5, 5, 10, 10);
    assert_eq!(a.intersect_area(&b), 5 * 5);
}

#[test]
fn intersect_area_contained() {
    let a = Rect::new(0, 0, 100, 100);
    let b = Rect::new(10, 10, 20, 20);
    assert_eq!(a.intersect_area(&b), 20 * 20);
}

#[test]
fn intersect_area_identical() {
    let a = Rect::new(5, 5, 50, 50);
    let b = Rect::new(5, 5, 50, 50);
    assert_eq!(a.intersect_area(&b), 50 * 50);
}

#[test]
fn intersect_area_adjacent_horizontal() {
    let a = Rect::new(0, 0, 10, 10);
    let b = Rect::new(10, 0, 10, 10);
    assert_eq!(a.intersect_area(&b), 0);
}

#[test]
fn intersect_area_adjacent_vertical() {
    let a = Rect::new(0, 0, 10, 10);
    let b = Rect::new(0, 10, 10, 10);
    assert_eq!(a.intersect_area(&b), 0);
}

#[test]
fn intersect_area_zero_width() {
    let a = Rect::new(0, 0, 0, 10);
    let b = Rect::new(0, 0, 10, 10);
    assert_eq!(a.intersect_area(&b), 0);
}

#[test]
fn intersect_area_negative_coords() {
    let a = Rect::new(-10, -10, 20, 20);
    let b = Rect::new(0, 0, 10, 10);
    assert_eq!(a.intersect_area(&b), 10 * 10);
}

#[test]
fn intersect_area_commutative() {
    let a = Rect::new(0, 0, 10, 10);
    let b = Rect::new(5, 5, 15, 15);
    assert_eq!(a.intersect_area(&b), b.intersect_area(&a));
}

#[test]
fn intersect_area_self() {
    let a = Rect::new(10, 20, 30, 40);
    assert_eq!(a.intersect_area(&a), 30 * 40);
}

#[test]
fn inset_normal() {
    let r = Rect::new(0, 0, 100, 100);
    let s = r.inset(5, 5, 5, 5);
    assert_eq!(s.x, 5);
    assert_eq!(s.y, 5);
    assert_eq!(s.w, 90);
    assert_eq!(s.h, 90);
}

#[test]
fn inset_zero() {
    let r = Rect::new(10, 10, 50, 50);
    let s = r.inset(0, 0, 0, 0);
    assert_eq!(s.x, 10);
    assert_eq!(s.y, 10);
    assert_eq!(s.w, 50);
    assert_eq!(s.h, 50);
}

#[test]
fn inset_single_side_top() {
    let r = Rect::new(0, 0, 100, 100);
    let s = r.inset(10, 0, 0, 0);
    assert_eq!(s.y, 10);
    assert_eq!(s.h, 90);
}

#[test]
fn inset_single_side_left() {
    let r = Rect::new(0, 0, 100, 100);
    let s = r.inset(0, 0, 0, 15);
    assert_eq!(s.x, 15);
    assert_eq!(s.w, 85);
}

#[test]
fn inset_min_width_one() {
    let r = Rect::new(0, 0, 10, 10);
    let s = r.inset(0, 100, 0, 0);
    assert_eq!(s.w, 1);
}

#[test]
fn inset_min_height_one() {
    let r = Rect::new(0, 0, 10, 10);
    let s = r.inset(100, 0, 0, 0);
    assert_eq!(s.h, 1);
}

#[test]
fn inset_negative_expands() {
    let r = Rect::new(50, 50, 100, 100);
    let s = r.inset(-5, -5, -5, -5);
    assert_eq!(s.x, 45);
    assert_eq!(s.y, 45);
    assert_eq!(s.w, 110);
    assert_eq!(s.h, 110);
}

#[test]
fn inset_all_sides_different() {
    let r = Rect::new(0, 0, 100, 80);
    let s = r.inset(2, 4, 6, 8);
    assert_eq!(s.x, 8);
    assert_eq!(s.y, 2);
    assert_eq!(s.w, 88);
    assert_eq!(s.h, 72);
}

#[test]
fn rect_trait_debug() {
    let r = Rect::new(1, 2, 3, 4);
    let s = format!("{:?}", r);
    assert!(s.contains("1"));
    assert!(s.contains("2"));
}

#[test]
fn rect_intersect_corner_touch() {
    let a = Rect::new(0, 0, 10, 10);
    let b = Rect::new(10, 10, 10, 10);
    assert_eq!(a.intersect_area(&b), 0);
}

#[test]
fn rect_intersect_one_pixel() {
    let a = Rect::new(0, 0, 10, 10);
    let b = Rect::new(9, 9, 10, 10);
    assert_eq!(a.intersect_area(&b), 1);
}

#[test]
fn rect_inset_exact_half() {
    let r = Rect::new(0, 0, 20, 20);
    let s = r.inset(5, 5, 5, 5);
    assert_eq!(s.w, 10);
    assert_eq!(s.h, 10);
}

#[test]
fn rect_inset_large_gaps() {
    let r = Rect::new(100, 100, 50, 50);
    let s = r.inset(20, 20, 20, 20);
    assert_eq!(s.x, 120);
    assert_eq!(s.y, 120);
    assert_eq!(s.w, 10);
    assert_eq!(s.h, 10);
}

#[test]
fn rect_negative_origin_inset() {
    let r = Rect::new(-20, -20, 50, 50);
    let s = r.inset(5, 5, 5, 5);
    assert_eq!(s.x, -15);
    assert_eq!(s.y, -15);
    assert_eq!(s.w, 40);
    assert_eq!(s.h, 40);
}

#[test]
fn rect_intersect_b_contains_a() {
    let a = Rect::new(50, 50, 10, 10);
    let b = Rect::new(0, 0, 100, 100);
    assert_eq!(a.intersect_area(&b), 10 * 10);
}

#[test]
fn rect_inset_zero_size_becomes_one() {
    let r = Rect::new(0, 0, 5, 5);
    let s = r.inset(2, 2, 2, 2);
    assert_eq!(s.w, 1);
    assert_eq!(s.h, 1);
}

#[test]
fn rect_equality_reflexive() {
    let r = Rect::new(1, 2, 3, 4);
    assert_eq!(r, r);
}

#[test]
fn rect_inset_symmetric() {
    let r = Rect::new(10, 10, 100, 100);
    let s = r.inset(10, 10, 10, 10);
    assert_eq!(s.x, 20);
    assert_eq!(s.y, 20);
    assert_eq!(s.w, 80);
    assert_eq!(s.h, 80);
}

#[test]
fn rect_intersect_staggered() {
    let a = Rect::new(0, 0, 30, 30);
    let b = Rect::new(20, 10, 30, 30);
    assert_eq!(a.intersect_area(&b), 10 * 20);
}

#[test]
fn rect_default_eq_new_zeros() {
    assert_eq!(Rect::default(), Rect::new(0, 0, 0, 0));
}

#[test]
fn rect_inset_negative_all() {
    let r = Rect::new(50, 50, 10, 10);
    let s = r.inset(-1, -1, -1, -1);
    assert_eq!(s.x, 49);
    assert_eq!(s.y, 49);
    assert_eq!(s.w, 12);
    assert_eq!(s.h, 12);
}
