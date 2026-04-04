//! Page fault handler (vector 14). Decodes error code; logs CR2, RIP, flags, process ID.

use core::arch::asm;

use crate::memory::paging::mapper;
use crate::memory::paging::tlb;
use crate::memory::paging::flags::PageFlag;

/// Page fault error code bits (x86-64).
pub const PF_P: u64 = 1 << 0; // 1 = protection violation, 0 = not present
pub const PF_W: u64 = 1 << 1; // 1 = write
pub const PF_U: u64 = 1 << 2; // 1 = user mode
pub const PF_R: u64 = 1 << 3; // 1 = instruction fetch (reserved)
pub const PF_I: u64 = 1 << 4; // 1 = instruction fetch (PKRU)

/// Callback to obtain current process ID for logging. Set by process module; memory does not depend on process.
pub type CurrentPidFn = fn() -> u64;
static mut CURRENT_PID_FN: Option<CurrentPidFn> = None;

/// Register the function to read current process ID. Call from process init.
pub fn set_page_fault_pid_fn(f: CurrentPidFn) {
    unsafe {
        CURRENT_PID_FN = Some(f);
    }
}

fn read_cr2() -> u64 {
    let cr2: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) cr2, options(nostack, preserves_flags));
    }
    cr2
}

fn decode_err(err: u64) {
    crate::arch::x86_64::serial::write_str(" P=");
    crate::arch::x86_64::serial::write_str(if (err & PF_P) != 0 { "1" } else { "0" });
    crate::arch::x86_64::serial::write_str(" W=");
    crate::arch::x86_64::serial::write_str(if (err & PF_W) != 0 { "1" } else { "0" });
    crate::arch::x86_64::serial::write_str(" U=");
    crate::arch::x86_64::serial::write_str(if (err & PF_U) != 0 { "1" } else { "0" });
    crate::arch::x86_64::serial::write_str(" R=");
    crate::arch::x86_64::serial::write_str(if (err & PF_R) != 0 { "1" } else { "0" });
    crate::arch::x86_64::serial::write_str(" I=");
    crate::arch::x86_64::serial::write_str(if (err & PF_I) != 0 { "1" } else { "0" });
}

fn dump_pte_for_va(cr3: u64, va: u64) {
    crate::arch::x86_64::serial::write_str(" va=");
    crate::arch::x86_64::serial::write_hex(va);
    match mapper::walk_pte(cr3, va) {
        Some(pte) => {
            let pa = pte & 0x000F_FFFF_FFFF_F000;
            crate::arch::x86_64::serial::write_str(" pte_pa=");
            crate::arch::x86_64::serial::write_hex(pa);
            crate::arch::x86_64::serial::write_str(" fl=");
            crate::arch::x86_64::serial::write_hex(pte & 0xFFF);
            crate::arch::x86_64::serial::write_str(if (pte & PageFlag::Present as u64) != 0 { " P" } else { "" });
            crate::arch::x86_64::serial::write_str(if (pte & PageFlag::Writable as u64) != 0 { " W" } else { "" });
            crate::arch::x86_64::serial::write_str(if (pte & PageFlag::User as u64) != 0 { " U" } else { "" });
            crate::arch::x86_64::serial::write_str(if (pte & PageFlag::NoExec as u64) != 0 { " NX" } else { "" });
        }
        None => {
            crate::arch::x86_64::serial::write_str(" not_mapped");
        }
    }
    crate::arch::x86_64::serial::write_str("\r\n");
}

const NULL_PAGE_THRESHOLD: u64 = 4096;

/// Page fault handler. Called from asm stub with error_code, rip, cs.
#[no_mangle]
pub extern "C" fn page_fault_handler(error_code: u64, rip: u64, cs: u64) -> ! {
    let fault_addr = read_cr2();
    let cr3 = tlb::read_cr3();
    let cpl = cs & 3;
    let is_user = (error_code & PF_U) != 0;
    let is_null = fault_addr < NULL_PAGE_THRESHOLD;
    let not_present = (error_code & PF_P) == 0;

    let pid = unsafe { CURRENT_PID_FN.map(|f| f()).unwrap_or(0) };

    crate::arch::x86_64::serial::write_str("[PF] CPL=");
    crate::arch::x86_64::serial::write_hex(cpl);
    crate::arch::x86_64::serial::write_str(" CS=");
    crate::arch::x86_64::serial::write_hex(cs);
    crate::arch::x86_64::serial::write_str(" RIP=");
    crate::arch::x86_64::serial::write_hex(rip);
    crate::arch::x86_64::serial::write_str(" CR2=");
    crate::arch::x86_64::serial::write_hex(fault_addr);
    crate::arch::x86_64::serial::write_str(" err=");
    crate::arch::x86_64::serial::write_hex(error_code);
    crate::arch::x86_64::serial::write_str(" ");
    crate::arch::x86_64::serial::write_str(if is_user { "user " } else { "kernel " });
    if is_null {
        crate::arch::x86_64::serial::write_str("null_access ");
    }
    if not_present {
        crate::arch::x86_64::serial::write_str("not_present ");
    }
    decode_err(error_code);
    crate::arch::x86_64::serial::write_str(" cr3=");
    crate::arch::x86_64::serial::write_hex(cr3);
    crate::arch::x86_64::serial::write_str(" pid=");
    crate::arch::x86_64::serial::write_hex(pid);
    crate::arch::x86_64::serial::write_str("\r\n");

    if is_user && (is_null || not_present) {
        crate::arch::x86_64::serial::write_str("[PF] killing process (user null/invalid)\r\n");
        crate::process::exit_current_and_schedule(None);
    }

    crate::arch::x86_64::serial::write_str("[PF] nearby PTEs:\r\n");
    let page = 4096u64;
    dump_pte_for_va(cr3, fault_addr);
    if fault_addr >= page {
        dump_pte_for_va(cr3, fault_addr - page);
    }
    dump_pte_for_va(cr3, fault_addr + page);
    if fault_addr >= 2 * page {
        dump_pte_for_va(cr3, fault_addr - 2 * page);
    }
    dump_pte_for_va(cr3, fault_addr + 2 * page);

    crate::drivers::framebuffer::clear(0xFF_80_40_00);
    crate::drivers::framebuffer::flush();
    crate::kernel::diagnostic::diagnostic_halt("page_fault_unhandled")
}

/// IDT entry for vector 14. CPU pushed [error_code][rip][cs][rflags][rsp][ss].
#[no_mangle]
pub unsafe extern "C" fn page_fault_stub() {
    core::arch::asm!(
        r#"
        mov rdi, [rsp]
        mov rsi, [rsp + 8]
        mov rdx, [rsp + 16]
        call {}
        "#,
        sym page_fault_handler,
        options(nostack, noreturn)
    );
}
