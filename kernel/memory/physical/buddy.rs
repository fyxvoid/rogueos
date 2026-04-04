//! Power-of-two block allocator with coalescing on free.
//! Manages a contiguous physical region; each order k holds blocks of 2^k pages.
//! Free lists per order; explicit merge when freeing a block whose buddy is free.
//!
//! **Identity-mapping requirement:** The frame region must be identity-mapped in the kernel's
//! address space (physical address == virtual address). `phys_to_virt(pa)` assumes that.

use core::ptr;

use crate::memory::physical::layout::{
    PAGE_SIZE, MAX_ORDER, FRAME_REGION_BASE, FRAME_REGION_PAGES,
};

/// Free list link stored at the start of each free block (physical address of next).
#[repr(C)]
struct FreeLink {
    next: u64,
}

/// Physical to virtual for frame region. Requires identity mapping of frame region.
fn phys_to_virt(pa: u64) -> *mut u8 {
    pa as *mut u8
}

/// Maximum region size (pages) we support; bitmap is sized for this.
pub(crate) const MAX_REGION_PAGES: usize = 65536; // 256 MiB
const BITMAP_WORDS: usize = (MAX_REGION_PAGES + 63) / 64;

static mut REGION_START: u64 = FRAME_REGION_BASE;
static mut REGION_PAGES: usize = FRAME_REGION_PAGES;
static mut BUDDY_INITED: bool = false;

static mut FREE_LISTS: [u64; MAX_ORDER + 1] = [0; MAX_ORDER + 1];
static mut ALLOC_BITMAP: [u64; BITMAP_WORDS] = [0; BITMAP_WORDS];

fn region_start() -> u64 {
    unsafe { REGION_START }
}
fn region_pages() -> usize {
    unsafe { REGION_PAGES }
}

fn page_index(pa: u64) -> usize {
    let start = region_start();
    debug_assert!(pa >= start && (pa - start) % PAGE_SIZE as u64 == 0);
    ((pa - start) / PAGE_SIZE as u64) as usize
}

fn bitmap_test(idx: usize) -> bool {
    // SAFETY: idx is page_index bounded by REGION_PAGES; BITMAP_WORDS >= (REGION_PAGES+63)/64.
    unsafe {
        let w = idx / 64;
        let b = idx % 64;
        (ALLOC_BITMAP[w] & (1u64 << b)) != 0
    }
}

fn bitmap_set(idx: usize) {
    // SAFETY: same as bitmap_test; single writer to bitmap.
    unsafe {
        let w = idx / 64;
        let b = idx % 64;
        ALLOC_BITMAP[w] |= 1u64 << b;
    }
}

fn bitmap_clear(idx: usize) {
    // SAFETY: same as bitmap_test.
    unsafe {
        let w = idx / 64;
        let b = idx % 64;
        ALLOC_BITMAP[w] &= !(1u64 << b);
    }
}

fn mark_alloc_range(pa: u64, order: usize) {
    let start = page_index(pa);
    let len = 1usize << order;
    let rp = region_pages();
    for i in 0..len {
        let idx = start + i;
        if idx >= rp {
            crate::kernel::diagnostic::diagnostic_halt("buddy_alloc_oob");
        }
        if bitmap_test(idx) {
            crate::kernel::diagnostic::diagnostic_halt("buddy_double_alloc");
        }
        bitmap_set(idx);
    }
}

fn mark_free_range(pa: u64, order: usize) {
    let start = page_index(pa);
    let len = 1usize << order;
    let rp = region_pages();
    for i in 0..len {
        let idx = start + i;
        if idx >= rp {
            crate::kernel::diagnostic::diagnostic_halt("buddy_free_oob");
        }
        if !bitmap_test(idx) {
            crate::kernel::diagnostic::diagnostic_halt("buddy_double_free");
        }
        bitmap_clear(idx);
    }
}

fn buddy_of(pa: u64, order: usize) -> u64 {
    let start = region_start();
    let size_pages = 1 << order;
    let size_bytes = (size_pages * PAGE_SIZE) as u64;
    let idx = (pa - start) / size_bytes;
    let buddy_idx = idx ^ 1;
    start + buddy_idx * size_bytes
}

fn pop_free(order: usize) -> u64 {
    // SAFETY: FREE_LISTS and identity-mapped frame region; head is valid pa in region.
    unsafe {
        let head = FREE_LISTS[order];
        if head == 0 {
            return 0;
        }
        let ptr = phys_to_virt(head) as *const FreeLink;
        let next = (*ptr).next;
        FREE_LISTS[order] = next;
        head
    }
}

