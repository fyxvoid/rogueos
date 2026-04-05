//! UEFI bootloader: load kernel ELF from same volume, exit boot services, jump to kernel.

#![no_main]
#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::ptr;
use libs::{BootInfo, BOOTINFO_PHYS_ADDR};
use uefi::fs::FileSystem;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::table::boot::{AllocateType, MemoryDescriptor, MemoryType, ScopedProtocol, SearchType};
use uefi::table::cfg::{ACPI2_GUID, ACPI_GUID, SMBIOS3_GUID, SMBIOS_GUID};
use uefi::Identify;
use uefi::Status;

/// Write directly to COM1 (0x3F8) bypassing UEFI logging — visible in serial captures.
fn com1_puts(s: &[u8]) {
    for &b in s {
        unsafe {
            // Wait for Transmitter Holding Register Empty (LSR bit 5)
            loop {
                let lsr: u8;
                core::arch::asm!("in al, dx", in("dx") 0x3FDu16, out("al") lsr,
                    options(nomem, nostack, preserves_flags));
                if lsr & 0x20 != 0 { break; }
            }
            core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") b,
                options(nomem, nostack, preserves_flags));
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    log::error!("boot panic: {}", info);
    loop {}
}

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const PT_LOAD: u32 = 1;
const PAGE_SIZE: usize = 4096;
/// Number of pages reserved to store a copy of the UEFI memory map for the kernel.
/// 64 pages = 256 KiB, which is ample for typical systems.
const MEMMAP_PAGES: usize = 64;

/// Global fatal helper: print a structured error and halt.
fn fatal(msg: &core::fmt::Arguments<'_>) -> ! {
    // Best-effort: use the UEFI logger first.
    log::error!("[GATEHOUSE FATAL] {}", msg);

    // Finally, halt the CPU in a tight loop.
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}

#[inline]
fn rdtsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    }
    ((hi as u64) << 32) | (lo as u64)
}

#[entry]
fn main(handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut system_table).unwrap();

    // Write directly to COM1 (0x3F8) so serial capture confirms bootloader ran.
    com1_puts(b"[BOOT] gatehouse entry\r\n");

    log::info!("[BOOT] uefi_entry");
    log::info!("custom_kernel boot: loading kernel...");

    // Capture config table entries (ACPI RSDP, SMBIOS) while still in boot services.
    let rsdp_addr = find_rsdp(&system_table);
    let smbios_addr = find_smbios(&system_table);

    let bs = system_table.boot_services();
    let kernel_elf = match load_kernel_file(bs, handle) {
        Ok(data) => { com1_puts(b"[BOOT] kernel.elf loaded\r\n"); data }
        Err(e) => {
            com1_puts(b"[BOOT] ERROR: kernel.elf not found\r\n");
            log::error!("Failed to load \\kernel.elf: {:?}", e);
            return e.status();
        }
    };

    let entry_point = match load_elf(&kernel_elf, bs) {
        Ok(entry) => { com1_puts(b"[BOOT] ELF mapped\r\n"); entry }
        Err(code) => {
            com1_puts(b"[BOOT] ERROR: ELF load failed\r\n");
            log::error!("Failed to load ELF (code {}): magic=1 type=3/4 machine=4 ph=10+ allocate=20+", code);
            return Status::LOAD_ERROR;
        }
    };

    // Populate BootInfo with framebuffer details (best-effort).
    if let Err(e) = init_boot_info(bs) {
        log::warn!("BootInfo/GOP init failed: {:?}", e);
    }

    // Reserve a region to hold a copy of the UEFI memory map for the kernel.
    let (mem_map_paddr, mem_map_capacity) =
        reserve_memmap_storage(bs).unwrap_or_else(|status| fatal(&format_args!(
            "Failed to reserve memory for kernel memory map: {:?}",
            status
        )));

    log::info!(
        "Kernel ELF entry e_entry = 0x{:x}",
        entry_point as u64
    );
    unsafe {
        let p = entry_point as *const u8;
        let mut bytes: [u8; 16] = [0; 16];
        let mut i = 0;
        while i < 16 {
            bytes[i] = core::ptr::read_volatile(p.add(i));
            i += 1;
        }
        log::info!(
            "Kernel entry bytes: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} \
{:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15],
        );
    }
    log::info!(
        "Exiting boot services and jumping to kernel entry at 0x{:x}",
        entry_point as u64
    );
    // Record a best-effort timestamp immediately before ExitBootServices.
    unsafe {
        let dst = BOOTINFO_PHYS_ADDR as *mut BootInfo;
        let mut info = core::ptr::read_volatile(dst);
        info.boot_exit_tsc = rdtsc();
        core::ptr::write_volatile(dst, info);
    }

    // Capture EFI Runtime Services table address before ExitBootServices.
    // The Runtime Services remain callable post-EBS if their memory regions stay mapped.
    let runtime_services_addr = system_table.runtime_services() as *const _ as u64;
    com1_puts(b"[BOOT] exiting boot services\r\n");

    // ExitBootServices performs the required GetMemoryMap + ExitBootServices dance.
    let (_runtime, mmap) = system_table.exit_boot_services(MemoryType::LOADER_DATA);

    // After ExitBootServices, we may not call boot services any more.
    // Copy the memory descriptors into the reserved region and finalize BootInfo.
    finalize_boot_info(mem_map_paddr, mem_map_capacity, &mmap, rsdp_addr, smbios_addr, runtime_services_addr);

    // Direct COM1 write after ExitBootServices (no UEFI services available now)
    com1_puts(b"[BOOT] jumping to kernel\r\n");

    type KernelEntry = extern "sysv64" fn(*const BootInfo) -> !;
    let entry: KernelEntry = unsafe { core::mem::transmute(entry_point) };
    let bootinfo_ptr = BOOTINFO_PHYS_ADDR as *const BootInfo;

    unsafe {
        core::arch::asm!("cli", options(nomem, nostack));
        entry(bootinfo_ptr);
    }
}

