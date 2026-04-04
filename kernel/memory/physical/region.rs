//! Contiguous physical region representation and validation.

use crate::memory::physical::layout::{PAGE_SIZE, FRAME_REGION_BASE, FRAME_REGION_PAGES};

/// A contiguous physical memory region (start address, length in bytes).
#[derive(Clone, Copy, Debug)]
pub struct PhysicalRegion {
    pub start: u64,
    pub length: usize,
}

impl PhysicalRegion {
    /// Default frame region used by the buddy allocator.
    pub fn frame_region() -> Self {
        Self {
            start: FRAME_REGION_BASE,
            length: FRAME_REGION_PAGES * PAGE_SIZE,
        }
    }

    /// End address (exclusive).
    pub fn end(&self) -> u64 {
        self.start + self.length as u64
    }

    /// True if the region is page-aligned and length is a multiple of page size.
    pub fn is_aligned(&self) -> bool {
        (self.start & (PAGE_SIZE as u64 - 1)) == 0
            && (self.length % PAGE_SIZE) == 0
    }

    /// Page index within this region for a given physical address.
    /// Panics if pa is outside the region or not page-aligned.
    pub fn page_index(&self, pa: u64) -> usize {
        assert!(pa >= self.start && pa < self.end());
        assert!((pa - self.start) % PAGE_SIZE as u64 == 0);
        ((pa - self.start) / PAGE_SIZE as u64) as usize
    }

    /// Physical address of the page at the given index.
    pub fn index_to_address(&self, idx: usize) -> u64 {
        self.start + (idx * PAGE_SIZE) as u64
    }
}
