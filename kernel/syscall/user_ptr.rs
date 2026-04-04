//! User pointer validation using current process CR3. All syscalls that accept
//! user pointers must validate the range before use.

use crate::process;
use crate::memory::paging::{self, PAGE_SIZE};

/// Maximum bytes we allow for a single user buffer (paths, list_root, etc.).
pub const MAX_USER_COPY: usize = 4096;

/// User half of canonical space (x86-64: below this is user).
const USER_VA_MAX: u64 = 0x0000_8000_0000_0000;

/// Syscall error type; matches libs SYSERR_*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SysErr(pub i64);

impl SysErr {
    pub const INVAL: SysErr = SysErr(libs::SYSERR_INVAL);
    pub const NOENT: SysErr = SysErr(libs::SYSERR_NOENT);
    pub const BADFD: SysErr = SysErr(libs::SYSERR_BADFD);
    pub const MFILE: SysErr = SysErr(libs::SYSERR_MFILE);
    pub const NOMEM: SysErr = SysErr(libs::SYSERR_NOMEM);
    pub const AGAIN: SysErr = SysErr(libs::SYSERR_AGAIN);
}

/// Convert Result to u64 for rax: Ok(v) => v, Err(e) => e.0 as u64 (sign-extended for isize).
#[inline]
pub fn result_to_rax<T, E>(r: Result<T, E>, ok_to_u64: impl FnOnce(T) -> u64, err_to_i64: impl FnOnce(E) -> i64) -> u64 {
    match r {
        Ok(v) => ok_to_u64(v),
        Err(e) => err_to_i64(e) as u64,
    }
}

/// Validate a large user buffer (e.g. pixel buffers) — checks pointer is in user
/// space without enforcing `MAX_USER_COPY`.  Used by surface_attach and fb_blit.
pub fn validate_user_ptr_large(ptr: u64, len: usize) -> Result<(), SysErr> {
    if len == 0 { return Ok(()); }
    if ptr == 0 || ptr >= USER_VA_MAX { return Err(SysErr::INVAL); }
    let end = ptr.saturating_add(len as u64);
    if end > USER_VA_MAX || end <= ptr { return Err(SysErr::INVAL); }
    Ok(())
}

/// Validate that [ptr, ptr+len) is in user space and mapped in the current process.
/// If need_write is true, we only validate presence (full R/W check would need walk_pte flags).
/// Returns Ok(()) if the range is safe to use.
pub fn validate_user_range(cr3: u64, ptr: u64, len: usize, _need_write: bool) -> Result<(), SysErr> {
    if len == 0 {
        return Ok(());
    }
    if ptr >= USER_VA_MAX {
        return Err(SysErr::INVAL);
    }
    let end = ptr.saturating_add(len as u64);
    if end > USER_VA_MAX || end <= ptr {
        return Err(SysErr::INVAL);
    }
    if len > MAX_USER_COPY {
        return Err(SysErr::INVAL);
    }
    let mut page_va = ptr & !(PAGE_SIZE as u64 - 1);
    let last_byte = end - 1;
    let last_page = last_byte & !(PAGE_SIZE as u64 - 1);
    while page_va <= last_page {
        if paging::translate_in_space(cr3, page_va).is_none() {
            return Err(SysErr::INVAL);
        }
        if page_va == last_page {
            break;
        }
        page_va += PAGE_SIZE as u64;
    }
    Ok(())
}

/// Validate and return current process CR3, or Err if no current process.
pub fn current_cr3() -> Result<u64, SysErr> {
    process::current_descriptor()
        .map(|d| d.cr3)
        .ok_or(SysErr::INVAL)
}
