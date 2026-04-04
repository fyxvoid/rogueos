//! Memory subsystem stats: frame counts, heap usage, leak detection.

use crate::memory::physical::buddy_dump_state_serial;
use crate::memory::heap::allocator::dump_state_serial as heap_dump_state_serial;
use crate::memory::paging;

/// Dump physical (buddy), heap, and current CR3 to serial.
pub fn dump_all_serial() {
    crate::arch::x86_64::serial::write_str("[DIAG] memory stats\r\n");
    let cr3 = paging::read_cr3();
    crate::arch::x86_64::serial::write_str("[DIAG] cr3=");
    crate::arch::x86_64::serial::write_hex(cr3);
    crate::arch::x86_64::serial::write_str("\r\n");
    heap_dump_state_serial();
    buddy_dump_state_serial();
}
