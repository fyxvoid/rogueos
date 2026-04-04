//! Slab page: partition a single page into fixed-size objects and link as free list.

use core::ptr;

const PAGE_SIZE: usize = 4096;

/// Partition a page (at `page_ptr`, must be PAGE_SIZE aligned) into objects of
/// `object_size` (at least 8 for link) and `object_align`. Links them into a free list
/// and returns the head. Does not modify any global state.
/// Returns head of free list (each object's first 8 bytes = next pointer, last = null).
pub unsafe fn partition_page_into_list(
    page_ptr: *mut u8,
    object_size: usize,
    object_align: usize,
) -> *mut u8 {
    let size = object_size.max(8);
    let mut head: *mut u8 = ptr::null_mut();
    let mut addr = page_ptr as usize;
    let end = page_ptr as usize + PAGE_SIZE;
    while addr + size <= end {
        let aligned = (addr + object_align - 1) & !(object_align - 1);
        if aligned + size > end {
            break;
        }
        let obj = aligned as *mut u8;
        ptr::write(obj as *mut *mut u8, head);
        head = obj;
        addr = aligned + size;
    }
    head
}
