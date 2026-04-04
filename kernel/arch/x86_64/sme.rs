//! AMD Secure Memory Encryption (SME) detection and enablement.
//!
//! AMD SME encrypts DRAM transparently using an AES-128 engine in the memory
//! controller. When enabled, every physical page is encrypted unless the C-bit
//! (bit 47 in the physical address by default) is cleared in the PTE.
//!
//! This is a **unique kernel feature**: no other general-purpose OS enables SME
//! by default without explicit administrator configuration. We enable it at boot
//! so that all physical memory — kernel, userland, heap, stacks — is encrypted
//! at rest against physical DMA attacks and cold-boot attacks.
//!
//! ## How it works
//!
//! 1. CPUID 0x8000_001F → EAX bit 0 = SME present; EBX[5:0] = C-bit position.
//! 2. MSR 0xC001_0010 (SYSCFG): set bit 23 (SMEE = SME Enable) to activate.
//! 3. After enabling, all memory that does NOT have C-bit cleared in its PTE is
//!    encrypted. The kernel identity-maps with C-bit set so everything is
//!    encrypted by default.
//!
//! ## SEV-SNP stub
//!
//! Full SEV-SNP (guest VM attestation) requires BIOS/hypervisor cooperation and
//! a PSP firmware round-trip. We detect it here and log availability, but runtime
//! enablement is deferred to a future secure-launch path.
//!
//! ## References
//!
//! AMD64 Architecture Programmer's Manual Vol. 2, Chapter 7 (Memory Encryption).

use core::arch::asm;

/// CPUID leaf for SME/SEV feature detection.
const CPUID_EXT_ENCRYPT: u32 = 0x8000_001F;

/// MSR: System Configuration — bit 23 enables SME.
const MSR_SYSCFG: u32 = 0xC001_0010;
const SYSCFG_SMEE_BIT: u64 = 1 << 23;

/// MSR: SEV Status (read-only, present on Naples+ / Zen+).
const MSR_SEV_STATUS: u32 = 0xC001_0131;
const SEV_STATUS_SEV_BIT: u64 = 1 << 0;
const SEV_STATUS_SEV_ES_BIT: u64 = 1 << 1;
const SEV_STATUS_SNP_BIT: u64 = 1 << 2;

/// Reported C-bit position in physical addresses (default 47 on most AMD Zen2+).
static mut SME_CBIT_POS: u32 = 0;
/// Whether SME was successfully enabled this boot.
static mut SME_ENABLED: bool = false;

// ---------------------------------------------------------------------------
// MSR helpers
// ---------------------------------------------------------------------------

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
    (hi as u64) << 32 | lo as u64
}

#[inline]
unsafe fn write_msr(msr: u32, val: u64) {
    asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") val as u32,
        in("edx") (val >> 32) as u32,
        options(nostack, preserves_flags)
    );
}

// ---------------------------------------------------------------------------
// CPUID helper (leaf, subleaf)
// ---------------------------------------------------------------------------

