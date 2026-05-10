//! Page mapping: map/unmap in current or given address space. Table allocation from static pool.

use crate::memory::paging::flags::{EntryFlags, PageFlag};
use crate::memory::paging::levels::{self, PAGE_SIZE, PAGE_SIZE_1GB, PAGE_SIZE_2MB};
use crate::memory::paging::page_table::{self, Table};
use crate::memory::paging::tlb;

/// PS (Page Size) bit in a PDE: entry maps a 2MB page instead of a P1 table.
const PDE_PS: u64 = 1 << 7;
/// Mask for 2MB page base in a PDE (bits 21-51; address is 2MB-aligned).
const PDE_2MB_FRAME_MASK: u64 = 0x000F_FFFF_FFFF_E0_0000;

/// PS (Page Size) bit in a PDPTE: entry maps a 1GB page instead of a P2 table.
const PDPTE_PS: u64 = 1 << 7;
/// Mask for 1GB page base in a PDPTE (bits 30-51; address is 1GB-aligned).
const PDPTE_1GB_FRAME_MASK: u64 = 0x000F_FFFF_C000_0000;

fn get_or_alloc_child(parent: *mut Table, idx: usize) -> *mut Table {
    unsafe {
        let e = (*parent).get(idx);
        if (e & PageFlag::Present as u64) != 0 {
            // Ensure User bit is set on existing intermediate entry so ring-3 can traverse it.
            if (e & PageFlag::User as u64) == 0 {
                (*parent).set(idx, e | PageFlag::User as u64);
            }
            return page_table::phys_to_virt_table(e & FRAME_MASK);
        }
        let Some(child_pa) = alloc_table_page() else {
            return core::ptr::null_mut();
        };
        let child = page_table::phys_to_virt_table(child_pa);
        core::ptr::write_bytes(child as *mut u8, 0, PAGE_SIZE);
        let flags = EntryFlags::empty().with(PageFlag::Present).with(PageFlag::Writable).with(PageFlag::User);
        (*parent).set(idx, page_table::Table::encode_entry(child_pa, flags));
        child
    }
}

/// Get or allocate PD for this VA. If the PDPT entry is a 1GB page (PS=1), split it into 512×2MB PDEs.
fn get_or_alloc_pd(pdpt: *mut Table, pdpt_idx: usize) -> *mut Table {
    unsafe {
        let e = (*pdpt).get(pdpt_idx);
        if (e & PageFlag::Present as u64) == 0 {
            let Some(child_pa) = alloc_table_page() else {
                return core::ptr::null_mut();
            };
            let child = page_table::phys_to_virt_table(child_pa);
            core::ptr::write_bytes(child as *mut u8, 0, PAGE_SIZE);
            let flags = EntryFlags::empty().with(PageFlag::Present).with(PageFlag::Writable).with(PageFlag::User);
            (*pdpt).set(pdpt_idx, page_table::Table::encode_entry(child_pa, flags));
            return child;
        }
        if (e & PDPTE_PS) == 0 {
            // Existing entry: ensure User bit is set.
            if (e & PageFlag::User as u64) == 0 {
                (*pdpt).set(pdpt_idx, e | PageFlag::User as u64);
            }
            return page_table::phys_to_virt_table(e & FRAME_MASK);
        }
        // 1GB page: split into 512×2MB in a new PD.
        let base_pa = e & PDPTE_1GB_FRAME_MASK;
        let leaf_flags = (e & 0xFFF) | PDE_PS | PageFlag::User as u64;
        let Some(pd_pa) = alloc_table_page() else {
            return core::ptr::null_mut();
        };
        let pd = page_table::phys_to_virt_table(pd_pa);
        core::ptr::write_bytes(pd as *mut u8, 0, PAGE_SIZE);
        for i in 0..levels::ENTRY_COUNT {
            let pa = base_pa + (i as u64) * PAGE_SIZE_2MB as u64;
            (*pd).set(i, (pa & PDE_2MB_FRAME_MASK) | leaf_flags);
        }
        let table_flags = EntryFlags::empty().with(PageFlag::Present).with(PageFlag::Writable).with(PageFlag::User);
        (*pdpt).set(pdpt_idx, (pd_pa & FRAME_MASK) | table_flags.as_u64());
        pd
    }
}

