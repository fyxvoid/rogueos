//! Minimal ELF64 loader: parse PT_LOAD segments, map and copy into address space.
//!
//! Text segments get USER|PRESENT|executable (NX=0); data segments get USER|PRESENT|writable (NX=1).
//! For each PT_LOAD where memsz > filesz, region [filesz, memsz] is zeroed; first BSS byte checked after load.

use crate::memory::paging;
use crate::memory::physical;
use crate::memory::r#virtual as virt;
use crate::arch::x86_64::serial;

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const PT_LOAD: u32 = 1;
const PAGE_SIZE: usize = 4096;

/// Result of loading an ELF: entry and optional addresses for permission/BSS checks.
#[derive(Clone, Copy)]
pub struct LoadResult {
    pub entry: u64,
    /// File offset of entry point (for post-load checksum: compare bytes at entry VA with elf_data[this..]).
    pub entry_file_offset: Option<usize>,
    /// First page of first writable (data) PT_LOAD, for PTE assert: must be USER|PRESENT|WRITABLE|NO_EXEC.
    pub data_page_va: Option<u64>,
    /// First byte of first BSS region (p_vaddr + p_filesz for first segment with memsz > filesz); verified zero.
    pub bss_check_va: Option<u64>,
}

/// Load ELF64 from elf_data into the address space with CR3 = cr3.
/// Returns LoadResult with entry and optional data/bss addresses for validation, or None on error.
pub fn load_elf(elf_data: &[u8], cr3: u64) -> Option<LoadResult> {
    serial::write_str("[KRN] load_elf: start\r\n");
    if elf_data.len() < 64 {
        serial::write_str("[KRN] load_elf: too_short\r\n");
        return None;
    }
    if elf_data[0..4] != ELF_MAGIC {
        serial::write_str("[KRN] load_elf: bad_magic\r\n");
        return None;
    }
    if elf_data[4] != 2 {
        serial::write_str("[KRN] load_elf: not_64bit\r\n");
        return None; // not 64-bit
    }
    let e_type = u16::from_le_bytes(elf_data[0x10..0x12].try_into().ok()?);
    let e_machine = u16::from_le_bytes(elf_data[0x12..0x14].try_into().ok()?);
    if e_type != 2 && e_type != 3 {
        serial::write_str("[KRN] load_elf: unsupported_e_type\r\n");
        return None; // not ET_EXEC / ET_DYN
    }
    if e_machine != 0x3e {
        serial::write_str("[KRN] load_elf: not_EM_X86_64\r\n");
        return None; // not EM_X86_64
    }
    let e_entry = u64::from_le_bytes(elf_data[0x18..0x20].try_into().ok()?);
    let e_phoff = u64::from_le_bytes(elf_data[0x20..0x28].try_into().ok()?) as usize;
    let e_phentsize = u16::from_le_bytes(elf_data[0x36..0x38].try_into().ok()?) as usize;
    let e_phnum = u16::from_le_bytes(elf_data[0x38..0x3a].try_into().ok()?) as usize;

    serial::write_str("[KRN] load_elf: magic=7f454c46 e_entry=");
    serial::write_hex(e_entry);
    serial::write_str(" phnum=");
    serial::write_hex(e_phnum as u64);
    serial::write_str("\r\n");

    const FRAME_MASK: u64 = 0x000F_FFFF_FFFF_F000;

    let mut entry_inside_load = false;
    let mut entry_file_offset: Option<usize> = None;
    let mut first_pt_load_logged = false;
    let mut first_data_page_va: Option<u64> = None;
    let mut first_bss_check_va: Option<u64> = None;

    serial::write_str("[KRN] load_elf: mapping_segments\r\n");
    for i in 0..e_phnum {
        let ph_off = e_phoff + i * e_phentsize;
        if elf_data.len() < ph_off + 56 {
            serial::write_str("[KRN] load_elf: ph_trunc\r\n");
            continue;
        }
        let p_type = u32::from_le_bytes(elf_data[ph_off..ph_off + 4].try_into().ok()?);
        if p_type != PT_LOAD {
            continue;
        }
        let p_offset = u64::from_le_bytes(elf_data[ph_off + 8..ph_off + 16].try_into().ok()?) as usize;
        let p_vaddr = u64::from_le_bytes(elf_data[ph_off + 16..ph_off + 24].try_into().ok()?);
        let p_filesz = u64::from_le_bytes(elf_data[ph_off + 32..ph_off + 40].try_into().ok()?) as usize;
        let p_memsz = u64::from_le_bytes(elf_data[ph_off + 40..ph_off + 48].try_into().ok()?) as usize;
        let p_flags = u32::from_le_bytes(elf_data[ph_off + 4..ph_off + 8].try_into().ok()?);
        if p_memsz == 0 {
            continue;
        }
        if !first_pt_load_logged {
            first_pt_load_logged = true;
            serial::write_str("[KRN] load_elf: first_PT_LOAD vaddr=");
            serial::write_hex(p_vaddr);
            serial::write_str(" memsz=");
            serial::write_hex(p_memsz as u64);
            serial::write_str(" filesz=");
            serial::write_hex(p_filesz as u64);
            serial::write_str("\r\n");
        }
        if e_entry >= p_vaddr && e_entry < p_vaddr + p_memsz as u64 {
            entry_inside_load = true;
            entry_file_offset = Some(p_offset + (e_entry - p_vaddr) as usize);
        }
        if first_data_page_va.is_none() && (p_flags & 2) != 0 {
            first_data_page_va = Some(p_vaddr & !(PAGE_SIZE as u64 - 1));
        }
        if first_bss_check_va.is_none() && p_memsz > p_filesz {
            first_bss_check_va = Some(p_vaddr + p_filesz as u64);
        }
        let writable = (p_flags & 2) != 0; // PF_W
        let executable = (p_flags & 1) != 0; // PF_X
        if executable {
            serial::write_str("[KRN] load_elf: PT_LOAD exec segment vaddr=");
            serial::write_hex(p_vaddr);
            serial::write_str(" file_off=");
            serial::write_hex(p_offset as u64);
            serial::write_str(" file_size=");
            serial::write_hex(p_filesz as u64);
            serial::write_str(" mem_size=");
            serial::write_hex(p_memsz as u64);
            serial::write_str("\r\n");
        }
        let flags = {
            let mut f = paging::EntryFlags::empty()
                .with(paging::PageFlag::Present)
                .with(paging::PageFlag::User);
            if writable {
                f = f.with(paging::PageFlag::Writable);
            }
            if !executable {
                f = f.with(paging::PageFlag::NoExec);
            }
            f.as_u64()
        };
        let mut va = p_vaddr & !(PAGE_SIZE as u64 - 1);
        let end_va = p_vaddr + p_memsz as u64;
        let mut file_off = p_offset;
        while va < end_va {
            let file_bytes_left = if file_off < p_offset + p_filesz {
                (p_offset + p_filesz - file_off).min(elf_data.len().saturating_sub(file_off))
            } else {
                0
            };
            let page_bytes_left = (PAGE_SIZE as u64).min(end_va - va) as usize;
            let copy_len = core::cmp::min(page_bytes_left, file_bytes_left);
            // For the first page the segment may start mid-page; for all subsequent
            // pages the segment data starts at the page base (offset 0).
            let dest_offset = p_vaddr.saturating_sub(va) as usize;

            // Backing page must be a physical frame (for PTE); PT_POOL is for table pages only. Use buddy allocator.
            let (pa, _need_map, _existing_pte) = match paging::walk_pte(cr3, va) {
                Some(pte) => (pte & FRAME_MASK, false, Some(pte)),
                None => {
                    let Some(pa) = physical::alloc_frame() else {
                        serial::write_str("[KRN] load_elf: alloc_frame_failed\r\n");
                        return None;
                    };
                    (pa, true, None)
                }
            };

            if copy_len > 0 {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        elf_data.as_ptr().add(file_off),
                        (pa as *mut u8).add(dest_offset),
                        copy_len,
                    );
                }
            }
            if dest_offset + copy_len < PAGE_SIZE {
                let zero_start = dest_offset + copy_len;
                let zero_len = PAGE_SIZE - zero_start;
                unsafe {
                    core::ptr::write_bytes((pa as *mut u8).add(zero_start), 0, zero_len);
                }
            }

            // Always use the ELF segment flags directly. Do not OR with existing PTE
            // flags: the kernel identity map at the same VA has Writable=1, and ORing
            // would make user text segments writable, failing the post-load check.
            let map_flags = flags;
            if !virt::map_page_in_space(cr3, va, pa, map_flags) {
                serial::write_str("[KRN] load_elf: map_page_in_space_failed\r\n");
                return None;
            }
            // Validate first 16 bytes of executable segment at mapped VA vs file.
            if executable && va == (p_vaddr & !(PAGE_SIZE as u64 - 1)) && p_filesz >= 16
                && elf_data.len() >= p_offset + 16
            {
                let mapped_ptr = p_vaddr as *const u8;
                let mut match_ok = true;
                for i in 0..16 {
                    let file_byte = elf_data[p_offset + i];
                    let mem_byte = unsafe { *mapped_ptr.add(i) };
                    if file_byte != mem_byte {
                        serial::write_str("[KRN] load_elf: text copy mismatch at offset ");
                        serial::write_hex(i as u64);
                        serial::write_str(" file=");
                        serial::write_hex(file_byte as u64);
                        serial::write_str(" mem=");
                        serial::write_hex(mem_byte as u64);
                        serial::write_str("\r\n");
                        match_ok = false;
                    }
                }
                if !match_ok {
                    crate::kernel::diagnostic::diagnostic_halt("elf_text_copy_mismatch");
                }
                serial::write_str("[KRN] load_elf: first 16 bytes of exec segment match file\r\n");
            }
            va += PAGE_SIZE as u64;
            file_off = file_off.saturating_add(copy_len);
        }
    }
    if !entry_inside_load {
        serial::write_str("[KRN] load_elf: entry not inside any PT_LOAD\r\n");
        crate::kernel::diagnostic::diagnostic_halt("user_entry_outside_load_segment");
    }

    // Assert first BSS region reads zero before first user instruction.
    if let Some(bss_va) = first_bss_check_va {
        let ptr = bss_va as *const u64;
        let val = unsafe { core::ptr::read_volatile(ptr) };
        if val != 0 {
            serial::write_str("[KRN] load_elf: BSS check failed at va=");
            serial::write_hex(bss_va);
            serial::write_str(" val=");
            serial::write_hex(val);
            serial::write_str("\r\n");
            crate::kernel::diagnostic::diagnostic_halt("bss_not_zero");
        }
        serial::write_str("[KRN] load_elf: BSS check ok (8 bytes zero at first BSS)\r\n");
    }

    serial::write_str("[KRN] load_elf: done\r\n");
    Some(LoadResult {
        entry: e_entry,
        entry_file_offset,
        data_page_va: first_data_page_va,
        bss_check_va: first_bss_check_va,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_minimal_elf64() -> [u8; 64] {
        let mut buf = [0u8; 64];
        buf[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
        buf[4] = 2; // 64-bit
        buf[0x10..0x12].copy_from_slice(&2u16.to_le_bytes()); // e_type ET_EXEC
        buf[0x12..0x14].copy_from_slice(&0x3eu16.to_le_bytes()); // e_machine EM_X86_64
        buf[0x18..0x20].copy_from_slice(&0x400_000u64.to_le_bytes()); // e_entry
        buf[0x20..0x28].copy_from_slice(&0x40u64.to_le_bytes()); // e_phoff
        buf[0x36..0x38].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
        buf[0x38..0x3a].copy_from_slice(&0u16.to_le_bytes()); // e_phnum = 0 (no PT_LOAD)
        buf
    }

    #[test]
    fn test_load_elf_too_short() {
        assert!(load_elf(&[], 0).is_none());
        assert!(load_elf(&[0u8; 63], 0).is_none());
    }

    #[test]
    fn test_load_elf_wrong_magic() {
        let mut buf = make_minimal_elf64();
        buf[0] = 0;
        assert!(load_elf(&buf, 0).is_none());
    }

    #[test]
    fn test_load_elf_wrong_class() {
        let mut buf = make_minimal_elf64();
        buf[4] = 1; // 32-bit
        assert!(load_elf(&buf, 0).is_none());
    }

    #[test]
    fn test_load_elf_wrong_type() {
        let mut buf = make_minimal_elf64();
        buf[0x10..0x12].copy_from_slice(&1u16.to_le_bytes()); // ET_REL
        assert!(load_elf(&buf, 0).is_none());
    }

    #[test]
    fn test_load_elf_wrong_machine() {
        let mut buf = make_minimal_elf64();
        buf[0x12..0x14].copy_from_slice(&0x28u16.to_le_bytes()); // EM_ARM
        assert!(load_elf(&buf, 0).is_none());
    }
}
