//! Slab-style object allocator for kernel objects. Uses the global allocator
//! for backing pages and hands out fixed-size objects with low fragmentation.

use core::alloc::Layout;
use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

const LIVE_CANARY: u64 = 0xC0FFEE00_CAFEBABE;
const FREE_CANARY: u64 = 0xDEADDEAD_BADC0DE5;

/// A slab cache for objects of a given layout. Objects are carved from
/// the global allocator and reused when freed.
pub struct SlabCache {
    object_size: usize,
    object_align: usize,
    /// Free list head: pointer to first free block; each block's first 8 bytes are next pointer.
    free_head: AtomicPtr<u8>,
}

impl SlabCache {
    /// Create a cache for objects with the given size and alignment.
    pub const fn new(object_size: usize, object_align: usize) -> Self {
        SlabCache {
            object_size,
            object_align,
            free_head: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Allocate one object. Returns null if allocation fails.
    pub fn alloc(&self) -> *mut u8 {
        // SAFETY: free_head and object pointers are from this cache or alloc; single-threaded kernel.
        unsafe {
            let head = self.free_head.load(Ordering::SeqCst);
            if !head.is_null() {
                // Verify free-canary at end-of-object before reuse.
                if self.object_size >= 8 {
                    let canary_ptr = head.add(self.object_size - 8) as *const u64;
                    let c = core::ptr::read_volatile(canary_ptr);
                    if c != FREE_CANARY {
                        crate::kernel::diagnostic::diagnostic_halt("slab_canary_corrupt_on_alloc");
                    }
                    core::ptr::write_volatile(head.add(self.object_size - 8) as *mut u64, LIVE_CANARY);
                }
                let next = *(head as *const *mut u8);
                self.free_head.store(next, Ordering::SeqCst);
                return head;
            }
            let size = self.object_size.max(8); // need at least 8 for free list link
            let layout = match Layout::from_size_align(size, self.object_align) {
                Ok(l) => l,
                Err(_) => crate::kernel::diagnostic::diagnostic_halt("slab_layout_invalid"),
            };
            let ptr = alloc::alloc::alloc(layout);
            if ptr.is_null() {
                return ptr::null_mut();
            }
            if self.object_size >= 8 {
                core::ptr::write_volatile(ptr.add(self.object_size - 8) as *mut u64, LIVE_CANARY);
            }
            ptr
        }
    }

    /// Free one object. ptr must have been returned from alloc() on this cache.
    /// SAFETY: ptr must be a valid object pointer previously returned from self.alloc().
    pub unsafe fn free(&self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }
        if self.object_size >= 8 {
            let canary_ptr = ptr.add(self.object_size - 8) as *const u64;
            let c = core::ptr::read_volatile(canary_ptr);
            if c != LIVE_CANARY {
                crate::kernel::diagnostic::diagnostic_halt("slab_canary_corrupt_on_free");
            }
            core::ptr::write_volatile(ptr.add(self.object_size - 8) as *mut u64, FREE_CANARY);
        }
        let slot = ptr as *mut *mut u8;
        ptr::write(slot, self.free_head.load(Ordering::SeqCst));
        self.free_head.store(ptr, Ordering::SeqCst);
    }
}

/// Slab cache for process descriptors or other small kernel structs.
/// Object size 64 bytes (placeholder; adjust to match your ProcessDescriptor size if needed).
pub static PROCESS_SLAB: SlabCache = SlabCache::new(64, 8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slab_cache_alloc_free() {
        let cache = SlabCache::new(32, 8);
        let a = cache.alloc();
        assert!(!a.is_null());
        let b = cache.alloc();
        assert!(!b.is_null());
        // SAFETY: a from cache.alloc().
        unsafe { cache.free(a) };
        let c = cache.alloc();
        assert!(!c.is_null());
        // SAFETY: c, b from cache.alloc().
        unsafe { cache.free(c) };
        unsafe { cache.free(b) };
    }
}
