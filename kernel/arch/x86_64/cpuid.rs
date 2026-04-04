//! Minimal CPUID for hardware contract check. UEFI implies long mode; we verify AMD.

use core::arch::asm;

/// CPUID vendor string "AuthenticAMD" (12 bytes) in EBX, EDX, ECX order.
const AMD_VENDOR: [u8; 12] = [b'A', b'u', b't', b'h', b'e', b'n', b't', b'i', b'c', b'A', b'M', b'D'];

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
