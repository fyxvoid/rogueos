//! CPUID feature detection and CPU security hardening.
//! Detects SMEP, SMAP, UMIP (CPUID leaf 0x07) and enables them in CR4.
//! Also enables CR0.WP so the kernel respects read-only page mappings.

use core::arch::asm;

/// CPUID vendor string "AuthenticAMD" (12 bytes) in EBX, EDX, ECX order.
const AMD_VENDOR: [u8; 12] = [b'A', b'u', b't', b'h', b'e', b'n', b't', b'i', b'c', b'A', b'M', b'D'];

// CR4 security bits.
const CR4_SMEP: u64 = 1 << 20; // Supervisor-Mode Execution Prevention
const CR4_UMIP: u64 = 1 << 11; // User-Mode Instruction Prevention

// CR0 security bit.
const CR0_WP: u64 = 1 << 16;   // Write-Protect: kernel respects read-only pages

// CPUID leaf 0x07 subleaf 0x00 EBX feature bits.
const CPUID_07_EBX_SMEP: u32 = 1 << 7;
const CPUID_07_EBX_SMAP: u32 = 1 << 20;

// CPUID leaf 0x07 subleaf 0x00 ECX feature bits.
const CPUID_07_ECX_UMIP: u32 = 1 << 2;

/// Verify CPU is AMD (Zen 2+ assumed). Call early after serial init.
/// Halts if vendor is not AMD so we fail fast on wrong hardware.
pub fn check_hardware_contract() {
    let (ebx, ecx, edx) = cpuid_vendor();
    let mut buf = [0u8; 12];
    buf[0..4].copy_from_slice(&ebx.to_le_bytes());
    buf[4..8].copy_from_slice(&edx.to_le_bytes());
    buf[8..12].copy_from_slice(&ecx.to_le_bytes());
    if buf != AMD_VENDOR {
        crate::arch::x86_64::serial::write_str("[HW] Not AMD CPU; halt. See HARDWARE.md.\r\n");
        loop {
            crate::arch::halt();
        }
    }
}

/// Enable CPU security features detected via CPUID leaf 0x07.
pub fn init_cpu_security() {
    let (ebx_07, ecx_07) = cpuid_leaf07();
    let has_smep = (ebx_07 & CPUID_07_EBX_SMEP) != 0;
    let has_smap = (ebx_07 & CPUID_07_EBX_SMAP) != 0;
    let has_umip = (ecx_07 & CPUID_07_ECX_UMIP) != 0;

    crate::arch::x86_64::serial::write_str("[CPU] CPUID 0x07: SMEP=");
    crate::arch::x86_64::serial::write_str(if has_smep { "1" } else { "0" });
    crate::arch::x86_64::serial::write_str(" SMAP=");
    crate::arch::x86_64::serial::write_str(if has_smap { "1" } else { "0" });
    crate::arch::x86_64::serial::write_str(" UMIP=");
    crate::arch::x86_64::serial::write_str(if has_umip { "1" } else { "0" });
    crate::arch::x86_64::serial::write_str("\r\n");

    unsafe {
        let cr0: u64;
        asm!("mov {}, cr0", out(reg) cr0, options(nostack, preserves_flags));
        asm!("mov cr0, {}", in(reg) cr0 | CR0_WP, options(nostack, preserves_flags));

        let cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4, options(nostack, preserves_flags));
        let mut new_cr4 = cr4;
        if has_smep { new_cr4 |= CR4_SMEP; }
        if has_umip { new_cr4 |= CR4_UMIP; }
        if new_cr4 != cr4 {
            asm!("mov cr4, {}", in(reg) new_cr4, options(nostack, preserves_flags));
        }
    }

    crate::arch::x86_64::serial::write_str("[CPU] CR0.WP=1");
    if has_smep { crate::arch::x86_64::serial::write_str(" CR4.SMEP=1"); }
    if has_umip { crate::arch::x86_64::serial::write_str(" CR4.UMIP=1"); }
    if has_smap { crate::arch::x86_64::serial::write_str(" (SMAP detected but not enabled)"); }
    crate::arch::x86_64::serial::write_str("\r\n");
}

fn cpuid_vendor() -> (u32, u32, u32) {
    let mut ebx: u32 = 0;
    let ecx: u32;
    let edx: u32;
    unsafe {
        asm!(
            "cpuid",
            "mov [{}], ebx",
            in(reg) &mut ebx,
            in("eax") 0u32,
            out("ecx") ecx,
            out("edx") edx,
            options(nostack, preserves_flags)
        );
    }
    (ebx, ecx, edx)
}

/// CPUID leaf 0x07 subleaf 0x00: returns (EBX, ECX) with extended feature flags.
fn cpuid_leaf07() -> (u32, u32) {
    let ebx: u32;
    let ecx: u32;
    unsafe {
        asm!(
            "push rbx",
            "xor ecx, ecx",
            "cpuid",
            "mov {0:e}, ebx",
            "pop rbx",
            out(reg) ebx,
            in("eax") 7u32,
            out("ecx") ecx,
            options(nostack, preserves_flags)
        );
    }
    (ebx, ecx)
}