/// Query GOP and write [`BootInfo`] to the well-known physical address.
fn init_boot_info(bs: &BootServices) -> uefi::Result<()> {
    let handle_buf = bs.locate_handle_buffer(SearchType::ByProtocol(&GraphicsOutput::GUID))?;
    let handle = *handle_buf
        .first()
        .ok_or_else(|| uefi::Error::from(Status::NOT_FOUND))?;

    let mut gop = bs.open_protocol_exclusive::<GraphicsOutput>(handle)?;
    let mode = gop.current_mode_info();
    let res = mode.resolution();
    let stride = mode.stride() as u32;
    let mut fb = gop.frame_buffer();
    let fb_base = fb.as_mut_ptr() as u64;
    let fb_size = fb.size() as u64;

    let info = BootInfo {
        fb_base,
        fb_size,
        fb_width: res.0 as u32,
        fb_height: res.1 as u32,
        fb_stride: stride,
        fb_bpp: 32,      // GOP typically exposes 32bpp; kernel will validate if needed.
        nvme_bar: 0,     // NVMe BAR; set by kernel PCI scan or future bootloader support.
        boot_exit_tsc: 0,
        mem_map_paddr: 0,
        mem_map_size: 0,
        mem_desc_size: 0,
        mem_desc_version: 0,
        mem_map_valid: 0,
        mem_map_reserved: 0,
        rsdp_addr: 0,
        bootloader_version: 0,
        _reserved: 0,
        smbios_addr: 0,
        runtime_services_addr: 0,
    };

    unsafe {
        let dst = BOOTINFO_PHYS_ADDR as *mut BootInfo;
        core::ptr::write_volatile(dst, info);
    }

    log::info!(
        "BootInfo: fb_base={:#x} size={} bytes {}x{} stride={} bpp={}",
        info.fb_base,
        info.fb_size,
        info.fb_width,
        info.fb_height,
        info.fb_stride,
        info.fb_bpp
    );

    Ok(())
}

fn load_kernel_file(bs: &BootServices, image_handle: Handle) -> uefi::Result<Vec<u8>> {
    let fs: ScopedProtocol<SimpleFileSystem> = bs.get_image_file_system(image_handle)?;
    let mut fs = FileSystem::new(fs);
    fs.read(uefi::cstr16!("\\kernel.elf")).map_err(|_| uefi::Error::from(Status::LOAD_ERROR))
}

