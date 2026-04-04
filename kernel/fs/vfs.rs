//! VFS: fd table, open/close/read/write/seek/fsync/unlink. Fd 0,1,2 = TTY.

use crate::fs::simple_fs;

const MAX_OPEN: usize = 8;
const FIRST_FD: u32 = 3;

pub struct OpenFlags;
impl OpenFlags {
    pub const O_RDONLY: u32 = 0;
    pub const O_WRONLY: u32 = 1;
    pub const O_RDWR: u32 = 2;
    pub const O_CREAT: u32 = 0x40;
    pub const O_TRUNC: u32 = 0x200;
}

#[derive(Clone, Copy)]
struct FdEntry {
    file_index: Option<usize>,
    offset: u32,
    /// Process that opened this fd; used to close all fds on exit (no leak).
    owner_pid: Option<u32>,
}

static mut FD_TABLE: [FdEntry; MAX_OPEN] = [FdEntry {
    file_index: None,
    offset: 0,
    owner_pid: None,
}; MAX_OPEN];

fn fd_to_slot(fd: u32) -> Option<usize> {
    if fd < FIRST_FD || fd >= FIRST_FD + MAX_OPEN as u32 {
        return None;
    }
    Some((fd - FIRST_FD) as usize)
}

/// Open a file by path (name only; no slashes). Returns fd or None.
/// owner_pid: process that owns this fd (for close on exit); use 0 if caller has no pid.
pub fn open(path: &[u8], flags: u32, owner_pid: u32) -> Option<u32> {
    if !simple_fs::root_mounted() {
        return None;
    }
    // SAFETY: Single-threaded kernel; FD_TABLE is only touched by this code path and close paths.
    let slot = (0..MAX_OPEN).find(|&i| unsafe { FD_TABLE[i].file_index.is_none() })?;
    let file_index = if path.iter().all(|&b| b != b'/') {
        simple_fs::find_file_by_name(path)
            .or_else(|| {
                if (flags & OpenFlags::O_CREAT) != 0 {
                    simple_fs::alloc_file_record(path)
                } else {
                    None
                }
            })?
    } else {
        return None;
    };
    // SAFETY: slot is in 0..MAX_OPEN; exclusive write to this slot.
    unsafe {
        FD_TABLE[slot].file_index = Some(file_index);
        FD_TABLE[slot].offset = 0;
        FD_TABLE[slot].owner_pid = if owner_pid != 0 { Some(owner_pid) } else { None };
    }
    Some(FIRST_FD + slot as u32)
}

/// Close fd. Returns true if was open.
pub fn close(fd: u32) -> bool {
    let slot = match fd_to_slot(fd) {
        Some(s) => s,
        None => return false,
    };
    // SAFETY: slot validated; exclusive clear of this slot.
    unsafe {
        let was = FD_TABLE[slot].file_index.is_some();
        FD_TABLE[slot].file_index = None;
        FD_TABLE[slot].owner_pid = None;
        was
    }
}

/// Close all fds owned by the given process. Call on process exit to prevent fd leak.
pub fn close_fds_for_process(pid: u32) {
    if pid == 0 {
        return;
    }
    // SAFETY: Single-threaded exit path; we only clear entries for this pid.
    unsafe {
        for i in 0..MAX_OPEN {
            if FD_TABLE[i].owner_pid == Some(pid) {
                FD_TABLE[i].file_index = None;
                FD_TABLE[i].owner_pid = None;
            }
        }
    }
}

/// Read from file fd into buf. Returns bytes read.
pub fn read_file(fd: u32, buf: &mut [u8]) -> usize {
    let slot = match fd_to_slot(fd) {
        Some(s) => s,
        None => return 0,
    };
    // SAFETY: slot in 0..MAX_OPEN; read-only borrow of one slot.
    let (file_index, offset) = unsafe {
        let e = &FD_TABLE[slot];
        match e.file_index {
            Some(idx) => (idx, e.offset),
            None => return 0,
        }
    };
    let n = simple_fs::read_file_data(file_index, offset, buf);
    // SAFETY: slot valid; single write to offset.
    unsafe {
        FD_TABLE[slot].offset = offset + n as u32;
    }
    n
}

/// Write to file fd from data. Returns bytes written.
pub fn write_file(fd: u32, data: &[u8]) -> usize {
    let slot = match fd_to_slot(fd) {
        Some(s) => s,
        None => return 0,
    };
    // SAFETY: slot in 0..MAX_OPEN; read-only borrow of one slot.
    let (file_index, offset) = unsafe {
        let e = &FD_TABLE[slot];
        match e.file_index {
            Some(idx) => (idx, e.offset),
            None => return 0,
        }
    };
    let n = simple_fs::write_file_data(file_index, offset, data);
    // SAFETY: slot valid; single write to offset.
    unsafe {
        FD_TABLE[slot].offset = offset + n as u32;
    }
    n
}

/// Seek: 0=SET, 1=CUR, 2=END. Returns new offset or None.
pub fn seek(fd: u32, offset: i64, whence: u32) -> Option<u32> {
    let slot = fd_to_slot(fd)?;
    // SAFETY: slot in 0..MAX_OPEN; read-only.
    let (file_index, cur) = unsafe {
        let e = &FD_TABLE[slot];
        let idx = e.file_index?;
        (idx, e.offset)
    };
    let (size, _) = simple_fs::get_file_record_info(file_index)?;
    let new_off: u32 = match whence {
        0 => offset.max(0) as u32,
        1 => (cur as i64 + offset).max(0) as u32,
        2 => (size as i64 + offset).max(0) as u32,
        _ => return None,
    };
    // SAFETY: slot valid; single write.
    unsafe {
        FD_TABLE[slot].offset = new_off;
    }
    Some(new_off)
}

/// Flush volume header to disk (and any file buffers). Call after writes for persistence.
pub fn fsync(fd: u32) -> bool {
    if fd_to_slot(fd).is_none() {
        return false;
    }
    simple_fs::flush_volume_header()
}

/// List root directory names into buf (newline-separated). Returns bytes written.
pub fn list_root(buf: &mut [u8]) -> usize {
    simple_fs::list_root(buf)
}

/// Unlink (delete) file by path. Returns true if removed.
pub fn unlink(path: &[u8]) -> bool {
    if !simple_fs::root_mounted() {
        return false;
    }
    let file_index = match simple_fs::find_file_by_name(path) {
        Some(idx) => idx,
        None => return false,
    };
    simple_fs::free_file_record(file_index)
}
