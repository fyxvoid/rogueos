//! Tag constants and is_visible() tests (~25)

use rwm_core::{is_visible, TAG_COUNT, TAGMASK, SCRATCHTAG};

#[test]
fn tag_count_is_nine() {
    assert_eq!(TAG_COUNT, 9);
}

#[test]
fn tagmask_is_0x1ff() {
    assert_eq!(TAGMASK, 0x1FF);
}

#[test]
fn tagmask_bits() {
    assert_eq!(TAGMASK, (1 << 9) - 1);
}

#[test]
fn scratchtag_is_bit_9() {
    assert_eq!(SCRATCHTAG, 1 << 9);
}

#[test]
fn scratchtag_is_512() {
    assert_eq!(SCRATCHTAG, 512);
}

#[test]
fn scratchtag_and_tagmask_disjoint() {
    assert_eq!(TAGMASK & SCRATCHTAG, 0);
}

#[test]
fn is_visible_exact_match() {
    assert!(is_visible(1, 1));
    assert!(is_visible(0b100, 0b100));
}

#[test]
fn is_visible_subset() {
    assert!(is_visible(0b011, 0b111));
}

#[test]
fn is_visible_superset() {
    assert!(is_visible(0b111, 0b011));
}

#[test]
fn is_visible_no_match() {
    assert!(!is_visible(0b001, 0b110));
}

#[test]
fn is_visible_zero_client() {
    assert!(!is_visible(0, 0b111));
}

#[test]
fn is_visible_zero_view() {
    assert!(!is_visible(0b111, 0));
}

#[test]
fn is_visible_both_zero() {
    assert!(!is_visible(0, 0));
}

#[test]
fn is_visible_scratchtag_with_scratch_in_view() {
    assert!(is_visible(SCRATCHTAG, SCRATCHTAG));
}

#[test]
fn is_visible_scratchtag_with_normal_view() {
    assert!(!is_visible(SCRATCHTAG, TAGMASK));
}

#[test]
fn is_visible_tag_1_through_9_bits() {
    for i in 0..9 {
        let bit = 1u32 << i;
        assert!(is_visible(bit, bit));
        assert!(is_visible(bit, TAGMASK));
    }
}

#[test]
fn is_visible_multi_tag_client() {
    assert!(is_visible(0b101, 0b001));
    assert!(is_visible(0b101, 0b100));
    assert!(is_visible(0b101, 0b101));
    assert!(!is_visible(0b101, 0b010));
}

#[test]
fn tagmask_covers_all_nine() {
    let mut sum = 0u32;
    for i in 0..TAG_COUNT {
        sum |= 1 << i;
    }
    assert_eq!(sum, TAGMASK);
}

#[test]
fn scratchtag_above_tagmask() {
    assert!(SCRATCHTAG > TAGMASK);
}