/// Load ELF64 kernel: one page-aligned allocation for all PT_LOAD segments
/// (per UEFI AllocatePages(Address) requirement and common bootloader pattern).
fn load_elf(data: &[u8], bs: &BootServices) -> Result<usize, u32> {
    if data.len() < 64 || data[0..4] != ELF_MAGIC {
        return Err(1);
    }
    if data[4] != 2 {
        return Err(2); // not 64-bit
    }
    let e_type = u16::from_le_bytes(data[0x10..0x12].try_into().unwrap());
    let e_machine = u16::from_le_bytes(data[0x12..0x14].try_into().unwrap());
    // Accept both ET_EXEC (2) and ET_DYN (3) for the kernel image. Many Rust
    // kernels are linked as position-independent ET_DYN binaries even when
    // they are intended to behave like a freestanding executable.
    if e_type != 2 && e_type != 3 {
        return Err(3); // unsupported ELF type
    }
    if e_machine != 0x3e {
        return Err(4); // not EM_X86_64
    }

    let e_entry = u64::from_le_bytes(data[0x18..0x20].try_into().unwrap()) as usize;
    let e_phoff = u64::from_le_bytes(data[0x20..0x28].try_into().unwrap()) as usize;
    let e_phentsize = u16::from_le_bytes(data[0x36..0x38].try_into().unwrap()) as usize;
    let e_phnum = u16::from_le_bytes(data[0x38..0x3a].try_into().unwrap()) as usize;

    // Collect PT_LOAD segments and compute one page-aligned range (per rust-osdev / UEFI docs).
    let mut min_v = 0usize;
    let mut max_v = 0usize;
    let mut segments: alloc::vec::Vec<(usize, usize, usize, usize)> = Vec::new();

    for i in 0..e_phnum {
        let ph_off = e_phoff + i * e_phentsize;
        if data.len() < ph_off + 56 {
            return Err(10 + i as u32);
        }
        let p_type = u32::from_le_bytes(data[ph_off..ph_off + 4].try_into().unwrap());
        if p_type != PT_LOAD {
            continue;
        }
        let p_offset = u64::from_le_bytes(data[ph_off + 8..ph_off + 16].try_into().unwrap()) as usize;
        let p_vaddr = u64::from_le_bytes(data[ph_off + 16..ph_off + 24].try_into().unwrap()) as usize;
        let p_filesz = u64::from_le_bytes(data[ph_off + 32..ph_off + 40].try_into().unwrap()) as usize;
        let p_memsz = u64::from_le_bytes(data[ph_off + 40..ph_off + 48].try_into().unwrap()) as usize;
        if p_memsz == 0 {
            continue;
        }
        let end = p_vaddr + p_memsz;
        if segments.is_empty() {
            min_v = p_vaddr;
            max_v = end;
        } else {
            if p_vaddr < min_v {
                min_v = p_vaddr;
            }
            if end > max_v {
                max_v = end;
            }
        }
        segments.push((p_offset, p_vaddr, p_filesz, p_memsz));
    }

    if segments.is_empty() {
        return Err(15);
    }

    // Single page-aligned allocation for whole kernel (UEFI requires page-aligned Address).
    let alloc_start = min_v & !(PAGE_SIZE - 1);
    let alloc_end = (max_v + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let num_pages = (alloc_end - alloc_start) / PAGE_SIZE;

    bs.allocate_pages(
        AllocateType::Address(alloc_start as u64),
        MemoryType::LOADER_CODE,
        num_pages,
    )
    .map_err(|e| {
        log::error!(
            "allocate_pages range {:#x}..{:#x} ({} pages) failed: {:?}",
            alloc_start,
            alloc_end,
            num_pages,
            e
        );
        20u32
    })?;

    // Copy each segment to its vaddr (all within the allocated block).
    for (p_offset, p_vaddr, p_filesz, p_memsz) in segments {
        let copy_len = p_filesz.min(p_memsz);
        if p_offset + copy_len > data.len() {
            return Err(30);
        }
        let src = &data[p_offset..p_offset + copy_len];
        unsafe {
            ptr::copy_nonoverlapping(src.as_ptr(), p_vaddr as *mut u8, src.len());
            if p_memsz > p_filesz {
                ptr::write_bytes((p_vaddr + p_filesz) as *mut u8, 0, p_memsz - p_filesz);
            }
        }
    }

    Ok(e_entry)
}

/// Reserve a contiguous region of physical memory where the final UEFI memory map
/// will be copied for the kernel to consume.
fn reserve_memmap_storage(bs: &BootServices) -> Result<(u64, usize), Status> {
    let num_pages = MEMMAP_PAGES;
    bs.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, num_pages)
        .map(|phys| (phys, num_pages * PAGE_SIZE))
        .map_err(|e| e.status())
}

