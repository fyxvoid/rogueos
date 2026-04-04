//! SLUB-style kernel heap: static pool, per-size caches, and page-sized large allocations.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;

use crate::memory::heap::cache::Cache;

pub const PAGE_SIZE: usize = 4096;
const NUM_PAGES: usize = 256;
const POOL_SIZE: usize = NUM_PAGES * PAGE_SIZE;

#[repr(align(4096))]
struct Pool([u8; POOL_SIZE]);

static mut POOL: Pool = Pool([0; POOL_SIZE]);
static mut PAGE_FREE_HEAD: *mut u8 = ptr::null_mut();
static mut INITED: bool = false;

/// Initialize the heap: link all pool pages into the free list. Call before first alloc.
pub fn init() {
    unsafe {
        if INITED {
            return;
        }
        for i in (0..NUM_PAGES).rev() {
            let page = POOL.0.as_ptr().add(i * PAGE_SIZE) as *mut u8;
            ptr::write(page as *mut *mut u8, PAGE_FREE_HEAD);
            PAGE_FREE_HEAD = page;
        }
        INITED = true;
    }
}

fn ensure_inited() {
    if !unsafe { INITED } {
        init();
    }
}

/// Allocate one page from the pool. Returns null if exhausted.
pub fn alloc_page() -> *mut u8 {
    ensure_inited();
    unsafe {
        let head = PAGE_FREE_HEAD;
        if head.is_null() {
            return ptr::null_mut();
        }
        PAGE_FREE_HEAD = ptr::read(head as *const *mut u8);
        head
    }
}

/// Return one page to the pool. Must be a pointer from alloc_page().
pub fn free_page(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        ptr::write(ptr as *mut *mut u8, PAGE_FREE_HEAD);
        PAGE_FREE_HEAD = ptr;
    }
}

const SIZE_CLASSES: [usize; 8] = [8, 16, 32, 64, 128, 256, 512, 1024];

fn size_to_class(size: usize) -> usize {
    for &class in &SIZE_CLASSES {
        if size <= class {
            return class;
        }
    }
    0
}

static CACHE_8: Cache = Cache::new(8, 8);
static CACHE_16: Cache = Cache::new(16, 8);
static CACHE_32: Cache = Cache::new(32, 8);
static CACHE_64: Cache = Cache::new(64, 8);
static CACHE_128: Cache = Cache::new(128, 8);
static CACHE_256: Cache = Cache::new(256, 8);
static CACHE_512: Cache = Cache::new(512, 8);
static CACHE_1024: Cache = Cache::new(1024, 8);

fn cache_for_class(class: usize) -> &'static Cache {
    match class {
        8 => &CACHE_8,
        16 => &CACHE_16,
        32 => &CACHE_32,
        64 => &CACHE_64,
        128 => &CACHE_128,
        256 => &CACHE_256,
        512 => &CACHE_512,
        1024 => &CACHE_1024,
        _ => &CACHE_1024,
    }
}

const HEADER_SIZE: usize = 16;

fn alloc_raw(total: usize) -> *mut u8 {
    let class = size_to_class(total);
    if class != 0 {
        cache_for_class(class).alloc()
    } else {
        alloc_page()
    }
}

unsafe fn free_raw(base: *mut u8, total: usize) {
    let class = size_to_class(total);
    if class != 0 {
        cache_for_class(class).free(base);
    } else {
        free_page(base);
    }
}

/// Allocate: 16-byte header [base, total], then user region (aligned). Returns user pointer.
pub fn alloc_with_header(size: usize, align: usize) -> *mut u8 {
    let align = align.max(8);
    let total = size + align + HEADER_SIZE;
    let base = alloc_raw(total);
    if base.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        let user_start = base.add(HEADER_SIZE);
        let aligned = (user_start as usize + align - 1) & !(align - 1);
        let aligned_ptr = aligned as *mut u8;
        ptr::write((aligned_ptr as *mut u8).sub(16) as *mut *mut u8, base);
        ptr::write((aligned_ptr as *mut u8).sub(8) as *mut usize, total);
        aligned_ptr
    }
}

/// Free. ptr must be the user pointer returned from alloc_with_header.
pub unsafe fn free_with_header(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    let base = ptr::read(ptr.sub(16) as *const *mut u8);
    let total = ptr::read(ptr.sub(8) as *const usize);
    free_raw(base, total);
}

pub struct KernelAllocator;

unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align().max(8);
        alloc_with_header(size, align)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        free_with_header(ptr);
    }
}

pub fn dump_state_serial() {
    ensure_inited();
    unsafe {
        let mut free_pages = 0;
        let mut p = PAGE_FREE_HEAD;
        while !p.is_null() {
            free_pages += 1;
            p = ptr::read(p as *const *mut u8);
        }
        crate::arch::x86_64::serial::write_str("[DIAG][HEAP] pool_pages_free=");
        crate::arch::x86_64::serial::write_hex(free_pages as u64);
        crate::arch::x86_64::serial::write_str(" total_pages=");
        crate::arch::x86_64::serial::write_hex(NUM_PAGES as u64);
        crate::arch::x86_64::serial::write_str("\r\n");
    }
}
