//! TLB and CR3 management. All paging-related assembly is confined here.

use core::arch::asm;

/// Virtual address (canonical form). Used for TLB flushes.
#[derive(Clone, Copy, Debug)]
pub struct VirtAddr(pub u64);

impl VirtAddr {
    pub const fn new(va: u64) -> Self {
        Self(va)
    }
}

/// Read current CR3 (physical address of PML4).
pub fn read_cr3() -> u64 {
    let cr3: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nostack, preserves_flags));
    }
    cr3
}

/// Load CR3. Caller must ensure pa is a valid PML4 physical address.
pub unsafe fn write_cr3(pa: u64) {
    asm!("mov cr3, {}", in(reg) pa, options(nostack, preserves_flags));
}

/// Invalidate a single TLB entry for the given virtual address.
pub fn flush_address(addr: VirtAddr) {
    unsafe {
        asm!("invlpg [{}]", in(reg) addr.0, options(nostack, preserves_flags));
    }
}

/// Invalidate entire TLB by reloading CR3. Use after bulk map/unmap in current address space.
pub fn flush_all() {
    let cr3 = read_cr3();
    unsafe {
        write_cr3(cr3);
    }
}