/// Scan the UEFI configuration table for the ACPI Root System Description Pointer.
fn find_rsdp(system_table: &SystemTable<Boot>) -> Option<u64> {
    for entry in system_table.config_table() {
        if entry.guid == ACPI2_GUID || entry.guid == ACPI_GUID {
            return Some(entry.address as u64);
        }
    }
    None
}

/// Scan the UEFI configuration table for the SMBIOS entry point.
/// Prefers SMBIOS 3.x (64-bit) over SMBIOS 2.x (32-bit). Returns physical address or None.
fn find_smbios(system_table: &SystemTable<Boot>) -> Option<u64> {
    let mut smbios2: Option<u64> = None;
    for entry in system_table.config_table() {
        if entry.guid == SMBIOS3_GUID {
            return Some(entry.address as u64); // SMBIOS 3.x preferred
        }
        if entry.guid == SMBIOS_GUID {
            smbios2 = Some(entry.address as u64);
        }
    }
    smbios2
}

/// Finalize [`BootInfo`] after `ExitBootServices` using the captured memory map and metadata.
fn finalize_boot_info(
    mem_map_paddr: u64,
    mem_map_capacity: usize,
    mmap: &uefi::table::boot::MemoryMap<'_>,
    rsdp_addr: Option<u64>,
    smbios_addr: Option<u64>,
    runtime_services_addr: u64,
) {
    // Copy descriptors into the reserved region as a packed array of MemoryDescriptor.
    let desc_size = core::mem::size_of::<MemoryDescriptor>();
    let count = mmap.entries().len();
    let required = count
        .checked_mul(desc_size)
        .expect("memory map size overflow");
    if required == 0 || required > mem_map_capacity {
        // Not enough space to store the full map; this is fatal for a production loader.
        fatal(&format_args!("[BOOT] invalid or too-large memory map (required={} capacity={})", required, mem_map_capacity));
    }

    let dst = mem_map_paddr as *mut MemoryDescriptor;
    for (i, desc) in mmap.entries().enumerate() {
        unsafe {
            core::ptr::write_volatile(dst.add(i), *desc);
        }
    }

    // Encode a simple bootloader version number (major.minor.patch packed as AABBCC).
    const VERSION_STR: &str = env!("CARGO_PKG_VERSION");
    let bootloader_version = encode_version(VERSION_STR);

    unsafe {
        let dst = BOOTINFO_PHYS_ADDR as *mut BootInfo;
        let mut info = core::ptr::read_volatile(dst);
        info.mem_map_paddr = mem_map_paddr;
        info.mem_map_size = required as u64;
        info.mem_desc_size = desc_size as u32;
        info.mem_desc_version = 0;
        info.mem_map_valid = 0xC0DEF00D;
        info.mem_map_reserved = 0;
        info.rsdp_addr = rsdp_addr.unwrap_or(0);
        info.bootloader_version = bootloader_version;
        info.smbios_addr = smbios_addr.unwrap_or(0);
        info.runtime_services_addr = runtime_services_addr;
        core::ptr::write_volatile(dst, info);
    }
}

/// Encode a semantic version string like \"0.1.0\" into a packed u32 AABBCC.
fn encode_version(s: &str) -> u32 {
    let mut parts = s.split('.');
    let major = parts.next().and_then(|p| p.parse::<u8>().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|p| p.parse::<u8>().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|p| p.parse::<u8>().ok()).unwrap_or(0);
    ((major as u32) << 16) | ((minor as u32) << 8) | (patch as u32)
}