/// Get or allocate P1 for this VA. If the P2 entry is a 2MB page (PS=1), split it into 512×4K PTEs.
fn get_or_alloc_pt(pd: *mut Table, pd_idx: usize) -> *mut Table {
    unsafe {
        let e = (*pd).get(pd_idx);
        if (e & PageFlag::Present as u64) == 0 {
            let Some(child_pa) = alloc_table_page() else {
                return core::ptr::null_mut();
            };
            let child = page_table::phys_to_virt_table(child_pa);
            core::ptr::write_bytes(child as *mut u8, 0, PAGE_SIZE);
            let flags = EntryFlags::empty().with(PageFlag::Present).with(PageFlag::Writable).with(PageFlag::User);
            (*pd).set(pd_idx, page_table::Table::encode_entry(child_pa, flags));
            return child;
        }
        if (e & PDE_PS) == 0 {
            // Existing PT entry: ensure User bit is set.
            if (e & PageFlag::User as u64) == 0 {
                (*pd).set(pd_idx, e | PageFlag::User as u64);
            }
            return page_table::phys_to_virt_table(e & FRAME_MASK);
        }
        // 2MB page: split. Base PA is in bits 21-51.
        let base_pa = e & PDE_2MB_FRAME_MASK;
        // Preserve existing leaf flags; add User so ring-3 can access these pages.
        let leaf_flags = (e & 0xFFF & !PDE_PS) | PageFlag::User as u64;
        let Some(pt_pa) = alloc_table_page() else {
            return core::ptr::null_mut();
        };
        let pt = page_table::phys_to_virt_table(pt_pa);
        core::ptr::write_bytes(pt as *mut u8, 0, PAGE_SIZE);
        for i in 0..levels::ENTRY_COUNT {
            let pa = base_pa + (i as u64) * PAGE_SIZE as u64;
            (*pt).set(i, (pa & FRAME_MASK) | leaf_flags);
        }
        let table_flags = EntryFlags::empty().with(PageFlag::Present).with(PageFlag::Writable).with(PageFlag::User);
        (*pd).set(pd_idx, (pt_pa & FRAME_MASK) | table_flags.as_u64());
        pt
    }
}

use crate::memory::paging::levels::FRAME_MASK;
/// Enough to identity-map 512 MiB + per-process page tables: 512 pages.
const PT_POOL_PAGES: usize = 512;

/// Page-table pool must be 4K-aligned so allocated PAs are 4K-aligned (CR3 / PTE entries mask low 12 bits).
#[repr(C, align(4096))]
struct PagePool([u8; PT_POOL_PAGES * PAGE_SIZE]);
static mut PT_POOL: PagePool = PagePool([0; PT_POOL_PAGES * PAGE_SIZE]);
static mut PT_POOL_NEXT: usize = 0;

/// Allocate one page for use as a page table. From static pool (bootstrap). Returns None when pool exhausted.
pub fn alloc_table_page() -> Option<u64> {
    unsafe {
        if PT_POOL_NEXT >= PT_POOL_PAGES {
            #[cfg(not(test))]
            {
                crate::arch::x86_64::serial::write_str("[paging] PT_POOL exhausted at ");
                crate::arch::x86_64::serial::write_hex(PT_POOL_NEXT as u64);
                crate::arch::x86_64::serial::write_str("\r\n");
            }
            return None;
        }
        let pa = PT_POOL.0.as_ptr() as usize + PT_POOL_NEXT * PAGE_SIZE;
        PT_POOL_NEXT += 1;
        Some(pa as u64)
    }
}

/// Map one page in the given address space (by CR3). Does not switch CR3.
/// If `flags` has the User bit, that bit is propagated to all intermediate
/// page-table entries so CPL=3 instruction/data fetches are not rejected at
/// the PML4/PDPT/PD level (x86-64 requires U=1 at every level for user access).
pub fn map_page_into_space(cr3: u64, va: u64, pa: u64, flags: u64) -> bool {
    let user_bit = PageFlag::User as u64;
    let pml4 = page_table::phys_to_virt_table(cr3 & FRAME_MASK) as *mut Table;
    unsafe {
        let pdpt = get_or_alloc_child(pml4, levels::pml4_index(va));
        if pdpt.is_null() {
            return false;
        }
        // Propagate User bit into the PML4 entry so user-mode walks succeed.
        if (flags & user_bit) != 0 {
            let idx = levels::pml4_index(va);
            let e = (*pml4).get(idx);
            (*pml4).set(idx, e | user_bit);
        }
        let pd = get_or_alloc_pd(pdpt, levels::pdpt_index(va));
        if pd.is_null() {
            return false;
        }
        // Propagate User bit into the PDPT entry.
        if (flags & user_bit) != 0 {
            let idx = levels::pdpt_index(va);
            let e = (*pdpt).get(idx);
            (*pdpt).set(idx, e | user_bit);
        }
        let pt = get_or_alloc_pt(pd, levels::pd_index(va));
        if pt.is_null() {
            return false;
        }
        // Propagate User bit into the PD entry.
        if (flags & user_bit) != 0 {
            let idx = levels::pd_index(va);
            let e = (*pd).get(idx);
            (*pd).set(idx, e | user_bit);
        }
        let entry = (pa & FRAME_MASK) | (flags & !FRAME_MASK);
        (*pt).set(levels::pt_index(va), entry);
        true
    }
}

/// Map one 2MB page in the given address space. va and pa must be 2MB-aligned.
pub fn map_page_2mb_into_space(cr3: u64, va: u64, pa: u64, flags: u64) -> bool {
    if (va & (PAGE_SIZE_2MB as u64 - 1)) != 0 || (pa & (PAGE_SIZE_2MB as u64 - 1)) != 0 {
        return false;
    }
    let pml4 = page_table::phys_to_virt_table(cr3 & FRAME_MASK) as *mut Table;
    unsafe {
        let pdpt = get_or_alloc_child(pml4, levels::pml4_index(va));
        if pdpt.is_null() {
            return false;
        }
        let pd = get_or_alloc_pd(pdpt, levels::pdpt_index(va));
        if pd.is_null() {
            return false;
        }
        let idx = levels::pd_index(va);
        let e = (*pd).get(idx);
        if (e & PageFlag::Present as u64) != 0 && (e & PDE_PS) == 0 {
            return false;
        }
        (*pd).set(idx, (pa & PDE_2MB_FRAME_MASK) | (flags & !FRAME_MASK) | PDE_PS);
        true
    }
}

