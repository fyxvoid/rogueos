//! x86_64: entry, GDT, IDT, serial, halt. Paging lives in crate::memory::paging.

mod port;
pub mod cpuid;
pub mod debug_regs;
pub mod gdt;
pub mod idt;
pub mod msr;
pub mod exceptions;
pub mod perf;
pub mod ps2;
pub mod serial;
pub mod sme;
pub mod syscall_entry;
pub mod tss;

#[cfg(feature = "multiboot2")]
pub mod multiboot2_parser;

use core::arch::asm;

/// Called from binary _start after stack is set. Do not call before stack is valid.

/// Rust entry: init serial first for debug, then IDT, GDT, paging, then kernel main.
#[no_mangle]
pub unsafe extern "C" fn rust_entry() -> ! {
    // Temporarily run with interrupts disabled during early bring-up to
    // avoid timer IRQs hitting an incomplete IDT, which currently causes
    // a general protection fault and triple fault in QEMU.
    asm!("cli", options(nomem, nostack));

    serial::init();
    serial::write_str("[DBG][H=A] E0 rust_entry\r\n");
    cpuid::check_hardware_contract();

    // Load IDT first so early faults go through our handlers instead of
    // triggering a silent triple fault under the firmware's default IDT.
    idt::init();
    // #region agent log
    serial::write_str("[DBG][H=B] E1 after idt\r\n");
    // #endregion

    gdt::init();
    // #region agent log
    serial::write_str("[DBG][H=C] E2 after gdt\r\n");
    // #endregion

    crate::memory::paging::init();
    // #region agent log
    serial::write_str("[DBG][H=D] E3 after paging\r\n");
    // #endregion
    serial::write_str("[KRN] paging_enabled\r\n");
    // Standalone rust_entry is not used when booting via Gatehouse; kernel_main is the real entry.
    loop {
        asm!("hlt", options(nomem, nostack));
    }
}

/// Halt CPU until next interrupt.
#[inline]
pub fn halt() {
    unsafe { asm!("hlt", options(nomem, nostack)) };
}

/// Reboot the machine via chipset reset (QEMU supports 0xCF9).
pub fn reboot() -> ! {
    unsafe {
        // 0x02 = system reset, 0x04 = full reset. 0x06 = both.
        port::outb(0xCF9, 0x06);
    }
    loop {
        halt();
    }
}
