//! Per-size object cache. Backed by pages from the pool; free list for O(1) alloc/free.

use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

use crate::memory::heap::allocator::alloc_page;
use crate::memory::heap::slab;

/// Cache for one object size. Free list: first 8 bytes of each free object = next.
pub struct Cache {
    object_size: usize,
    object_align: usize,
    free_head: AtomicPtr<u8>,
}

impl Cache {
    pub const fn new(object_size: usize, object_align: usize) -> Self {
        Self {
            object_size: if object_size >= 8 { object_size } else { 8 },
            object_align,
            free_head: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Allocate one object. Returns null on failure.
    pub fn alloc(&self) -> *mut u8 {
        unsafe {
            let head = self.free_head.load(Ordering::SeqCst);
            if !head.is_null() {
                let next = ptr::read(head as *const *mut u8);
                self.free_head.store(next, Ordering::SeqCst);
                return head;
            }
            let page = alloc_page();
            if page.is_null() {
                return ptr::null_mut();
            }
            let list = slab::partition_page_into_list(page, self.object_size, self.object_align);
            if list.is_null() {
                crate::memory::heap::allocator::free_page(page);
                return ptr::null_mut();
            }
            let next = ptr::read(list as *const *mut u8);
            self.free_head.store(next, Ordering::SeqCst);
            list
        }
    }

    /// Return one object to the cache. ptr must have been from alloc() on this cache.
    pub unsafe fn free(&self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }
        ptr::write(ptr as *mut *mut u8, self.free_head.load(Ordering::SeqCst));
        self.free_head.store(ptr, Ordering::SeqCst);
    }
}