/// Map one 1GB page in the given address space. va and pa must be 1GB-aligned.
pub fn map_page_1gb_into_space(cr3: u64, va: u64, pa: u64, flags: u64) -> bool {
    if (va & (PAGE_SIZE_1GB as u64 - 1)) != 0 || (pa & (PAGE_SIZE_1GB as u64 - 1)) != 0 {
        return false;
    }
    let pml4 = page_table::phys_to_virt_table(cr3 & FRAME_MASK) as *mut Table;
    unsafe {
        let pdpt = get_or_alloc_child(pml4, levels::pml4_index(va));
        if pdpt.is_null() {
            return false;
        }
        let idx = levels::pdpt_index(va);
        let e = (*pdpt).get(idx);
        if (e & PageFlag::Present as u64) != 0 && (e & PDPTE_PS) == 0 {
            return false;
        }
        (*pdpt).set(idx, (pa & PDPTE_1GB_FRAME_MASK) | (flags & !FRAME_MASK) | PDPTE_PS);
        true
    }
}

const TWO_MB: u64 = 2 * 1024 * 1024;

/// Relocate pd (and pdpt if needed) out of [base_pa, base_pa+2MB) so we can split the 2MB page without losing the table.
unsafe fn relocate_tables_outside_2mb(
    pml4: *mut Table,
    mut pdpt: *mut Table,
    mut pd: *mut Table,
    va: u64,
    base_pa: u64,
) -> (*mut Table, *mut Table) {
    let flags = EntryFlags::empty().with(PageFlag::Present).with(PageFlag::Writable).with(PageFlag::User);
    let pdpt_pa = pdpt as u64;
    if pdpt_pa >= base_pa && pdpt_pa < base_pa + TWO_MB {
        // #region agent log
        #[cfg(not(test))]
        { crate::arch::x86_64::serial::write_str("[DBG] reloc pdpt in range\r\n"); }
        // #endregion
        if let Some(new_pdpt_pa) = alloc_table_page() {
            let new_pdpt = page_table::phys_to_virt_table(new_pdpt_pa);
            core::ptr::write_bytes(new_pdpt as *mut u8, 0, PAGE_SIZE);
            for i in 0..levels::ENTRY_COUNT {
                (*new_pdpt).set(i, (*pdpt).get(i));
            }
            (*pml4).set(levels::pml4_index(va), (new_pdpt_pa & FRAME_MASK) | flags.as_u64());
            tlb::flush_all();
            pdpt = new_pdpt;
            // #region agent log
            #[cfg(not(test))]
            { crate::arch::x86_64::serial::write_str("[DBG] reloc pdpt done\r\n"); }
            // #endregion
        }
    }
    let pd_pa = pd as u64;
    if pd_pa >= base_pa && pd_pa < base_pa + TWO_MB {
        // #region agent log
        #[cfg(not(test))]
        { crate::arch::x86_64::serial::write_str("[DBG] reloc pd in range\r\n"); }
        // #endregion
        if let Some(new_pd_pa) = alloc_table_page() {
            let new_pd = page_table::phys_to_virt_table(new_pd_pa);
            core::ptr::write_bytes(new_pd as *mut u8, 0, PAGE_SIZE);
            for i in 0..levels::ENTRY_COUNT {
                (*new_pd).set(i, (*pd).get(i));
            }
            (*pdpt).set(levels::pdpt_index(va), (new_pd_pa & FRAME_MASK) | flags.as_u64());
            tlb::flush_all();
            pd = new_pd;
            // #region agent log
            #[cfg(not(test))]
            { crate::arch::x86_64::serial::write_str("[DBG] reloc pd done\r\n"); }
            // #endregion
        }
    }
    (pdpt, pd)
}

