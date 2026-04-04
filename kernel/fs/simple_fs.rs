//! Minimal single-block volume header + file record table + data region. No subdirs; root only.

use crate::drivers::traits::BlockDevice;

const BLOCK_SIZE: usize = 4096;
const MAGIC: u32 = 0x5349_4D46; // "SIMF"
const FILE_TABLE_BLOCK: u32 = 1;
const DATA_START_BLOCK: u32 = 2;
const MAX_FILE_RECORDS: usize = 64;
const FILE_RECORD_SIZE: usize = 64; // name[32], size(u32), start_block(u32), reserved
const NAME_MAX: usize = 32;

#[repr(C)]
struct VolumeHeader {
    magic: u32,
    data_start_block: u32,
    next_free_block: u32,
    file_table_block: u32,
    file_count: u32,
}

#[repr(C)]
struct FileRecord {
    name: [u8; NAME_MAX],
    size: u32,
    start_block: u32,
    _reserved: u32,
}

static mut ROOT_MOUNTED: bool = false;
static mut NEXT_FREE_BLOCK: u32 = DATA_START_BLOCK;
static mut VOLUME_HEADER_DIRTY: bool = false;

fn get_block_device() -> Option<&'static dyn BlockDevice> {
    crate::drivers::nvme::get_block_device()
}

fn read_block(block: u32, buf: &mut [u8]) -> bool {
    let dev = match get_block_device() {
        Some(d) => d,
        None => return false,
    };
    let off = (block as u64) * (BLOCK_SIZE as u64);
    dev.read_blocks(off, buf)
}

fn write_block(block: u32, buf: &[u8]) -> bool {
    let dev = match get_block_device() {
        Some(d) => d,
        None => return false,
    };
    let off = (block as u64) * (BLOCK_SIZE as u64);
    dev.write_blocks(off, buf)
}

/// Mount root from block device. Reads volume header, sets ROOT_MOUNTED.
pub fn mount_root() -> bool {
    let dev = match get_block_device() {
        Some(d) => d,
        None => return false,
    };
    let mut block = [0u8; BLOCK_SIZE];
    if !dev.read_blocks(0, &mut block) {
        return false;
    }
    let vh = unsafe { &*(block.as_ptr() as *const VolumeHeader) };
    if vh.magic != MAGIC {
        let mut init = [0u8; BLOCK_SIZE];
        let vh_init = unsafe { &mut *(init.as_mut_ptr() as *mut VolumeHeader) };
        vh_init.magic = MAGIC;
        vh_init.data_start_block = DATA_START_BLOCK;
        vh_init.next_free_block = DATA_START_BLOCK;
        vh_init.file_table_block = FILE_TABLE_BLOCK;
        vh_init.file_count = MAX_FILE_RECORDS as u32;
        if !write_block(0, &init) {
            return false;
        }
        unsafe {
            NEXT_FREE_BLOCK = DATA_START_BLOCK;
            ROOT_MOUNTED = true;
        }
        return true;
    }
    unsafe {
        NEXT_FREE_BLOCK = vh.next_free_block;
        ROOT_MOUNTED = true;
    }
    true
}

pub fn root_mounted() -> bool {
    unsafe { ROOT_MOUNTED }
}

/// Flush volume header to disk if dirty.
pub fn flush_volume_header() -> bool {
    if !unsafe { VOLUME_HEADER_DIRTY } {
        return true;
    }
    let mut block = [0u8; BLOCK_SIZE];
    if !read_block(0, &mut block) {
        return false;
    }
    let vh = unsafe { &mut *(block.as_mut_ptr() as *mut VolumeHeader) };
    vh.next_free_block = unsafe { NEXT_FREE_BLOCK };
    if !write_block(0, &block) {
        return false;
    }
    unsafe { VOLUME_HEADER_DIRTY = false };
    true
}

