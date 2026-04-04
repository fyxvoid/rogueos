//! Physical frame allocation: trait and default buddy-based implementation.

use crate::memory::physical::layout::{PAGE_SIZE, MAX_ORDER};
use crate::memory::physical::buddy;
use crate::memory::physical::memmap;

/// Physical address. Opaque wrapper for type safety.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalAddress(pub u64);

impl PhysicalAddress {
    pub const fn new(pa: u64) -> Self {
        Self(pa)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub fn is_aligned(self) -> bool {
        (self.0 & (PAGE_SIZE as u64 - 1)) == 0
    }
}

/// A single page-sized frame (order 0).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub start: PhysicalAddress,
}

impl Frame {
    pub fn new(pa: PhysicalAddress) -> Self {
        Self { start: pa }
    }

    pub fn as_u64(self) -> u64 {
        self.start.0
    }
}

/// Allocation order: 2^order pages per block. Order 0 = one page.
#[derive(Clone, Copy, Debug)]
pub struct AllocationOrder(pub usize);

impl AllocationOrder {
    pub const fn order_0() -> Self {
        Self(0)
    }

    pub fn as_usize(self) -> usize {
        self.0
    }

    pub fn page_count(self) -> usize {
        1 << self.0
    }
}

/// Trait for physical frame allocation. Implementations are backend-specific.
pub trait FrameAllocator {
    /// Allocate 2^order contiguous physical pages. Returns None on failure.
    fn allocate(&self, order: AllocationOrder) -> Option<Frame>;
    /// Free a frame previously returned by allocate. Order must match.
    fn free(&self, frame: Frame, order: AllocationOrder);
}

/// Default global frame allocator (buddy).
pub struct BuddyFrameAllocator;

impl FrameAllocator for BuddyFrameAllocator {
    fn allocate(&self, order: AllocationOrder) -> Option<Frame> {
        if order.0 == 0 {
            buddy::alloc_frame().map(|pa| Frame::new(PhysicalAddress::new(pa)))
        } else {
            let pa = buddy::alloc_order(order.0);
            if pa == 0 {
                None
            } else {
                Some(Frame::new(PhysicalAddress::new(pa)))
            }
        }
    }

    fn free(&self, frame: Frame, order: AllocationOrder) {
        buddy::free_order(frame.as_u64(), order.0);
    }
}

/// Global instance. Used by paging and virtual mapping.
pub static FRAME_ALLOCATOR: BuddyFrameAllocator = BuddyFrameAllocator;

/// Mark the frame region as initialized (fixed layout). Call once after identity-mapping the region.
/// Prefer init_from_bootinfo when BootInfo with memory map is available.
pub fn init() {
    buddy::init();
}

/// Initialize the physical allocator from the UEFI memory map in BootInfo.
/// Parses EfiConventionalMemory, reserves kernel/fb/memmap/NVMe/ACPI, picks one conventional
/// range, and initializes the buddy. Call before paging::init() so the frame region is known;
/// paging::init() will identity-map it.
/// Returns true if initialization succeeded, false if map invalid or no suitable range.
pub fn init_from_bootinfo(bi: &libs::BootInfo) -> bool {
    if bi.mem_map_valid != 0xC0DEF00D {
        return false;
    }
    let reserved = memmap::reserved_from_bootinfo(
        bi.fb_base,
        bi.fb_size,
        bi.mem_map_paddr,
        bi.mem_map_size,
        bi.nvme_bar,
        bi.rsdp_addr,
    );
    let (start, pages) = match memmap::choose_conventional_region(
        bi.mem_map_paddr,
        bi.mem_map_size,
        bi.mem_desc_size,
        &reserved,
    ) {
        Some(r) => r,
        None => return false,
    };
    // Buddy bitmap is sized for MAX_REGION_PAGES; cap so we don't halt.
    let pages_capped = core::cmp::min(pages, buddy::MAX_REGION_PAGES);
    memmap::log_chosen_region(start, pages_capped);
    buddy::init_with_region(start, pages_capped);
    true
}

/// Allocate one physical frame. Returns None when no memory available.
pub fn alloc_frame() -> Option<u64> {
    buddy::alloc_frame()
}

/// Allocate n contiguous physical pages. Returns physical address or 0.
/// May allocate a larger power-of-two block (2^order >= n).
/// For 2 MiB large pages use alloc_contiguous(512) (future: dedicated PMD support).
pub fn alloc_contiguous(n: usize) -> u64 {
    if n == 0 {
        return 0;
    }
    let order = n.next_power_of_two().trailing_zeros() as usize;
    if order > MAX_ORDER {
        return 0;
    }
    buddy::alloc_order(order)
}

/// Free one physical frame. pa must be page-aligned and from alloc_frame.
pub fn free_frame(pa: u64) {
    buddy::free_frame(pa);
}

/// Return the physical region start and length for identity mapping.
pub fn region() -> (u64, usize) {
    buddy::region()
}
