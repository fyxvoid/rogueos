//! Cogman restart journal — 4 KiB static region.
//!
//! Cogman writes its serialised supervisor state here before every
//! service-table mutation. If Cogman crashes or is restarted by the kernel,
//! the new instance calls `SYS_JOURNAL_READ` to restore state and resume
//! supervising all services within 5 ms.
//!
//! The journal is plain bytes: the userland cogman binary defines the
//! schema. The kernel only stores and retrieves it.
//!
//! Concurrency: single-core, no locks needed. The write is a memcpy into a
//! static buffer followed by a length update.

pub const JOURNAL_SIZE: usize = 4096;

/// The journal data.
static mut JOURNAL_DATA: [u8; JOURNAL_SIZE] = [0; JOURNAL_SIZE];
/// Byte count of the last valid write (0 = journal empty).
static mut JOURNAL_LEN: usize = 0;
/// PID of the process that last wrote to the journal.
static mut JOURNAL_WRITER_PID: u32 = 0;

/// Overwrite the journal with `data`. Returns the number of bytes stored,
/// or 0 if `data` exceeds `JOURNAL_SIZE`.
pub fn write(data: &[u8], writer_pid: u32) -> usize {
    if data.len() > JOURNAL_SIZE {
        crate::arch::serial::write_str("[JOURNAL] write too large\r\n");
        return 0;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), JOURNAL_DATA.as_mut_ptr(), data.len());
        // Zero the remainder so stale bytes never resurface.
        if data.len() < JOURNAL_SIZE {
            core::ptr::write_bytes(
                JOURNAL_DATA.as_mut_ptr().add(data.len()),
                0,
                JOURNAL_SIZE - data.len(),
            );
        }
        JOURNAL_LEN = data.len();
        JOURNAL_WRITER_PID = writer_pid;
    }
    data.len()
}

/// Copy the journal into `buf`. Returns bytes copied (0 if journal is empty).
pub fn read(buf: &mut [u8]) -> usize {
    unsafe {
        let n = core::cmp::min(buf.len(), JOURNAL_LEN);
        if n > 0 {
            core::ptr::copy_nonoverlapping(JOURNAL_DATA.as_ptr(), buf.as_mut_ptr(), n);
        }
        n
    }
}

/// Length of the last committed journal write (0 = nothing written yet).
pub fn len() -> usize {
    unsafe { JOURNAL_LEN }
}