/// Find file record index by name. Returns None if not found.
pub fn find_file_by_name(name: &[u8]) -> Option<usize> {
    if !root_mounted() {
        return None;
    }
    let mut block = [0u8; BLOCK_SIZE];
    if !read_block(FILE_TABLE_BLOCK, &mut block) {
        return None;
    }
    for i in 0..MAX_FILE_RECORDS {
        let off = i * FILE_RECORD_SIZE;
        if off + FILE_RECORD_SIZE > BLOCK_SIZE {
            break;
        }
        let rec = unsafe { &*(block[off..].as_ptr() as *const FileRecord) };
        if rec.start_block == 0 {
            continue;
        }
        let mut match_len = 0;
        for j in 0..NAME_MAX {
            if j >= name.len() || name[j] == 0 {
                break;
            }
            if rec.name[j] != name[j] {
                break;
            }
            match_len = j + 1;
        }
        if match_len == name.len() || (match_len > 0 && name.get(match_len).copied() == Some(0)) {
            let rec_name_len = (0..NAME_MAX).take_while(|&k| rec.name[k] != 0).count();
            if rec_name_len == match_len {
                return Some(i);
            }
        }
    }
    None
}

/// Allocate a new file record with the given name. Returns file record index or None.
pub fn alloc_file_record(name: &[u8]) -> Option<usize> {
    if !root_mounted() || name.len() > NAME_MAX {
        return None;
    }
    let mut block = [0u8; BLOCK_SIZE];
    if !read_block(FILE_TABLE_BLOCK, &mut block) {
        return None;
    }
    for i in 0..MAX_FILE_RECORDS {
        let off = i * FILE_RECORD_SIZE;
        if off + FILE_RECORD_SIZE > BLOCK_SIZE {
            break;
        }
        let rec = unsafe { &mut *(block[off..].as_mut_ptr() as *mut FileRecord) };
        if rec.start_block != 0 {
            continue;
        }
        rec.name[..name.len().min(NAME_MAX)].copy_from_slice(&name[..name.len().min(NAME_MAX)]);
        for j in name.len()..NAME_MAX {
            rec.name[j] = 0;
        }
        rec.size = 0;
        rec.start_block = unsafe { NEXT_FREE_BLOCK };
        rec._reserved = 0;
        unsafe {
            NEXT_FREE_BLOCK += 1;
            VOLUME_HEADER_DIRTY = true;
        }
        if !write_block(FILE_TABLE_BLOCK, &block) {
            return None;
        }
        return Some(i);
    }
    None
}

/// Get file record size and start_block by index.
pub fn get_file_record_info(file_index: usize) -> Option<(u32, u32)> {
    if !root_mounted() || file_index >= MAX_FILE_RECORDS {
        return None;
    }
    let mut block = [0u8; BLOCK_SIZE];
    if !read_block(FILE_TABLE_BLOCK, &mut block) {
        return None;
    }
    let off = file_index * FILE_RECORD_SIZE;
    let rec = unsafe { &*(block[off..].as_ptr() as *const FileRecord) };
    if rec.start_block == 0 {
        return None;
    }
    Some((rec.size, rec.start_block))
}

/// Extend file at file index by allocating num_blocks more; update file record size.
fn extend_file_blocks(file_index: usize, num_blocks: u32) -> bool {
    if !root_mounted() || file_index >= MAX_FILE_RECORDS || num_blocks == 0 {
        return false;
    }
    let mut block = [0u8; BLOCK_SIZE];
    if !read_block(FILE_TABLE_BLOCK, &mut block) {
        return false;
    }
    let rec = unsafe { &mut *(block[file_index * FILE_RECORD_SIZE..].as_mut_ptr() as *mut FileRecord) };
    if rec.start_block == 0 {
        return false;
    }
    rec.size = rec.size.saturating_add(num_blocks * BLOCK_SIZE as u32);
    unsafe {
        NEXT_FREE_BLOCK += num_blocks;
        VOLUME_HEADER_DIRTY = true;
    }
    write_block(FILE_TABLE_BLOCK, &block)
}

/// Read file data: file_index, offset, length -> copy to buf. Returns bytes read.
pub fn read_file_data(file_index: usize, offset: u32, buf: &mut [u8]) -> usize {
    let (size, start_block) = match get_file_record_info(file_index) {
        Some(x) => x,
        None => return 0,
    };
    if offset >= size {
        return 0;
    }
    let to_read = (size - offset).min(buf.len() as u32) as usize;
    if to_read == 0 {
        return 0;
    }
    let mut block_buf = [0u8; BLOCK_SIZE];
    let mut read = 0;
    let mut block_off = offset as u64 / BLOCK_SIZE as u64;
    let mut in_block_off = (offset as usize) % BLOCK_SIZE;
    let mut data_block = start_block as u64 + block_off;
    while read < to_read {
        if !read_block(data_block as u32, &mut block_buf) {
            break;
        }
        let copy = (BLOCK_SIZE - in_block_off).min(to_read - read);
        buf[read..read + copy].copy_from_slice(&block_buf[in_block_off..in_block_off + copy]);
        read += copy;
        in_block_off = 0;
        block_off += 1;
        data_block = start_block as u64 + block_off;
    }
    read
}