/// Map one page in the current address space.
pub fn map_page(va: u64, pa: u64, flags: u64) -> bool {
    // #region agent log
    #[cfg(not(test))]
    if pa == 0x1780000 {
        crate::arch::x86_64::serial::write_str("[DBG] map_page va=");
        crate::arch::x86_64::serial::write_hex(va);
        crate::arch::x86_64::serial::write_str("\r\n");
    }
    // #endregion
    let cr3 = tlb::read_cr3();
    let pml4 = page_table::phys_to_virt_table(cr3 & FRAME_MASK) as *mut Table;
    unsafe {
        let pdpt = get_or_alloc_child(pml4, levels::pml4_index(va));
        if pdpt.is_null() {
            return false;
        }
        let mut pd = get_or_alloc_pd(pdpt, levels::pdpt_index(va));
        if pd.is_null() {
            return false;
        }
        let e = (*pd).get(levels::pd_index(va));
        // #region agent log
        #[cfg(not(test))]
        if pa == 0x1780000 {
            crate::arch::x86_64::serial::write_str("[DBG] e=");
            crate::arch::x86_64::serial::write_hex(e);
            crate::arch::x86_64::serial::write_str(" pd=");
            crate::arch::x86_64::serial::write_hex(pd as u64);
            crate::arch::x86_64::serial::write_str("\r\n");
        }
        // #endregion
        if (e & PageFlag::Present as u64) != 0 && (e & PDE_PS) != 0 {
            let base_pa = e & PDE_2MB_FRAME_MASK;
            // #region agent log
            #[cfg(not(test))]
            if pa == 0x1780000 {
                crate::arch::x86_64::serial::write_str("[DBG] reloc base_pa=");
                crate::arch::x86_64::serial::write_hex(base_pa);
                crate::arch::x86_64::serial::write_str("\r\n");
            }
            // #endregion
            let (_pdpt2, pd2) = relocate_tables_outside_2mb(pml4, pdpt, pd, va, base_pa);
            pd = pd2;
            // #region agent log
            #[cfg(not(test))]
            if pa == 0x1780000 {
                crate::arch::x86_64::serial::write_str("[DBG] after_reloc pd=");
                crate::arch::x86_64::serial::write_hex(pd as u64);
                crate::arch::x86_64::serial::write_str("\r\n");
            }
            // #endregion
        }
        let pt = get_or_alloc_pt(pd, levels::pd_index(va));
        if pt.is_null() {
            return false;
        }
        // #region agent log
        #[cfg(not(test))]
        if pa == 0x1780000 {
            crate::arch::x86_64::serial::write_str("[DBG] pt=");
            crate::arch::x86_64::serial::write_hex(pt as u64);
            crate::arch::x86_64::serial::write_str(" set_pte\r\n");
        }
        // #endregion
        // Preserve both the low 12 flag bits AND bit 63 (NX/NoExec).
        (*pt).set(levels::pt_index(va), (pa & FRAME_MASK) | (flags & !FRAME_MASK));
        // #region agent log
        #[cfg(not(test))]
        if pa == 0x1780000 {
            crate::arch::x86_64::serial::write_str("[DBG] map_page done\r\n");
        }
        // #endregion
        true
    }
}

/// Map in current space then restore CR3 (for mapping in another space by temporarily switching).
pub fn map_page_in_space(cr3: u64, va: u64, pa: u64, flags: u64) -> bool {
    let current = tlb::read_cr3();
    unsafe { tlb::write_cr3(cr3) };
    let ok = map_page(va, pa, flags);
    unsafe { tlb::write_cr3(current) };
    ok
}

#[inline]
pub fn map_page_identity(pa: u64, flags: u64) -> bool {
    map_page(pa, pa, flags)
}

/// Unmap one page in current address space.
pub fn unmap_page(va: u64) {
    let cr3 = tlb::read_cr3();
    let pml4 = page_table::phys_to_virt_table(cr3 & FRAME_MASK) as *mut Table;
    unsafe {
        let e = (*pml4).get(levels::pml4_index(va));
        if (e & PageFlag::Present as u64) == 0 {
            return;
        }
        let pdpt = page_table::phys_to_virt_table(e & FRAME_MASK) as *mut Table;
        let e = (*pdpt).get(levels::pdpt_index(va));
        if (e & PageFlag::Present as u64) == 0 {
            return;
        }
        let pd = page_table::phys_to_virt_table(e & FRAME_MASK) as *mut Table;
        let e = (*pd).get(levels::pd_index(va));
        if (e & PageFlag::Present as u64) == 0 {
            return;
        }
        let pt = page_table::phys_to_virt_table(e & FRAME_MASK) as *mut Table;
        (*pt).set(levels::pt_index(va), 0);
    }
}

/// Unmap a single 4K page in the given address space (by CR3).
pub fn unmap_page_in_space(cr3: u64, va: u64) {
    let pml4 = page_table::phys_to_virt_table(cr3 & FRAME_MASK) as *mut Table;
    unsafe {
        let e = (*pml4).get(levels::pml4_index(va));
        if (e & PageFlag::Present as u64) == 0 { return; }
        let pdpt = page_table::phys_to_virt_table(e & FRAME_MASK) as *mut Table;
        let e = (*pdpt).get(levels::pdpt_index(va));
        if (e & PageFlag::Present as u64) == 0 { return; }
        let pd = page_table::phys_to_virt_table(e & FRAME_MASK) as *mut Table;
        let e = (*pd).get(levels::pd_index(va));
        if (e & PageFlag::Present as u64) == 0 { return; }
        let pt = page_table::phys_to_virt_table(e & FRAME_MASK) as *mut Table;
        (*pt).set(levels::pt_index(va), 0);
    }
}