fn push_free(pa: u64, order: usize) {
    let next = unsafe { FREE_LISTS[order] };
    #[cfg(not(test))]
    {
        let block_size = (1 << order) * PAGE_SIZE;
        let region_end = region_start() + (region_pages() * PAGE_SIZE) as u64;
        crate::arch::x86_64::serial::write_str("[physical] push_free start=");
        crate::arch::x86_64::serial::write_hex(pa);
        crate::arch::x86_64::serial::write_str(" order=");
        crate::arch::x86_64::serial::write_hex(order as u64);
        crate::arch::x86_64::serial::write_str(" block_size=");
        crate::arch::x86_64::serial::write_hex(block_size as u64);
        crate::arch::x86_64::serial::write_str(" next=");
        crate::arch::x86_64::serial::write_hex(next);
        crate::arch::x86_64::serial::write_str("\r\n");
        if next != 0 && (next < region_start() || next >= region_end) {
            crate::arch::x86_64::serial::write_str("[physical] push_free next outside region\r\n");
            crate::kernel::diagnostic::diagnostic_halt("buddy_next_ptr_oob");
        }
    }
    // SAFETY: pa is from alloc_order in our region; identity-mapped.
    unsafe {
        let ptr = phys_to_virt(pa) as *mut FreeLink;
        ptr::write(ptr, FreeLink { next });
        FREE_LISTS[order] = pa;
    }
}

fn remove_from_free_list(pa: u64, order: usize) -> bool {
    // SAFETY: cur/prev are free-list physical addrs in identity-mapped region.
    unsafe {
        let mut prev = 0u64;
        let mut cur = FREE_LISTS[order];
        while cur != 0 {
            if cur == pa {
                if prev == 0 {
                    let ptr = phys_to_virt(cur) as *const FreeLink;
                    FREE_LISTS[order] = (*ptr).next;
                } else {
                    let prev_ptr = phys_to_virt(prev) as *mut FreeLink;
                    let cur_ptr = phys_to_virt(cur) as *const FreeLink;
                    (*prev_ptr).next = (*cur_ptr).next;
                }
                return true;
            }
            prev = cur;
            let ptr = phys_to_virt(cur) as *const FreeLink;
            cur = (*ptr).next;
        }
        false
    }
}

/// Initialize the buddy allocator: entire region as one free block at max order that fits.
/// Uses layout constants (fixed 2 MiB base, 8 MiB). Call init_with_region for BootInfo-driven init.
pub fn init() {
    init_with_region(FRAME_REGION_BASE, FRAME_REGION_PAGES);
}

/// Initialize with a specific physical region (e.g. from UEFI memory map).
/// Does not touch region memory (no push_free); call build_initial_freelist() after the region is identity-mapped.
pub fn init_with_region(start: u64, pages: usize) {
    if pages == 0 || pages > MAX_REGION_PAGES {
        crate::kernel::diagnostic::diagnostic_halt("buddy_region_invalid");
    }
    if start & (PAGE_SIZE as u64 - 1) != 0 {
        crate::kernel::diagnostic::diagnostic_halt("buddy_region_unaligned");
    }
    // SAFETY: single init; we own statics. FREE_LISTS and ALLOC_BITMAP are already zero by static init.
    unsafe {
        REGION_START = start;
        REGION_PAGES = pages;
        BUDDY_INITED = true;
    }
    #[cfg(not(test))]
    crate::arch::x86_64::serial::write_str("[physical] buddy_init_done\r\n");
}

