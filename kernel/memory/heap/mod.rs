pub mod allocator;
pub mod cache;
pub mod kmalloc;
pub mod slab;

pub use allocator::{init as heap_init, dump_state_serial, KernelAllocator};
pub use kmalloc::{kmalloc, kfree};
