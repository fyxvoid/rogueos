//! General protection, double-fault, and simple IRQ stubs.
//!
//! These are intentionally minimal: they log what happened over serial and
//! then halt the CPU, so we avoid silent triple-fault resets in QEMU while
//! bringing the rest of the kernel up.

use core::arch::asm;

/// Generic fault logger: used by GP and double-fault handlers.
fn log_fault(name: &str, vec: u8) {
    crate::arch::serial::write_str("[FAULT] ");
    crate::arch::serial::write_str(name);
    crate::arch::serial::write_str(" vec=");
    crate::arch::serial::write_hex(vec as u64);
    crate::arch::serial::write_str("\r\n");
}

/// Ring transition trace: CPL = CS & 3, RIP, error code. Called from gp_fault_stub with stacked values.
#[no_mangle]
pub extern "C" fn gp_fault_handler(error_code: u64, rip: u64, cs: u64) -> ! {
    let cpl = cs & 3;
    crate::arch::serial::write_str("[FAULT] GP vec=13 CPL=");
    crate::arch::serial::write_hex(cpl);
    crate::arch::serial::write_str(" CS=");
    crate::arch::serial::write_hex(cs);
    crate::arch::serial::write_str(" RIP=");
    crate::arch::serial::write_hex(rip);
    crate::arch::serial::write_str(" err=");
    crate::arch::serial::write_hex(error_code);
    crate::arch::serial::write_str("\r\n");
    crate::drivers::framebuffer::clear(0xFF_00_00_80);
    crate::drivers::framebuffer::flush();
    loop {
        crate::arch::halt();
    }
}

#[no_mangle]
pub extern "C" fn double_fault_handler() -> ! {
    log_fault("DF", 8);
    crate::drivers::framebuffer::clear(0xFF_80_00_80);
    crate::drivers::framebuffer::flush();
    loop {
        crate::arch::halt();
    }
}

#[no_mangle]
pub extern "C" fn ud_fault_handler() -> ! {
    log_fault("UD", 6);
    crate::drivers::framebuffer::clear(0xFF_80_40_40);
    crate::drivers::framebuffer::flush();
    loop {
        crate::arch::halt();
    }
}

/// IDT entry for #GP (vector 13). CPU has pushed [error_code][rip][cs][rflags][rsp][ss].
#[no_mangle]
pub unsafe extern "C" fn gp_fault_stub() {
    asm!(
        r#"
        mov rdi, [rsp]
        mov rsi, [rsp + 8]
        mov rdx, [rsp + 16]
        call {handler}
        "#,
        handler = sym gp_fault_handler,
        options(noreturn)
    );
}

/// IDT entry for #DF (vector 8). Same pushed layout as #GP but error_code is always 0.
#[no_mangle]
pub unsafe extern "C" fn double_fault_stub() {
    asm!(
        r#"
        call {handler}
        "#,
        handler = sym double_fault_handler,
        options(noreturn)
    );
}

/// IDT entry for #UD (vector 6). No error code is pushed.
#[no_mangle]
pub unsafe extern "C" fn ud_fault_stub() {
    asm!(
        r#"
        call {handler}
        "#,
        handler = sym ud_fault_handler,
        options(noreturn)
    );
}

/// Very simple IRQ0 handler: just log that it fired and return.
#[no_mangle]
pub extern "C" fn irq0_handler() {
    crate::arch::serial::write_str("[INT] irq0\r\n");
}

/// IDT entry for IRQ0 (vector 0x20). No error code is pushed.
#[no_mangle]
pub unsafe extern "C" fn irq0_stub() {
    asm!(
        r#"
        push rbp
        mov rbp, rsp
        call {handler}
        pop rbp
        iretq
        "#,
        handler = sym irq0_handler,
        options(noreturn)
    );
}

