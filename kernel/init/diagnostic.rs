//! Unified diagnostic dump + halt path used by panic/fault/invariant checks.

use core::arch::asm;

fn read_rbp() -> u64 {
    let rbp: u64;
    unsafe { asm!("mov {}, rbp", out(reg) rbp, options(nomem, nostack, preserves_flags)) };
    rbp
}

fn is_canonical(va: u64) -> bool {
    // x86_64 canonical: bits 63..48 are sign-extension of bit 47.
    let top = va >> 48;
    top == 0 || top == 0xFFFF
}

pub fn dump_stack_trace_serial(max_frames: usize) {
    crate::arch::x86_64::serial::write_str("[DIAG] stack trace (rbp walk)\r\n");
    let mut rbp = read_rbp();
    for i in 0..max_frames {
        if rbp == 0 || (rbp & 0x7) != 0 || !is_canonical(rbp) {
            break;
        }
        unsafe {
            let rbp_ptr = rbp as *const u64;
            // [0] = previous rbp, [1] = return address
            let next_rbp = core::ptr::read_volatile(rbp_ptr);
            let ret = core::ptr::read_volatile(rbp_ptr.add(1));
            crate::arch::x86_64::serial::write_fmt(format_args!("[DIAG]  #{}", i));
            crate::arch::x86_64::serial::write_str(" rbp=");
            crate::arch::x86_64::serial::write_hex(rbp);
            crate::arch::x86_64::serial::write_str(" ret=");
            crate::arch::x86_64::serial::write_hex(ret);
            crate::arch::x86_64::serial::write_str("\r\n");
            if next_rbp <= rbp {
                break;
            }
            rbp = next_rbp;
        }
    }
}

pub fn dump_allocator_state_serial() {
    crate::memory::debug::dump_all_serial();
}

pub fn dump_scheduler_state_serial() {
    crate::arch::x86_64::serial::write_str("[DIAG] scheduler state\r\n");
    crate::process::dump_state_serial();
}

pub fn dump_paging_state_serial() {
    crate::arch::x86_64::serial::write_str("[DIAG] paging state\r\n");
    let cr3 = crate::memory::paging::read_cr3();
    crate::arch::x86_64::serial::write_str("[DIAG] cr3=");
    crate::arch::x86_64::serial::write_hex(cr3);
    crate::arch::x86_64::serial::write_str("\r\n");
    crate::memory::paging::dump_ptes_range_serial(cr3, 0, 0x20_000);
    crate::memory::paging::dump_ptes_range_serial(cr3, 0x100_000, 0x120_000);
}

pub fn diagnostic_halt(reason: &'static str) -> ! {
    crate::arch::x86_64::serial::write_str("\r\n[DIAG] HALT reason: ");
    crate::arch::x86_64::serial::write_str(reason);
    crate::arch::x86_64::serial::write_str("\r\n");

    dump_stack_trace_serial(16);
    dump_paging_state_serial();
    dump_scheduler_state_serial();
    dump_allocator_state_serial();
    crate::drivers::framebuffer::dump_state_serial();

    crate::arch::x86_64::serial::write_str("[DIAG] system halted.\r\n");
    loop {
        crate::arch::halt();
    }
}

