//! Virtual memory region (VmArea). Ordered, non-overlapping ranges with flags.

use crate::memory::paging::PAGE_SIZE;

/// A contiguous virtual memory region: [start, end) with flags. Optional backing for future use.
#[derive(Clone, Copy, Debug)]
pub struct VmArea {
    pub start: u64,
    pub end: u64,
    pub flags: u64,
}

impl VmArea {
    pub fn new(start: u64, end: u64, flags: u64) -> Self {
        Self { start, end, flags }
    }

    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_page_aligned(&self) -> bool {
        (self.start & (PAGE_SIZE as u64 - 1)) == 0
            && (self.end & (PAGE_SIZE as u64 - 1)) == 0
    }

    pub fn overlaps(&self, other: &VmArea) -> bool {
        self.start < other.end && other.start < self.end
    }
}
