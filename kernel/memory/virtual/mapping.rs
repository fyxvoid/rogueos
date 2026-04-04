//! Mapping operations: all map/unmap go through here. Uses paging and frame allocator.

use crate::memory::paging::{self, PAGE_SIZE};
use crate::memory::physical::frame_allocator;

/// Create a new empty address space. Returns CR3 or 0.
pub fn alloc_address_space() -> u64 {
    paging::new_address_space()
}

/// Map one page in the given address space.
pub fn map_page_in_space(cr3: u64, va: u64, pa: u64, flags: u64) -> bool {
    paging::map_page_into_space(cr3, va, pa, flags)
}

/// Allocate one page (for tables or user mappings). Returns None when pool exhausted.
pub fn alloc_table_page() -> Option<u64> {
    paging::alloc_table_page()
}

/// Map a page range in the given address space, allocating frames. Returns true if all mapped.
pub fn map_range(cr3: u64, va_start: u64, num_pages: usize, flags: u64) -> bool {
    let mut va = va_start;
    for _ in 0..num_pages {
        let Some(pa) = frame_allocator::alloc_frame() else {
            return false;
        };
        if !paging::map_page_into_space(cr3, va, pa, flags) {
            frame_allocator::free_frame(pa);
            return false;
        }
        va += PAGE_SIZE as u64;
    }
    true
}
