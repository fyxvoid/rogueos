#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), deny(warnings))]
#![allow(static_mut_refs, dead_code, unused_imports, unused_mut, unused_unsafe, private_interfaces)]

//! Kernel is single-threaded and uses global state (`static mut`) by design. All such state is
//! owned by one execution context and is not shared across cores. Main subsystems that hold
//! globals: process table, paging pool (PT_POOL), heap, NVMe, TTY, VFS, display, page-fault PID
//! callback. Future work may consolidate or document per-subsystem state.

extern crate alloc;

pub mod arch;
pub mod capability;
pub mod iflow;
pub mod memory;
pub mod init;
pub mod kernel;
pub mod process;
pub mod syscall;
pub mod drivers;
pub mod display;
pub mod fs;

/// Linker symbols for kernel stack; used for RSP sanity in build_initial_freelist.
pub mod stack_bounds {
    extern "C" {
        pub static _stack_bottom: u8;
        pub static _stack_top: u8;
    }
    pub fn kernel_stack_bounds() -> (u64, u64) {
        unsafe {
            let b = &_stack_bottom as *const u8 as u64;
            let t = &_stack_top as *const u8 as u64;
            (b, t)
        }
    }
}

#[global_allocator]
static ALLOC: memory::heap::allocator::KernelAllocator = memory::heap::allocator::KernelAllocator;
