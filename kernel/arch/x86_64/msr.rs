//! Model-specific registers for SYSCALL/SYSRET.

use core::arch::asm;

/// IA32_EFER: Extended Feature Enable. Bit 0 = SCE (SYSCALL enable).
const IA32_EFER: u32 = 0xC000_0080;
/// IA32_LSTAR: Long Syscall Target Address (RIP on SYSCALL).
const IA32_LSTAR: u32 = 0xC000_0082;
/// IA32_FMASK: RFLAGS mask (bits set here are cleared in RFLAGS on SYSCALL). We clear IF (bit 9).
const IA32_FMASK: u32 = 0xC000_0084;

const EFER_SCE: u64 = 1 << 0;
const RFLAGS_IF: u64 = 1 << 9;

#[inline]
fn read_msr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(nostack, preserves_flags)
        );
    }
    (hi as u64) << 32 | (lo as u64)
}

#[inline]
unsafe fn write_msr(msr: u32, value: u64) {
    let lo = value as u32;
    let hi = (value >> 32) as u32;
    asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") lo,
        in("edx") hi,
        options(nostack, preserves_flags)
    );
}

/// Enable SYSCALL and set LSTAR to the given entry address (physical/virtual; kernel identity-mapped).
/// FMASK clears IF so interrupts are disabled on syscall entry.
pub fn init_syscall_msrs(syscall_entry_addr: u64) {
    unsafe {
        let efer = read_msr(IA32_EFER);
        write_msr(IA32_EFER, efer | EFER_SCE);
        write_msr(IA32_LSTAR, syscall_entry_addr);
        write_msr(IA32_FMASK, RFLAGS_IF);
    }
}