/// Identity-map a physical range (current space). Uses 1GB, 2MB, then 4KB pages where aligned.
pub fn identity_map_range(pa_start: u64, len: usize) -> bool {
    let flags = EntryFlags::kernel_rw().as_u64();
    let cr3 = tlb::read_cr3();
    let mut pa = pa_start & !(PAGE_SIZE as u64 - 1);
    let end = pa_start.saturating_add(len as u64);
    while pa < end {
        if (pa & (PAGE_SIZE_1GB as u64 - 1)) == 0 && end >= pa + PAGE_SIZE_1GB as u64 {
            if !map_page_1gb_into_space(cr3, pa, pa, flags) {
                return false;
            }
            pa += PAGE_SIZE_1GB as u64;
            continue;
        }
        if (pa & (PAGE_SIZE_2MB as u64 - 1)) == 0 && end >= pa + PAGE_SIZE_2MB as u64 {
            if !map_page_2mb_into_space(cr3, pa, pa, flags) {
                let mut pa_4k = pa;
                let end_2mb = pa + PAGE_SIZE_2MB as u64;
                while pa_4k < end_2mb {
                    if !map_page_into_space(cr3, pa_4k, pa_4k, flags) {
                        return false;
                    }
                    pa_4k += PAGE_SIZE as u64;
                }
            }
            pa += PAGE_SIZE_2MB as u64;
            continue;
        }
        if !map_page_into_space(cr3, pa, pa, flags) {
            return false;
        }
        pa += PAGE_SIZE as u64;
    }
    true
}

/// Allocate a new empty top-level table (PML4). Returns physical address or 0.
pub fn alloc_pml4() -> u64 {
    alloc_table_page().unwrap_or(0)
}

/// Create new address space: one zeroed PML4. Returns CR3 value or 0.
pub fn new_address_space() -> u64 {
    if let Some(pa) = alloc_table_page() {
        unsafe {
            core::ptr::write_bytes(page_table::phys_to_virt_table(pa) as *mut u8, 0, PAGE_SIZE);
        }
        pa
    } else {
        0
    }
}

// ── Per-process CR3 ────────────────────────────────────────────────────────

/// Saved after paging::init() so every new process can inherit kernel mappings.
static mut KERNEL_CR3: u64 = 0;

/// Called by paging::init() after the kernel CR3 is live.
pub fn set_kernel_cr3(cr3: u64) {
    unsafe { KERNEL_CR3 = cr3; }
}

/// The kernel's CR3, for use by diagnostics and process creation.
pub fn get_kernel_cr3() -> u64 {
    unsafe { KERNEL_CR3 }
}

/// PCID counter — simple bump allocator (1-4094; 0 = no PCID).
static mut PCID_NEXT: u16 = 1;

/// Allocate a unique PCID for a new process. Wraps after 4094.
pub fn alloc_pcid() -> u16 {
    unsafe {
        let p = PCID_NEXT;
        PCID_NEXT = if PCID_NEXT >= 4094 { 1 } else { PCID_NEXT + 1 };
        p
    }
}

/// Allocate a fresh per-process address space that shares kernel PML4 mappings.
/// Returns the new CR3 value, or 0 on allocation failure.
pub fn create_process_cr3() -> u64 {
    let kernel_cr3 = unsafe { KERNEL_CR3 };
    if kernel_cr3 == 0 {
        // KERNEL_CR3 not set yet — fall back to sharing current CR3 (bootstrap only).
        #[cfg(not(test))]
        crate::arch::x86_64::serial::write_str("[PAGING] create_process_cr3: KERNEL_CR3 not set, using read_cr3\r\n");
        return crate::memory::paging::tlb::read_cr3();
    }

    unsafe {
        let kernel_pml4 = page_table::phys_to_virt_table(kernel_cr3 & FRAME_MASK) as *mut Table;

        // 1. Allocate + copy fresh PML4 (shares kernel higher-half entries).
        let new_pml4_pa = match alloc_table_page() { Some(p) => p, None => return 0 };
        let new_pml4 = page_table::phys_to_virt_table(new_pml4_pa) as *mut Table;
        core::ptr::copy_nonoverlapping(kernel_pml4 as *const u8, new_pml4 as *mut u8, PAGE_SIZE);

        // 2. Deep-copy PML4[0] → fresh PDPT so user modifications don't touch kernel's PDPT.
        let kpml4_e0 = (*kernel_pml4).get(0);
        if (kpml4_e0 & PageFlag::Present as u64) != 0 {
            let kernel_pdpt = page_table::phys_to_virt_table(kpml4_e0 & FRAME_MASK) as *mut Table;

            let new_pdpt_pa = match alloc_table_page() { Some(p) => p, None => return 0 };
            let new_pdpt = page_table::phys_to_virt_table(new_pdpt_pa) as *mut Table;
            core::ptr::copy_nonoverlapping(kernel_pdpt as *const u8, new_pdpt as *mut u8, PAGE_SIZE);

            let flags0 = kpml4_e0 & !FRAME_MASK;
            (*new_pml4).set(0, (new_pdpt_pa & FRAME_MASK) | flags0);

            // 3. Deep-copy PDPT[0] → fresh PD; zero PD[5] (user load base = 0xA00000 → pd_idx 5).
            let kpdpt_e0 = (*kernel_pdpt).get(0);
            if (kpdpt_e0 & PageFlag::Present as u64) != 0 && (kpdpt_e0 & PDPTE_PS) == 0 {
                let kernel_pd = page_table::phys_to_virt_table(kpdpt_e0 & FRAME_MASK) as *mut Table;

                let new_pd_pa = match alloc_table_page() { Some(p) => p, None => return 0 };
                let new_pd = page_table::phys_to_virt_table(new_pd_pa) as *mut Table;
                core::ptr::copy_nonoverlapping(kernel_pd as *const u8, new_pd as *mut u8, PAGE_SIZE);

                // Zero PD[5] — the 2MB range 0xA00000-0xBFFFFF where user ELFs load
                // (USER_LOAD_BASE = 0xA00000). The kernel .bss ends at ~0x695218, so
                // PD[0..4] all carry kernel identity mappings that must stay intact.
                // PML4[255] (user stack, 0x7fff_...) is cleared separately below.
                (*new_pd).set(5, 0);

                let flags1 = kpdpt_e0 & !FRAME_MASK;
                (*new_pdpt).set(0, (new_pd_pa & FRAME_MASK) | flags1);
            }
        }

        // 4. Clear PML4[255] (user stack range 0x7fff_...) for isolation.
        (*new_pml4).set(255, 0);

        new_pml4_pa
    }
}

