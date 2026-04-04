//! UEFI memory map parsing for physical allocator init.
//!
//! Descriptor layout must match what the bootloader (gatehouse) writes:
//! uefi::table::boot::MemoryDescriptor (Type, phys_start, virt_start, page_count, att).

use crate::arch::x86_64::serial;

/// UEFI memory type: EfiConventionalMemory = 7 (usable RAM).
pub const EFI_CONVENTIONAL_MEMORY: u32 = 7;

/// Kernel copy of UEFI EFI_MEMORY_DESCRIPTOR for parsing the map at BootInfo::mem_map_paddr.
/// Layout must match uefi::table::boot::MemoryDescriptor (gatehouse copies this).
#[repr(C)]
pub struct MemoryDescriptor {
    pub ty: u32,
    pub _pad: u32,
    pub phys_start: u64,
    pub virt_start: u64,
    pub page_count: u64,
    pub att: u64,
}

/// Reserved physical regions we must not use for general allocation.
#[derive(Clone, Copy, Debug)]
pub struct ReservedRegion {
    pub start: u64,
    pub len: u64,
}

/// Returns true if [start, start+len) overlaps [r_start, r_start+r_len).
#[inline]
fn overlaps(start: u64, len: u64, r_start: u64, r_len: u64) -> bool {
    let end = start.saturating_add(len);
    let r_end = r_start.saturating_add(r_len);
    start < r_end && r_start < end
}

/// Page size for UEFI (4 KiB).
const PAGE_SIZE: u64 = 4096;

/// Build reserved regions from BootInfo (kernel, framebuffer, BootInfo page, memory map, NVMe BAR, ACPI RSDP).
pub fn reserved_from_bootinfo(
    fb_base: u64,
    fb_size: u64,
    mem_map_paddr: u64,
    mem_map_size: u64,
    nvme_bar: u64,
    rsdp_addr: u64,
) -> [ReservedRegion; 8] {
    let page = PAGE_SIZE;
    [
        ReservedRegion { start: 0, len: 16 * 1024 * 1024 }, // low 16 MB: kernel image, etc.
        ReservedRegion { start: 0x8000, len: page },         // BootInfo
        ReservedRegion { start: fb_base, len: (fb_size + page - 1) & !(page - 1) },
        ReservedRegion { start: mem_map_paddr, len: (mem_map_size + page - 1) & !(page - 1) },
        ReservedRegion { start: nvme_bar, len: if nvme_bar != 0 { page } else { 0 } },
        ReservedRegion { start: rsdp_addr, len: if rsdp_addr != 0 { page } else { 0 } },
        ReservedRegion { start: 0, len: 0 },
        ReservedRegion { start: 0, len: 0 },
    ]
}

/// Choose one contiguous EfiConventionalMemory range for the frame allocator.
/// Prefer a range >= 2 MiB that does not overlap any reserved region; pick largest such.
/// Returns (phys_start, page_count) or None if no suitable range.
pub fn choose_conventional_region(
    mem_map_paddr: u64,
    mem_map_size: u64,
    mem_desc_size: u32,
    reserved: &[ReservedRegion],
) -> Option<(u64, usize)> {
    let desc_size = mem_desc_size as usize;
    if desc_size < core::mem::size_of::<MemoryDescriptor>() {
        return None;
    }
    let num_entries = mem_map_size as usize / desc_size;
    let mut best_start = 0u64;
    let mut best_pages = 0usize;
    const MIN_BASE: u64 = 2 * 1024 * 1024; // 2 MiB

    for i in 0..num_entries {
        let desc_ptr = (mem_map_paddr as usize)
            .checked_add(i.checked_mul(desc_size)?)?
            as *const u8;
        let ty = unsafe { (desc_ptr as *const u32).read() };
        if ty != EFI_CONVENTIONAL_MEMORY {
            continue;
        }
        let phys_start = unsafe { (desc_ptr.add(8) as *const u64).read() };
        let page_count = unsafe { (desc_ptr.add(24) as *const u64).read() };
        let len = page_count.saturating_mul(PAGE_SIZE);
        if phys_start < MIN_BASE || page_count == 0 {
            continue;
        }
        let mut overlap = false;
        for r in reserved.iter() {
            if r.len == 0 {
                continue;
            }
            if overlaps(phys_start, len, r.start, r.len) {
                overlap = true;
                break;
            }
        }
        if overlap {
            continue;
        }
        let pages = page_count as usize;
        if pages > best_pages {
            best_start = phys_start;
            best_pages = pages;
        }
    }
    if best_pages == 0 {
        None
    } else {
        Some((best_start, best_pages))
    }
}

/// Log chosen region to serial (for diagnostics).
pub fn log_chosen_region(start: u64, pages: usize) {
    serial::write_str("[physical] conventional_region start=");
    serial::write_hex(start);
    serial::write_str(" pages=");
    serial::write_hex(pages as u64);
    serial::write_str("\r\n");
}
