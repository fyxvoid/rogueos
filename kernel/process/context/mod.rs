//! Context switch: save/restore user state, enter user mode.

use core::arch::asm;

use crate::process::process::TrapFrame;
use crate::arch::x86_64::{gdt, serial, tss};

/// Switch to user: set TSS RSP0, load CR3, set kernel stack, push iretq frame (ss, rsp, rflags, cs, rip), iretq.
#[no_mangle]
pub unsafe extern "C" fn enter_user(frame: *const TrapFrame, cr3: u64, kernel_stack_top: u64) {
    tss::set_kernel_rsp(kernel_stack_top);
    crate::arch::x86_64::syscall_entry::set_current_kernel_rsp(kernel_stack_top);
    // Debug before iretq: confirm USER_CS/SS (with RPL=3), TSS.RSP0, frame->rip, frame->rsp.
    let f = &*frame;
    serial::write_str("[enter_user] USER_CS=");
    serial::write_hex((gdt::USER_CS | 3) as u64);
    serial::write_str(" USER_SS=");
    serial::write_hex((gdt::USER_SS | 3) as u64);
    serial::write_str(" TSS.RSP0=");
    serial::write_hex(kernel_stack_top);
    serial::write_str(" frame.rip=");
    serial::write_hex(f.rip);
    serial::write_str(" frame.rsp=");
    serial::write_hex(f.rsp);
    serial::write_str(" frame.cs=");
    serial::write_hex(f.cs);
    serial::write_str(" frame.ss=");
    serial::write_hex(f.ss);
    serial::write_str("\r\n");
    asm!(
        "mov cr3, {}",
        "mov rsp, {}",
        "push qword ptr [{} + 32]",
        "push qword ptr [{} + 24]",
        "push qword ptr [{} + 16]",
        "push qword ptr [{} + 8]",
        "push qword ptr [{} + 0]",
        "iretq",
        in(reg) cr3,
        in(reg) kernel_stack_top,
        in(reg) frame,
        in(reg) frame,
        in(reg) frame,
        in(reg) frame,
        in(reg) frame,
        options(nostack, noreturn)
    );
}