/// Walk page tables and return the leaf entry if present (PTE, or PDE with PS, or PDPTE with PS).
pub fn walk_pte(cr3: u64, va: u64) -> Option<u64> {
    let pml4 = page_table::phys_to_virt_table(cr3 & FRAME_MASK) as *mut Table;
    unsafe {
        let e = (*pml4).get(levels::pml4_index(va));
        if (e & PageFlag::Present as u64) == 0 {
            return None;
        }
        let pdpt = page_table::phys_to_virt_table(e & FRAME_MASK) as *mut Table;
        let e = (*pdpt).get(levels::pdpt_index(va));
        if (e & PageFlag::Present as u64) == 0 {
            return None;
        }
        if (e & PDPTE_PS) != 0 {
            return Some(e);
        }
        let pd = page_table::phys_to_virt_table(e & FRAME_MASK) as *mut Table;
        let e = (*pd).get(levels::pd_index(va));
        if (e & PageFlag::Present as u64) == 0 {
            return None;
        }
        if (e & PDE_PS) != 0 {
            return Some(e);
        }
        let pt = page_table::phys_to_virt_table(e & FRAME_MASK) as *mut Table;
        let pte = (*pt).get(levels::pt_index(va));
        if (pte & PageFlag::Present as u64) == 0 {
            return None;
        }
        Some(pte)
    }
}

/// Translate VA to PA in the address space given by cr3. Handles 4KB, 2MB, and 1GB pages.
pub fn translate_in_space(cr3: u64, va: u64) -> Option<u64> {
    let pml4 = page_table::phys_to_virt_table(cr3 & FRAME_MASK) as *mut Table;
    unsafe {
        let e = (*pml4).get(levels::pml4_index(va));
        if (e & PageFlag::Present as u64) == 0 {
            return None;
        }
        let pdpt = page_table::phys_to_virt_table(e & FRAME_MASK) as *mut Table;
        let e = (*pdpt).get(levels::pdpt_index(va));
        if (e & PageFlag::Present as u64) == 0 {
            return None;
        }
        if (e & PDPTE_PS) != 0 {
            return Some((e & PDPTE_1GB_FRAME_MASK) + (va & 0x3FFF_FFFF));
        }
        let pd = page_table::phys_to_virt_table(e & FRAME_MASK) as *mut Table;
        let e = (*pd).get(levels::pd_index(va));
        if (e & PageFlag::Present as u64) == 0 {
            return None;
        }
        if (e & PDE_PS) != 0 {
            return Some((e & PDE_2MB_FRAME_MASK) + (va & 0x1F_FFFF));
        }
        let pt = page_table::phys_to_virt_table(e & FRAME_MASK) as *mut Table;
        let e = (*pt).get(levels::pt_index(va));
        if (e & PageFlag::Present as u64) == 0 {
            return None;
        }
        Some((e & FRAME_MASK) + (va & 0xFFF))
    }
}

/// Translate VA to PA in current address space. Handles 4KB, 2MB, and 1GB pages.
pub fn translate(va: u64) -> Option<u64> {
    translate_in_space(tlb::read_cr3(), va)
}

#[cfg(not(test))]
pub fn dump_ptes_for_vas_serial(cr3: u64, vas: &[u64]) {
    for &va in vas {
        let va_page = levels::page_align_down(va);
        match (walk_pte(cr3, va_page), translate_in_space(cr3, va_page)) {
            (Some(pte), Some(pa)) => {
                crate::arch::x86_64::serial::write_str("[PTE] va=");
                crate::arch::x86_64::serial::write_hex(va_page);
                crate::arch::x86_64::serial::write_str(" pa=");
                crate::arch::x86_64::serial::write_hex(pa);
                crate::arch::x86_64::serial::write_str(" fl=");
                crate::arch::x86_64::serial::write_hex(pte & 0xFFF);
                crate::arch::x86_64::serial::write_str(if (pte & PageFlag::Present as u64) != 0 { " P" } else { "" });
                crate::arch::x86_64::serial::write_str(if (pte & PageFlag::Writable as u64) != 0 { " W" } else { "" });
                crate::arch::x86_64::serial::write_str(if (pte & PageFlag::User as u64) != 0 { " U" } else { "" });
                crate::arch::x86_64::serial::write_str(if (pte & PageFlag::NoExec as u64) != 0 { " NX" } else { " X" });
                crate::arch::x86_64::serial::write_str("\r\n");
            }
            (None, _) | (_, None) => {
                crate::arch::x86_64::serial::write_str("[PTE] va=");
                crate::arch::x86_64::serial::write_hex(va_page);
                crate::arch::x86_64::serial::write_str(" not_present\r\n");
            }
        }
    }
}
#[cfg(test)]
pub fn dump_ptes_for_vas_serial(_cr3: u64, _vas: &[u64]) {}

