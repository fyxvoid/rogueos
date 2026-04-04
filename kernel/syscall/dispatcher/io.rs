//! I/O and file syscalls: read, write, open, close, lseek, unlink, fsync, list_root.

use crate::syscall::user_ptr::{self, SysErr};
use crate::syscall::dispatcher::copy_path_from_user;

pub(super) fn sys_read(fd: u32, buf: *mut u8, count: usize) -> Result<u64, SysErr> {
    if buf.is_null() || count == 0 {
        return Ok(0);
    }
    let cr3 = user_ptr::current_cr3()?;
    user_ptr::validate_user_range(cr3, buf as u64, count, true)?;
    if fd == 0 {
        // Block until at least one byte arrives; stop at newline or buffer full.
        // This gives the shell a natural blocking read without busy-looping at
        // the prompt and without requiring cooperative yielding to other processes.
        let mut n = 0usize;
        loop {
            match crate::drivers::tty::getchar() {
                None => {
                    if n > 0 {
                        // Have partial data; return it so caller can process.
                        break;
                    }
                    // Spin (polling) until first char arrives. Tiny asm pause to
                    // reduce bus pressure.
                    unsafe { core::arch::asm!("pause", options(nomem, nostack)); }
                    continue;
                }
                Some(b) => {
                    unsafe { *buf.add(n) = b; }
                    n += 1;
                    if b == b'\n' || b == b'\r' || n >= count {
                        break;
                    }
                }
            }
        }
        return Ok(n as u64);
    }
    if fd >= 3 {
        let slice = unsafe { core::slice::from_raw_parts_mut(buf, count) };
        return Ok(crate::fs::read_file(fd, slice) as u64);
    }
    Err(SysErr::INVAL)
}

pub(super) fn sys_write(fd: u32, buf: *const u8, count: usize) -> Result<u64, SysErr> {
    if buf.is_null() || count == 0 {
        return Ok(0);
    }
    let cr3 = user_ptr::current_cr3()?;
    user_ptr::validate_user_range(cr3, buf as u64, count, false)?;
    if fd == 1 || fd == 2 {
        for i in 0..count {
            unsafe { crate::drivers::tty::putchar(*buf.add(i)) }
        }
        return Ok(count as u64);
    }
    if fd >= 3 {
        let slice = unsafe { core::slice::from_raw_parts(buf, count) };
        return Ok(crate::fs::write_file(fd, slice) as u64);
    }
    Err(SysErr::INVAL)
}

pub(super) fn sys_open(path_ptr: *const u8, path_len: usize, flags: u32) -> Result<u64, SysErr> {
    let path = copy_path_from_user(path_ptr, path_len)?;
    let owner_pid = crate::process::current_pid().map(|p| p as u32).unwrap_or(0);
    match crate::fs::open(path, flags, owner_pid) {
        Some(fd) => Ok(fd as u64),
        None => Err(SysErr::MFILE),
    }
}

pub(super) fn sys_close(fd: u32) -> Result<u64, SysErr> {
    if fd < 3 {
        return Err(SysErr::BADFD);
    }
    if crate::fs::close(fd) {
        Ok(0)
    } else {
        Err(SysErr::BADFD)
    }
}

pub(super) fn sys_lseek(fd: u32, offset: i64, whence: u32) -> Result<u64, SysErr> {
    if fd < 3 {
        return Err(SysErr::BADFD);
    }
    match crate::fs::seek(fd, offset, whence) {
        Some(off) => Ok(off as u64),
        None => Err(SysErr::INVAL),
    }
}

pub(super) fn sys_unlink(path_ptr: *const u8, path_len: usize) -> Result<u64, SysErr> {
    let path = copy_path_from_user(path_ptr, path_len)?;
    if crate::fs::unlink(path) {
        Ok(0)
    } else {
        Err(SysErr::NOENT)
    }
}

pub(super) fn sys_fsync(fd: u32) -> Result<u64, SysErr> {
    if fd < 3 {
        return Err(SysErr::BADFD);
    }
    if crate::fs::fsync(fd) {
        Ok(0)
    } else {
        Err(SysErr::INVAL)
    }
}

pub(super) fn sys_list_root(buf: *mut u8, capacity: usize) -> Result<u64, SysErr> {
    if buf.is_null() || capacity == 0 {
        return Err(SysErr::INVAL);
    }
    let cr3 = user_ptr::current_cr3()?;
    user_ptr::validate_user_range(cr3, buf as u64, capacity, true)?;
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, capacity) };
    Ok(crate::fs::list_root(slice) as u64)
}
