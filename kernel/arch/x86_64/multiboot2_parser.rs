//! Parse Multiboot2 info (physical address in EBX from GRUB), fill BootInfo at 0x8000,
//! then call kernel_main. Called from boot_multiboot2.S after switching to long mode.

use core::ptr::{read_unaligned, write_unaligned};
use libs::{BootInfo, BOOTINFO_PHYS_ADDR};
use crate::arch::x86_64::serial;
use crate::memory::physical::memmap::{MemoryDescriptor, EFI_CONVENTIONAL_MEMORY};

/// Convert multiboot2 memory map (tag 6) to UEFI-like descriptors and write at CONVERTED_MAP_ADDR.
const CONVERTED_MAP_ADDR: u64 = 0x9000;
const CONVERTED_MAP_MAX_BYTES: usize = 0x1000;
const PAGE_SIZE: u64 = 4096;

/// Multiboot2 info: total_size (u32), reserved (u32), then tags. Each tag: type (u32), size (u32).
const MB2_TAG_END: u32 = 0;
const MB2_TAG_MMAP: u32 = 6;
const MB2_TAG_FRAMEBUFFER: u32 = 8;
const MB2_TAG_ACPI_OLD: u32 = 14;
const MB2_TAG_ACPI_NEW: u32 = 15;

/// Multiboot2 memory map entry: base_addr (u64), length (u64), type (u32), reserved (u32).
/// Type 1 = available RAM.
const MB2_MMAP_AVAILABLE: u32 = 1;

#[no_mangle]
pub unsafe extern "C" fn multiboot2_entry(mbi_phys: u64) -> ! {
    serial::init();
    serial::write_str("[KRN] multiboot2_entry\r\n");

    let ptr = mbi_phys as *const u8;
    let total_size = read_unaligned(ptr as *const u32);
    if total_size < 8 {
        serial::write_str("[KRN] multiboot2 total_size too small\r\n");
        halt_loop();
    }
    let mut fb_base = 0u64;
    let mut fb_size = 0u64;
    let mut fb_width = 0u32;
    let mut fb_height = 0u32;
    let mut fb_stride = 0u32;
    let mut fb_bpp = 0u32;
    let mut rsdp_addr = 0u64;
    let mut mem_map_paddr = 0u64;
    let mut mem_map_size = 0u64;
    let mut mem_map_count = 0usize;

    let mut offset = 8u64;
    while offset + 8 <= total_size as u64 {
        let tag_type = read_unaligned(ptr.add(offset as usize) as *const u32);
        let tag_size = read_unaligned(ptr.add(offset as usize + 4) as *const u32);
        if tag_type == MB2_TAG_END && tag_size == 8 {
            break;
        }
        if tag_size < 8 {
            offset += 8;
            continue;
        }
        match tag_type {
            MB2_TAG_MMAP => {
                let entry_size = read_unaligned(ptr.add(offset as usize + 8) as *const u32);
                let _entry_version = read_unaligned(ptr.add(offset as usize + 12) as *const u32);
                if entry_size < 20 {
                    offset += (tag_size as u64 + 7) & !7;
                    continue;
                }
                let entries_start = offset + 16;
                let num_entries = (tag_size as u64 - 16) / entry_size as u64;
                let dst = CONVERTED_MAP_ADDR as *mut u8;
                let desc_size = core::mem::size_of::<MemoryDescriptor>();
                let max_entries = CONVERTED_MAP_MAX_BYTES / desc_size;
                let mut n = 0usize;
                for i in 0..num_entries {
                    if n >= max_entries {
                        break;
                    }
                    let ent_off = entries_start + i * entry_size as u64;
                    let base = read_unaligned(ptr.add(ent_off as usize) as *const u64);
                    let length = read_unaligned(ptr.add(ent_off as usize + 8) as *const u64);
                    let ty = read_unaligned(ptr.add(ent_off as usize + 16) as *const u32);
                    let page_count = (length + PAGE_SIZE - 1) / PAGE_SIZE;
                    let desc = dst.add(n * desc_size) as *mut MemoryDescriptor;
                    (*desc).ty = if ty == MB2_MMAP_AVAILABLE {
                        EFI_CONVENTIONAL_MEMORY
                    } else {
                        0
                    };
                    (*desc)._pad = 0;
                    (*desc).phys_start = base;
                    (*desc).virt_start = 0;
                    (*desc).page_count = page_count;
                    (*desc).att = 0;
                    n += 1;
                }
                mem_map_paddr = CONVERTED_MAP_ADDR;
                mem_map_size = (n * desc_size) as u64;
                mem_map_count = n;
            }
            MB2_TAG_FRAMEBUFFER => {
                if tag_size >= 8 + 8 + 4 + 4 + 4 + 1 + 1 + 1 {
                    fb_base = read_unaligned(ptr.add(offset as usize + 8) as *const u64);
                    fb_stride = read_unaligned(ptr.add(offset as usize + 16) as *const u32);
                    fb_width = read_unaligned(ptr.add(offset as usize + 20) as *const u32);
                    fb_height = read_unaligned(ptr.add(offset as usize + 24) as *const u32);
                    fb_bpp = read_unaligned(ptr.add(offset as usize + 28) as *const u8) as u32;
                    fb_size = (fb_stride as u64) * (fb_height as u64);
                }
            }
            MB2_TAG_ACPI_OLD | MB2_TAG_ACPI_NEW => {
                if rsdp_addr == 0 {
                    rsdp_addr = mbi_phys + offset + 8;
                }
            }
            _ => {}
        }
        offset = (offset + (tag_size as u64 + 7)) & !7;
    }

    let bi = BOOTINFO_PHYS_ADDR as *mut BootInfo;
    write_unaligned(
        bi,
        BootInfo {
            fb_base,
            fb_size,
            fb_width,
            fb_height,
            fb_stride,
            fb_bpp,
            nvme_bar: 0,
            boot_exit_tsc: 0,
            mem_map_paddr,
            mem_map_size,
            mem_desc_size: core::mem::size_of::<MemoryDescriptor>() as u32,
            mem_desc_version: 0,
            mem_map_valid: if mem_map_count > 0 { 0xC0DEF00D } else { 0 },
            mem_map_reserved: 0,
            rsdp_addr,
            bootloader_version: 0,
            _reserved: 0,
        },
    );

    serial::write_str("[KRN] multiboot2 BootInfo filled, calling kernel_main\r\n");
    crate::init::kernel_main(BOOTINFO_PHYS_ADDR as *const BootInfo);
}

#[inline(never)]
fn halt_loop() -> ! {
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)) };
    }
}
