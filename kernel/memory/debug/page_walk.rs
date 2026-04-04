//! Page table walk: VA to PA and flags. Debug tool only.

use crate::memory::paging;

/// Result of walking one VA: physical address and raw flags, or not present.
pub fn walk(cr3: u64, va: u64) -> Option<(u64, u64)> {
    let va_page = paging::page_align_down(va);
    let pte = paging::walk_pte(cr3, va_page)?;
    let pa = pte & 0x000F_FFFF_FFFF_F000;
    let flags = pte & 0xFFF;
    Some((pa, flags))
}
