//! Anonymous memory mapping: SYS_MMAP / SYS_MUNMAP.
//!
//! Each process has its own address space (per-process CR3). SYS_MMAP
//! allocates physical frames from the buddy allocator and maps them at the
//! next available virtual address in the process's anonymous heap region.
//!
//! ## Virtual address layout for anonymous mappings
//!
//! ```text
//! 0x0000_4000_0000  ANON_BASE   (above the 1 GiB mark, clear of ELF load)
//!   … grows upward …
//! 0x0000_7000_0000  ANON_LIMIT
//! ```
//!
//! This range sits in PDPT[0] within PML4[0] but above PD[2] (where ELF
//! loads at 0x400000). Each process's fresh PD gets its own PT entries here.
//!
//! ## Future work
//! - Track mapped regions per process (VMA list) for correct MUNMAP + reuse.
//! - Shared mappings (SHM upgrade path).
//! - Guard pages / PROT_NONE.
//! - Demand paging (map without backing frame; fault-in on access).

use crate::memory::paging::{self, EntryFlags, PageFlag, PAGE_SIZE};
use crate::memory::physical;
use crate::syscall::user_ptr::SysErr;
use libs::prot;

/// Start of the per-process anonymous mapping region.
pub(crate) const ANON_BASE: u64 = 0x0000_4000_0000;
/// Hard limit (exclusive) for anonymous mappings.
pub(crate) const ANON_LIMIT: u64 = 0x0000_7000_0000;

/// Per-process bump pointer for anonymous VA allocation.
/// Stored as a static array indexed by process slot.
static mut ANON_NEXT: [u64; crate::process::MAX_PROCESSES] = [ANON_BASE; crate::process::MAX_PROCESSES];

/// SYS_MMAP — map `pages` anonymous pages with the given protection.
/// Returns the virtual address of the first page, or a negative error.
pub(super) fn sys_mmap(pages_raw: u64, prot_raw: u64) -> Result<u64, SysErr> {
    let pages = pages_raw as usize;
    let prot  = prot_raw as u32;

    if pages == 0 || pages > 1024 {   // 4 MiB cap per single call
        return Err(SysErr::INVAL);
    }

    let idx = crate::process::current_index().ok_or(SysErr::INVAL)?;
    let cr3 = crate::process::current_descriptor()
        .map(|d| d.cr3)
        .ok_or(SysErr::INVAL)?;

    // Bump-allocate virtual addresses.
    let va_start = unsafe { ANON_NEXT[idx] };
    let va_end   = va_start + (pages as u64) * PAGE_SIZE as u64;
    if va_end > ANON_LIMIT {
        return Err(SysErr::NOMEM);
    }

    // Build PTE flags from prot bits.
    let mut flags = EntryFlags::empty().with(PageFlag::Present).with(PageFlag::User);
    if (prot & prot::WRITE) != 0 { flags = flags.with(PageFlag::Writable); }
    if (prot & prot::EXEC)  == 0 { flags = flags.with(PageFlag::NoExec); }
    let flags_u64 = flags.as_u64();

    // Allocate and map each page.
    let mut va = va_start;
    while va < va_end {
        let pa = physical::alloc_frame().ok_or(SysErr::NOMEM)?;
        // Zero the frame so user code sees clean memory.
        unsafe { core::ptr::write_bytes(pa as *mut u8, 0, PAGE_SIZE); }
        if !paging::map_page_into_space(cr3, va, pa, flags_u64) {
            return Err(SysErr::NOMEM);
        }
        va += PAGE_SIZE as u64;
    }

    unsafe { ANON_NEXT[idx] = va_end; }
    paging::flush_all(); // TLB: new mappings in current CR3 (or process's CR3)

    crate::arch::serial::write_str("[MMAP] pid=");
    crate::arch::serial::write_hex(crate::process::current_pid().unwrap_or(0) as u64);
    crate::arch::serial::write_str(" va=");
    crate::arch::serial::write_hex(va_start);
    crate::arch::serial::write_str(" pages=");
    crate::arch::serial::write_hex(pages as u64);
    crate::arch::serial::write_str("\r\n");

    Ok(va_start)
}

/// SYS_MUNMAP — unmap pages at `va` (must be a previous MMAP base).
/// Barebone: unmaps the requested page count but does not reclaim physical
/// frames or update the bump pointer (VMA tracking is future work).
pub(super) fn sys_munmap(va: u64, pages_raw: u64) -> Result<u64, SysErr> {
    let pages = pages_raw as usize;
    if pages == 0 || va < ANON_BASE || va >= ANON_LIMIT {
        return Err(SysErr::INVAL);
    }
    let cr3 = crate::process::current_descriptor()
        .map(|d| d.cr3)
        .ok_or(SysErr::INVAL)?;

    let mut v = va;
    for _ in 0..pages {
        paging::unmap_page_in_space(cr3, v);
        v += PAGE_SIZE as u64;
    }
    paging::flush_all();
    Ok(0)
}

/// Reset the ANON bump pointer for a process slot (called on process exit).
pub fn reset_anon_for_slot(slot: usize) {
    if slot < crate::process::MAX_PROCESSES {
        unsafe { ANON_NEXT[slot] = ANON_BASE; }
    }
}