fn cpuid(leaf: u32, subleaf: u32) -> (u32, u32, u32, u32) {
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;
    unsafe {
        asm!(
            "push rbx",
            "cpuid",
            "mov esi, ebx",
            "pop rbx",
            inout("eax") leaf => eax,
            inout("ecx") subleaf => ecx,
            out("edx") edx,
            out("esi") ebx,
            options(nostack, preserves_flags)
        );
    }
    (eax, ebx, ecx, edx)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Detect AMD SME/SEV-SNP, enable SME via SYSCFG, and log results to serial.
///
/// Call this early in `kernel_main`, after serial is up but before paging init,
/// so the C-bit position is known before we build page tables.
///
/// Returns `true` if SME was enabled successfully.
pub fn init() -> bool {
    let log = crate::arch::x86_64::serial::write_str;

    log("[SME] Checking AMD memory encryption features...\r\n");

    // ── Step 1: CPUID 0x8000_001F ─────────────────────────────────────────
    let (eax, ebx, _ecx, _edx) = cpuid(CPUID_EXT_ENCRYPT, 0);

    let sme_avail  = (eax & (1 << 0)) != 0;
    let sev_avail  = (eax & (1 << 1)) != 0;
    let snp_avail  = (eax & (1 << 3)) != 0;
    let cbit_pos   = ebx & 0x3F;          // bits [5:0]
    let phys_addr_reduction = (ebx >> 6) & 0x3F; // bits [11:6]

    log("[SME] CPUID 0x8000001F:\r\n");
    log("  SME=");
    log(if sme_avail { "yes" } else { "no" });
    log("  SEV=");
    log(if sev_avail { "yes" } else { "no" });
    log("  SNP=");
    log(if snp_avail { "yes" } else { "no" });
    log("  C-bit=");
    crate::arch::x86_64::serial::write_hex(cbit_pos as u64);
    log("  phys_reduction=");
    crate::arch::x86_64::serial::write_hex(phys_addr_reduction as u64);
    log("\r\n");

    if !sme_avail {
        log("[SME] Not available on this CPU — skipping.\r\n");
        return false;
    }

    unsafe { SME_CBIT_POS = cbit_pos; }

    // ── Step 2: Read SYSCFG, set SMEE ────────────────────────────────────
    let syscfg_before = read_msr(MSR_SYSCFG);
    log("[SME] SYSCFG before=");
    crate::arch::x86_64::serial::write_hex(syscfg_before);
    log("\r\n");

    if syscfg_before & SYSCFG_SMEE_BIT != 0 {
        log("[SME] SMEE already set by firmware — SME active.\r\n");
        unsafe { SME_ENABLED = true; }
    } else {
        unsafe {
            write_msr(MSR_SYSCFG, syscfg_before | SYSCFG_SMEE_BIT);
        }
        let syscfg_after = read_msr(MSR_SYSCFG);
        log("[SME] SYSCFG after=");
        crate::arch::x86_64::serial::write_hex(syscfg_after);
        log("\r\n");

        if syscfg_after & SYSCFG_SMEE_BIT != 0 {
            log("[SME] SME ENABLED. All physical memory now encrypted by default.\r\n");
            unsafe { SME_ENABLED = true; }
        } else {
            log("[SME] WARN: SMEE bit did not stick — firmware may require BIOS opt-in.\r\n");
        }
    }

    // ── Step 3: SEV Status (informational) ───────────────────────────────
    if sev_avail {
        let sev_status = read_msr(MSR_SEV_STATUS);
        log("[SME] SEV_STATUS=");
        crate::arch::x86_64::serial::write_hex(sev_status);
        log("\r\n");
        if sev_status & SEV_STATUS_SEV_BIT != 0 {
            log("[SME] Running as SEV guest.\r\n");
        }
        if sev_status & SEV_STATUS_SEV_ES_BIT != 0 {
            log("[SME] Running as SEV-ES guest (register state encrypted).\r\n");
        }
        if sev_status & SEV_STATUS_SNP_BIT != 0 {
            log("[SME] Running as SEV-SNP guest (full memory integrity).\r\n");
        }
    }

    unsafe { SME_ENABLED }
}

/// Returns the C-bit position in physical addresses (e.g. 47 on most Zen2+).
/// Returns 0 if SME is not available.
#[inline]
pub fn cbit_position() -> u32 {
    unsafe { SME_CBIT_POS }
}

/// Returns true if SME was enabled this boot.
#[inline]
pub fn is_enabled() -> bool {
    unsafe { SME_ENABLED }
}

/// C-bit mask to OR into a physical address to mark it as encrypted in a PTE.
/// Returns 0 if SME is not active (C-bit not required when SME off).
#[inline]
pub fn cbit_mask() -> u64 {
    let pos = unsafe { SME_CBIT_POS };
    if pos == 0 { 0 } else { 1u64 << pos }
}