/// Build the initial free list (one block at max order). Call once after the frame region is identity-mapped.
pub fn build_initial_freelist() {
    #[cfg(not(test))]
    {
        let rsp: u64;
        unsafe { core::arch::asm!("mov {}, rsp", out(reg) rsp, options(nostack)); }
        let (stack_bottom, stack_top) = crate::stack_bounds::kernel_stack_bounds();
        crate::arch::x86_64::serial::write_str("[physical] build_freelist_start RSP=");
        crate::arch::x86_64::serial::write_hex(rsp);
        crate::arch::x86_64::serial::write_str(" stack_bottom=");
        crate::arch::x86_64::serial::write_hex(stack_bottom);
        crate::arch::x86_64::serial::write_str(" stack_top=");
        crate::arch::x86_64::serial::write_hex(stack_top);
        crate::arch::x86_64::serial::write_str("\r\n");
        if rsp < stack_bottom || rsp > stack_top {
            crate::arch::x86_64::serial::write_str("[physical] RSP outside kernel stack\r\n");
            crate::kernel::diagnostic::diagnostic_halt("buddy_rsp_oob");
        }
    }
    let start = region_start();
    let pages = region_pages();
    #[cfg(not(test))]
    {
        use crate::memory::paging::mapper::debug_walk;
        crate::arch::x86_64::serial::write_str("[physical] build_freelist region_start=");
        crate::arch::x86_64::serial::write_hex(start);
        crate::arch::x86_64::serial::write_str(" pages=");
        crate::arch::x86_64::serial::write_hex(pages as u64);
        crate::arch::x86_64::serial::write_str("\r\n");
        debug_walk(start);
    }
    // STEP 1 — Canary before push_free: write pattern, read back, log.
    const CANARY: u64 = 0xCAFEBABECAFED00D;
    #[cfg(not(test))]
    {
        unsafe {
            let ptr = start as *mut u64;
            core::ptr::write_volatile(ptr, CANARY);
            let read_back = core::ptr::read_volatile(ptr);
            crate::arch::x86_64::serial::write_str("[physical] canary write then read: wrote ");
            crate::arch::x86_64::serial::write_hex(CANARY);
            crate::arch::x86_64::serial::write_str(" read ");
            crate::arch::x86_64::serial::write_hex(read_back);
            crate::arch::x86_64::serial::write_str("\r\n");
            if read_back != CANARY {
                crate::arch::x86_64::serial::write_str("[physical] canary mismatch -> aliasing/overlap\r\n");
                crate::kernel::diagnostic::diagnostic_halt("buddy_canary_mismatch");
            }
        }
    }
    let mut order = 0;
    while (1 << (order + 1)) <= pages {
        order += 1;
    }
    push_free(start, order);
    #[cfg(not(test))]
    {
        use crate::memory::paging::mapper::debug_walk;
        debug_walk(start);
        unsafe {
            let ptr = start as *const u8;
            crate::arch::x86_64::serial::write_str("[physical] after push_free first 16 bytes at start: ");
            for i in 0..16 {
                crate::arch::x86_64::serial::write_hex(*ptr.add(i) as u64);
                crate::arch::x86_64::serial::write_str(" ");
            }
            crate::arch::x86_64::serial::write_str("\r\n");
        }
    }
    #[cfg(not(test))]
    crate::arch::x86_64::serial::write_str("[physical] build_freelist_done\r\n");
}

/// Returns true if the buddy was initialized (from init or init_with_region).
pub fn inited() -> bool {
    unsafe { BUDDY_INITED }
}

/// Allocate 2^order physical pages. Returns physical address or 0.
pub fn alloc_order(order: usize) -> u64 {
    if !inited() {
        return 0;
    }
    if order > MAX_ORDER {
        return 0;
    }
    let mut k = order;
    loop {
        let pa = pop_free(k);
        if pa != 0 {
            while k > order {
                k -= 1;
                let half = (1 << k) * PAGE_SIZE;
                push_free(pa + half as u64, k);
            }
            mark_alloc_range(pa, order);
            return pa;
        }
        k += 1;
        if k > MAX_ORDER {
            return 0;
        }
    }
}

/// Allocate one physical frame (order 0). Returns None when no memory available.
pub fn alloc_frame() -> Option<u64> {
    let pa = alloc_order(0);
    if pa == 0 {
        None
    } else {
        Some(pa)
    }
}

/// Free 2^order physical pages at pa. pa must be from alloc_order.
pub fn free_order(pa: u64, order: usize) {
    if order > MAX_ORDER {
        return;
    }
    let start = region_start();
    if pa < start || (pa - start) % PAGE_SIZE as u64 != 0 {
        crate::kernel::diagnostic::diagnostic_halt("buddy_free_invalid_pa");
    }
    mark_free_range(pa, order);
    let buddy = buddy_of(pa, order);
    if remove_from_free_list(buddy, order) {
        let merged = if pa < buddy { pa } else { buddy };
        free_order(merged, order + 1);
    } else {
        push_free(pa, order);
    }
}

/// Free one physical frame (order 0).
pub fn free_frame(pa: u64) {
    free_order(pa, 0);
}

/// Return the physical region (start, size in bytes) for identity mapping.
pub fn region() -> (u64, usize) {
    (region_start(), region_pages() * PAGE_SIZE)
}

pub fn dump_state_serial() {
    crate::arch::x86_64::serial::write_str("[DIAG][BUDDY] free_lists:");
    // SAFETY: read-only traversal of free lists; cur in identity-mapped region.
    unsafe {
        for order in 0..=MAX_ORDER {
            let mut cnt = 0u64;
            let mut cur = FREE_LISTS[order];
            while cur != 0 {
                cnt += 1;
                let ptr = phys_to_virt(cur) as *const FreeLink;
                cur = (*ptr).next;
                if cnt > 10_000 {
                    break;
                }
            }
            crate::arch::x86_64::serial::write_str(" o");
            crate::arch::x86_64::serial::write_hex(order as u64);
            crate::arch::x86_64::serial::write_str("=");
            crate::arch::x86_64::serial::write_hex(cnt);
        }
    }
    crate::arch::x86_64::serial::write_str("\r\n");
}