/// Write file data: file_index, offset, data. Extends file if needed. Returns bytes written.
pub fn write_file_data(file_index: usize, offset: u32, data: &[u8]) -> usize {
    let (size, start_block) = match get_file_record_info(file_index) {
        Some(x) => x,
        None => return 0,
    };
    if data.is_empty() {
        return 0;
    }
    let end = offset.saturating_add(data.len() as u32);
    let need_blocks = (end as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
    let have_blocks = (size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
    if need_blocks > have_blocks {
        let add_blocks = (need_blocks - have_blocks) as u32;
        if !extend_file_blocks(file_index, add_blocks) {
            return 0;
        }
    }
    let mut block_buf = [0u8; BLOCK_SIZE];
    let mut written = 0;
    let mut block_off = offset as u64 / BLOCK_SIZE as u64;
    let mut in_block_off = (offset as usize) % BLOCK_SIZE;
    let mut data_block = start_block as u32 + block_off as u32;
    while written < data.len() {
        let _ = read_block(data_block, &mut block_buf);
        let copy = (BLOCK_SIZE - in_block_off).min(data.len() - written);
        block_buf[in_block_off..in_block_off + copy].copy_from_slice(&data[written..written + copy]);
        if !write_block(data_block, &block_buf) {
            break;
        }
        written += copy;
        in_block_off = 0;
        block_off += 1;
        data_block = start_block + block_off as u32;
    }
    if written > 0 {
        let (new_size, _) = get_file_record_info(file_index).unwrap_or((0, 0));
        let mut block = [0u8; BLOCK_SIZE];
        if read_block(FILE_TABLE_BLOCK, &mut block) {
            let rec = unsafe { &mut *(block[file_index * FILE_RECORD_SIZE..].as_mut_ptr() as *mut FileRecord) };
            if end > new_size {
                rec.size = end;
                let _ = write_block(FILE_TABLE_BLOCK, &block);
            }
        }
    }
    written
}

/// Delete file record by index (mark free).
pub fn free_file_record(file_index: usize) -> bool {
    if !root_mounted() || file_index >= MAX_FILE_RECORDS {
        return false;
    }
    let mut block = [0u8; BLOCK_SIZE];
    if !read_block(FILE_TABLE_BLOCK, &mut block) {
        return false;
    }
    let rec = unsafe { &mut *(block[file_index * FILE_RECORD_SIZE..].as_mut_ptr() as *mut FileRecord) };
    rec.start_block = 0;
    rec.size = 0;
    write_block(FILE_TABLE_BLOCK, &block)
}

/// List root directory: write file names to buf, newline-separated. Returns bytes written.
pub fn list_root(buf: &mut [u8]) -> usize {
    if !root_mounted() || buf.is_empty() {
        return 0;
    }
    let mut block = [0u8; BLOCK_SIZE];
    if !read_block(FILE_TABLE_BLOCK, &mut block) {
        return 0;
    }
    let mut written = 0;
    for i in 0..MAX_FILE_RECORDS {
        let off = i * FILE_RECORD_SIZE;
        if off + FILE_RECORD_SIZE > BLOCK_SIZE {
            break;
        }
        let rec = unsafe { &*(block[off..].as_ptr() as *const FileRecord) };
        if rec.start_block == 0 {
            continue;
        }
        let name_len = (0..NAME_MAX).take_while(|&k| rec.name[k] != 0).count();
        if name_len == 0 {
            continue;
        }
        if written + name_len + 1 > buf.len() {
            break;
        }
        buf[written..written + name_len].copy_from_slice(&rec.name[..name_len]);
        buf[written + name_len] = b'\n';
        written += name_len + 1;
    }
    written
}
