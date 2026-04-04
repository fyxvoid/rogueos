//! Kernel binary entry: set stack, then call rust_entry.
//! When feature "multiboot2" is enabled, GRUB uses boot_multiboot2.S entry instead.
#![no_std]
#![no_main]
#![allow(static_mut_refs)]

// Ensure lib is linked (panic_handler, multiboot2_entry, etc.).
extern crate kernel;

/// Multiboot 1 header for QEMU -kernel (must be in first 8KB of image).
/// Omitted when building for GRUB multiboot2 (header and entry come from boot_multiboot2.S).
#[cfg(not(feature = "multiboot2"))]
#[link_section = ".multiboot2"]
#[used]
static MULTIBOOT_HEADER: [u32; 3] = [
    0x1BADB002,           // magic
    0,                    // flags
    (-0x1BADB002i32) as u32,  // checksum: -(magic + flags)
];

#[cfg(not(feature = "multiboot2"))]
static mut BOOT_STACK: [u8; 0x4000] = [0; 0x4000];

#[cfg(not(feature = "multiboot2"))]
#[no_mangle]
pub extern "C" fn _start() -> ! {
    let stack_top = unsafe { BOOT_STACK.as_ptr().add(0x4000) as u64 };
    unsafe {
        core::arch::asm!(
            "mov rsp, {}",
            in(reg) stack_top,
            options(nostack),
        );
    }
    unsafe { kernel::arch::x86_64::rust_entry() }
}
