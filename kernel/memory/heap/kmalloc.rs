//! Kernel malloc/free API. No heap internals used outside this module.

use crate::memory::heap::allocator::{alloc_with_header, free_with_header};

/// Allocate `size` bytes. Returns null on failure. Alignment is 8 minimum.
#[inline]
pub fn kmalloc(size: usize) -> *mut u8 {
    alloc_with_header(size, 8)
}

/// Free a pointer from kmalloc. No-op if ptr is null.
#[inline]
pub unsafe fn kfree(ptr: *mut u8) {
    free_with_header(ptr);
}
