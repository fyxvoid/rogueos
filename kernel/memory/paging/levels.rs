//! 4-level paging: level indices and alignment helpers.
//! Page sizes: 4 KB (base), 2 MB (large), 1 GB (huge).

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_SIZE_4K: usize = 4096;
pub const PAGE_SIZE_2MB: usize = 2 * 1024 * 1024;
pub const PAGE_SIZE_1GB: usize = 1024 * 1024 * 1024;
pub const ENTRY_COUNT: usize = 512;

#[inline]
pub fn pml4_index(va: u64) -> usize {
    ((va >> 39) & 0x1FF) as usize
}

#[inline]
pub fn pdpt_index(va: u64) -> usize {
    ((va >> 30) & 0x1FF) as usize
}

#[inline]
pub fn pd_index(va: u64) -> usize {
    ((va >> 21) & 0x1FF) as usize
}

#[inline]
pub fn pt_index(va: u64) -> usize {
    ((va >> 12) & 0x1FF) as usize
}

#[inline]
pub fn page_align_down(va: u64) -> u64 {
    va & !(PAGE_SIZE as u64 - 1)
}

#[inline]
pub fn page_align_up(va: u64) -> u64 {
    (va + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1)
}

#[inline]
pub fn page_align_down_2mb(va: u64) -> u64 {
    va & !(PAGE_SIZE_2MB as u64 - 1)
}

#[inline]
pub fn page_align_up_2mb(va: u64) -> u64 {
    (va + PAGE_SIZE_2MB as u64 - 1) & !(PAGE_SIZE_2MB as u64 - 1)
}

#[inline]
pub fn page_align_down_1gb(va: u64) -> u64 {
    va & !(PAGE_SIZE_1GB as u64 - 1)
}

#[inline]
pub fn page_align_up_1gb(va: u64) -> u64 {
    (va + PAGE_SIZE_1GB as u64 - 1) & !(PAGE_SIZE_1GB as u64 - 1)
}

/// Mask for physical frame number in an entry (bits 12..52).
pub const FRAME_MASK: u64 = 0x000F_FFFF_FFFF_F000;
