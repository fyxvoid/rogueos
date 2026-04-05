//! Model-specific registers for SYSCALL/SYSRET.

use core::arch::asm;

/// IA32_EFER: Extended Feature Enable. Bit 0 = SCE (SYSCALL enable).
const IA32_EFER: u32 = 0xC000_0080;
/// IA32_STAR: Segment selectors for SYSCALL/SYSRET.
///   Bits 47:32 = CS for SYSCALL (kernel CS); SS = STAR[47:32]+8.
///   Bits 63:48 = base for SYSRET; CS = base+16 (with RPL=3), SS = base+8 (with RPL=3).
///   We set [47:32]=0x08 (KERNEL_CS) and [63:48]=0x10 (KERNEL_DS) so that:
///     SYSCALL → CS=0x08, SS=0x10 (kernel code+data)
///     SYSRET  → CS=0x20 (USER_CS), SS=0x18 (USER_SS)
const IA32_STAR: u32 = 0xC000_0081;
/// IA32_LSTAR: Long Syscall Target Address (RIP on SYSCALL).
const IA32_LSTAR: u32 = 0xC000_0082;
/// IA32_FMASK: RFLAGS mask (bits set here are cleared in RFLAGS on SYSCALL). We clear IF (bit 9).
const IA32_FMASK: u32 = 0xC000_0084;

const EFER_SCE: u64 = 1 << 0;
/// EFER.NXE (bit 11): enable the No-Execute bit (bit 63) in page table entries.
const EFER_NXE: u64 = 1 << 11;
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
/// STAR: [47:32]=KERNEL_CS(0x08) for SYSCALL; [63:48]=KERNEL_DS(0x10) for SYSRET.
pub fn init_syscall_msrs(syscall_entry_addr: u64) {
    // STAR layout: bits 47:32 = KERNEL_CS, bits 63:48 = SYSRET base (KERNEL_DS = 0x10).
    // SYSRET: CS = (0x10+16)|3 = 0x23 → but wait: SYSRET sets RPL bits directly, so
    // CS.Selector = (0x10+16) OR 3 = 0x23, which is selector 0x20 | RPL=3 = USER_CS|3. ✓
    // SS.Selector = (0x10+8) OR 3 = 0x1b, which is selector 0x18 | RPL=3 = USER_SS|3. ✓
    const STAR_KERNEL_CS_SHIFT: u64 = 32;
    const STAR_SYSRET_BASE_SHIFT: u64 = 48;
    let star = (0x0008u64 << STAR_KERNEL_CS_SHIFT) | (0x0010u64 << STAR_SYSRET_BASE_SHIFT);
    unsafe {
        let efer = read_msr(IA32_EFER);
        write_msr(IA32_EFER, efer | EFER_SCE | EFER_NXE);
        write_msr(IA32_STAR, star);
        write_msr(IA32_LSTAR, syscall_entry_addr);
        write_msr(IA32_FMASK, RFLAGS_IF);
    }
}