#[cfg(not(test))]
pub fn dump_ptes_range_serial(cr3: u64, va_start: u64, va_end: u64) {
    let start = levels::page_align_down(va_start);
    let end = levels::page_align_up(va_end);
    let mut va = start;
    while va < end {
        match (walk_pte(cr3, va), translate_in_space(cr3, va)) {
            (Some(pte), Some(pa)) => {
                crate::arch::x86_64::serial::write_str("[PTE] va=");
                crate::arch::x86_64::serial::write_hex(va);
                crate::arch::x86_64::serial::write_str(" pa=");
                crate::arch::x86_64::serial::write_hex(pa);
                crate::arch::x86_64::serial::write_str(" fl=");
                crate::arch::x86_64::serial::write_hex(pte & 0xFFF);
                crate::arch::x86_64::serial::write_str(if (pte & PageFlag::Present as u64) != 0 { " P" } else { "" });
                crate::arch::x86_64::serial::write_str(if (pte & PageFlag::Writable as u64) != 0 { " W" } else { "" });
                crate::arch::x86_64::serial::write_str(if (pte & PageFlag::User as u64) != 0 { " U" } else { "" });
                crate::arch::x86_64::serial::write_str(if (pte & PageFlag::NoExec as u64) != 0 { " NX" } else { " X" });
                crate::arch::x86_64::serial::write_str("\r\n");
            }
            (None, _) | (_, None) => {
                crate::arch::x86_64::serial::write_str("[PTE] va=");
                crate::arch::x86_64::serial::write_hex(va);
                crate::arch::x86_64::serial::write_str(" not_present\r\n");
            }
        }
        va += PAGE_SIZE as u64;
    }
}
#[cfg(test)]
pub fn dump_ptes_range_serial(_cr3: u64, _va_start: u64, _va_end: u64) {}

#[cfg(not(test))]
pub fn debug_walk_in_space(cr3: u64, va: u64) {
    use crate::arch::x86_64::serial;

    serial::write_str("[PTW] va=");
    serial::write_hex(va);
    serial::write_str(" cr3=");
    serial::write_hex(cr3 & FRAME_MASK);
    serial::write_str("\r\n");

    unsafe {
        let pml4 = page_table::phys_to_virt_table(cr3 & FRAME_MASK) as *mut Table;
        let l4_idx = levels::pml4_index(va);
        let l4e = (*pml4).get(l4_idx);
        serial::write_str("[PTW]  L4 idx=");
        serial::write_hex(l4_idx as u64);
        serial::write_str(" e=");
        serial::write_hex(l4e);
        serial::write_str("\r\n");
        if (l4e & PageFlag::Present as u64) == 0 {
            serial::write_str("[PTW]  L4 not present\r\n");
            return;
        }

        let pdpt = page_table::phys_to_virt_table(l4e & FRAME_MASK) as *mut Table;
        let l3_idx = levels::pdpt_index(va);
        let l3e = (*pdpt).get(l3_idx);
        serial::write_str("[PTW]  L3 idx=");
        serial::write_hex(l3_idx as u64);
        serial::write_str(" e=");
        serial::write_hex(l3e);
        serial::write_str("\r\n");
        if (l3e & PageFlag::Present as u64) == 0 {
            serial::write_str("[PTW]  L3 not present\r\n");
            return;
        }
        if (l3e & PDPTE_PS) != 0 {
            let pa = (l3e & PDPTE_1GB_FRAME_MASK) + (va & 0x3FFF_FFFF);
            serial::write_str("[PTW]  1GB leaf pa=");
            serial::write_hex(pa);
            serial::write_str("\r\n");
            return;
        }

        let pd = page_table::phys_to_virt_table(l3e & FRAME_MASK) as *mut Table;
        let l2_idx = levels::pd_index(va);
        let l2e = (*pd).get(l2_idx);
        serial::write_str("[PTW]  L2 idx=");
        serial::write_hex(l2_idx as u64);
        serial::write_str(" e=");
        serial::write_hex(l2e);
        serial::write_str("\r\n");
        if (l2e & PageFlag::Present as u64) == 0 {
            serial::write_str("[PTW]  L2 not present\r\n");
            return;
        }
        if (l2e & PDE_PS) != 0 {
            let pa = (l2e & PDE_2MB_FRAME_MASK) + (va & 0x1F_FFFF);
            serial::write_str("[PTW]  2MB leaf pa=");
            serial::write_hex(pa);
            serial::write_str("\r\n");
            return;
        }

        let pt = page_table::phys_to_virt_table(l2e & FRAME_MASK) as *mut Table;
        let l1_idx = levels::pt_index(va);
        let l1e = (*pt).get(l1_idx);
        serial::write_str("[PTW]  L1 idx=");
        serial::write_hex(l1_idx as u64);
        serial::write_str(" e=");
        serial::write_hex(l1e);
        serial::write_str("\r\n");
        if (l1e & PageFlag::Present as u64) == 0 {
            serial::write_str("[PTW]  L1 not present\r\n");
            return;
        }
        let pa = (l1e & FRAME_MASK) + (va & 0xFFF);
        serial::write_str("[PTW]  4K leaf pa=");
        serial::write_hex(pa);
        serial::write_str("\r\n");
    }
}

