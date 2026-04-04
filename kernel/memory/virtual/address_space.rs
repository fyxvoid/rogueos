//! Address space: CR3 plus optional region list. Validation on insert.

use crate::memory::r#virtual::region::VmArea;

const MAX_REGIONS: usize = 32;

/// An address space (one PML4). Holds CR3 and an ordered list of mapped regions.
pub struct AddressSpace {
    pub cr3: u64,
    regions: [Option<VmArea>; MAX_REGIONS],
    len: usize,
}

impl AddressSpace {
    pub const fn new(cr3: u64) -> Self {
        Self {
            cr3,
            regions: [None; MAX_REGIONS],
            len: 0,
        }
    }

    /// Add a region. Returns false if overlap or full.
    pub fn add_region(&mut self, area: VmArea) -> bool {
        if !area.is_page_aligned() {
            return false;
        }
        for i in 0..self.len {
            if let Some(ref r) = self.regions[i] {
                if area.overlaps(r) {
                    return false;
                }
            }
        }
        if self.len >= MAX_REGIONS {
            return false;
        }
        self.regions[self.len] = Some(area);
        self.len += 1;
        true
    }

    pub fn region_count(&self) -> usize {
        self.len
    }
}
