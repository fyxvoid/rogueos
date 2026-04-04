//! Page table structure and entry encoding.

use crate::memory::paging::flags::EntryFlags;
use crate::memory::paging::levels::ENTRY_COUNT;

const FRAME_MASK: u64 = 0x000F_FFFF_FFFF_F000;

/// One page table (512 x 8-byte entries). 4 KiB aligned.
#[repr(C, align(4096))]
pub struct Table {
    entries: [u64; ENTRY_COUNT],
}

impl Table {
    pub const fn new() -> Self {
        Self {
            entries: [0; ENTRY_COUNT],
        }
    }

    /// Read raw entry at index.
    pub fn get(&self, idx: usize) -> u64 {
        self.entries[idx]
    }

    /// Write raw entry at index.
    pub fn set(&mut self, idx: usize, value: u64) {
        self.entries[idx] = value;
    }

    /// Encode a leaf entry: physical address (must be page-aligned) and flags.
    pub fn encode_entry(pa: u64, flags: EntryFlags) -> u64 {
        (pa & FRAME_MASK) | flags.as_u64()
    }

    /// Decode entry to (physical_address, flags_bits). Returns None if not present.
    pub fn decode_entry(entry: u64) -> Option<(u64, u64)> {
        if (entry & 1) == 0 {
            return None;
        }
        Some((entry & FRAME_MASK, entry & 0xFFF))
    }

    /// Physical address from entry (frame number). Assumes entry is present.
    pub fn entry_to_pa(entry: u64) -> u64 {
        entry & FRAME_MASK
    }
}

/// Convert physical address of a table to a mutable pointer. Tables must be identity-mapped.
pub fn phys_to_virt_table(pa: u64) -> *mut Table {
    pa as *mut Table
}