#[cfg(not(test))]
pub fn debug_walk(va: u64) {
    let cr3 = tlb::read_cr3();
    debug_walk_in_space(cr3, va);
}

/// Kernel load address (linker . = 1M). Identity-map this range in our CR3 so we don't touch UEFI tables.
const KERNEL_IDENTITY_START: u64 = 0x100000;
/// Map 8MB (kernel + .bss + PT_POOL + stack). Stack is set at kernel_main entry to _stack_top so it stays in range.
const KERNEL_IDENTITY_LEN: u64 = 8 * 1024 * 1024;

pub fn init() {
    #[cfg(not(test))]
    crate::arch::x86_64::serial::write_str("[KRN] paging_init_start\r\n");
    // If physical allocator was already inited from BootInfo (init_from_bootinfo), skip default init.
    if !crate::memory::physical::buddy::inited() {
        crate::memory::physical::frame_allocator::init();
    }
    // Build our own CR3 and identity-map the kernel so we never write to UEFI's (read-only) page tables.
    let our_cr3 = new_address_space();
    if our_cr3 == 0 {
        #[cfg(not(test))]
        crate::arch::x86_64::serial::write_str("[KRN] new_address_space failed\r\n");
        return;
    }
    let kflags = EntryFlags::kernel_rw().as_u64();
    // Use 4K pages so we only touch our new tables (2MB path can hit "already present" in some setups).
    let mut pa = KERNEL_IDENTITY_START;
    while pa < KERNEL_IDENTITY_START + KERNEL_IDENTITY_LEN {
        if !map_page_into_space(our_cr3, pa, pa, kflags) {
            #[cfg(not(test))]
            {
                crate::arch::x86_64::serial::write_str("[KRN] kernel identity map failed pa=");
                crate::arch::x86_64::serial::write_hex(pa);
                crate::arch::x86_64::serial::write_str("\r\n");
            }
            return;
        }
        pa += PAGE_SIZE as u64;
    }
    unsafe { tlb::write_cr3(our_cr3) };
    // Save KERNEL_CR3 so per-process address spaces can inherit kernel mappings.
    set_kernel_cr3(our_cr3);
    #[cfg(not(test))]
    {
        crate::arch::x86_64::serial::write_str("[KRN] switched to kernel CR3\r\n");
        // Debug: print PT_POOL_NEXT (no memory dereference needed)
        unsafe {
            crate::arch::x86_64::serial::write_str("[KRN] post-cr3 PT_POOL_NEXT=");
            crate::arch::x86_64::serial::write_hex(PT_POOL_NEXT as u64);
            crate::arch::x86_64::serial::write_str(" our_cr3=");
            crate::arch::x86_64::serial::write_hex(our_cr3);
            crate::arch::x86_64::serial::write_str("\r\n");
        }
    }

    let (start, len) = crate::memory::physical::frame_allocator::region();
    let ok = identity_map_range(start, len);
    #[cfg(not(test))]
    {
        unsafe {
            let pml4 = our_cr3 as *const u64;
            let pdpt_pa = *pml4 & FRAME_MASK;
            let pdpt = pdpt_pa as *const u64;
            let pdpt0 = *pdpt;
            crate::arch::x86_64::serial::write_str("[KRN] post-identity-map: PDPT[0]=");
            crate::arch::x86_64::serial::write_hex(pdpt0);
            crate::arch::x86_64::serial::write_str(" PT_POOL_NEXT=");
            crate::arch::x86_64::serial::write_hex(PT_POOL_NEXT as u64);
            crate::arch::x86_64::serial::write_str("\r\n");
        }
    }
    #[cfg(not(test))]
    crate::arch::x86_64::serial::write_str(
        if ok {
            "[KRN] identity_map_range done ok=1\r\n"
        } else {
            "[KRN] identity_map_range done ok=0\r\n"
        },
    );
    if !ok {
        crate::kernel::diagnostic::diagnostic_halt("identity_map_range_fail");
    }
    tlb::flush_all();
    // Ensure the first frame page at `start` is mapped RW in the active kernel CR3 before buddy writes to it.
    let cr3_now = tlb::read_cr3();
    let kflags = EntryFlags::kernel_rw().as_u64();
    if !map_page_into_space(cr3_now, start, start, kflags) {
        crate::kernel::diagnostic::diagnostic_halt("map_start_frame_fail");
    }
    tlb::flush_address(tlb::VirtAddr::new(start));
    crate::memory::physical::buddy::build_initial_freelist();
    #[cfg(not(test))]
    crate::arch::x86_64::serial::write_str("[KRN] paging_init_done\r\n");
}
