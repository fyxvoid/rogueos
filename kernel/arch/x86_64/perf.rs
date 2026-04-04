//! AMD Performance Monitoring Unit (PMU) — perf-counter telemetry.
//!
//! Exposes hardware performance counters to userland via syscall so AI/ML
//! scheduling models, profilers, and pentester tools can read live CPU metrics
//! without ring-0 access.
//!
//! ## AMD PMU architecture (Family 17h / 19h — Zen 2/3/4)
//!
//! - 6 general-purpose performance counters per core.
//! - Each counter: pair of MSRs:
//!     PERF_CTL[n]  = 0xC001_0200 + n*2   (event select, unit mask, enable)
//!     PERF_CTR[n]  = 0xC001_0201 + n*2   (48-bit count, wraps on overflow)
//! - Legacy pair also at 0xC001_0000 + n (CTL) / 0xC001_0004 + n (CTR).
//! - RDPMC(n) can read PERF_CTR[n] from ring-3 if CR4.PCE is set (we set it).
//!
//! ## Event codes (AMD Fam17h/19h — independently referenced from AMD PPR)
//!
//! | Name           | EventSelect | UnitMask | Description                      |
//! |----------------|-------------|----------|----------------------------------|
//! | CYCLES         | 0x76        | 0x00     | CPU clock cycles                 |
//! | INSTRUCTIONS   | 0xC0        | 0x00     | Retired instructions             |
//! | L1D_ACCESS     | 0x40        | 0xFF     | L1 data cache accesses           |
//! | L1D_MISS       | 0x41        | 0xFF     | L1 data cache misses             |
//! | L2_ACCESS      | 0x43        | 0xFF     | L2 cache accesses                |
//! | L2_MISS        | 0x45        | 0xFF     | L2 cache misses                  |
//! | BRANCH_RETIRED | 0xC2        | 0x00     | Retired branch instructions      |
//! | BRANCH_MISPR   | 0xC3        | 0x00     | Retired mispredicted branches    |
//! | ICACHE_MISS    | 0x81        | 0x00     | Instruction cache misses         |
//! | STALL_CYCLES   | 0x87        | 0x00     | Dispatch stall cycles            |
//!
//! ## Syscall interface
//!
//! ```
//! handle = SYS_PERF_OPEN(event_id)   // allocate counter, start counting
//! count  = SYS_PERF_READ(handle)     // read current 64-bit count
//! SYS_PERF_CLOSE(handle)             // release counter
//! ```
//!
//! Up to `MAX_COUNTERS` (6) simultaneous handles per system (single-core).

use core::arch::asm;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Number of general-purpose perf counters available (Zen 2/3/4).
pub const MAX_COUNTERS: usize = 6;

/// PERF_CTL base MSR (extended, Fam17h+). CTL[n] = BASE + n*2, CTR[n] = BASE + n*2 + 1.
const PERF_CTL_BASE: u32 = 0xC001_0200;
const PERF_CTR_BASE: u32 = 0xC001_0201;

/// CR4 bit 8: PCE — allow RDPMC from ring-3.
const CR4_PCE: u64 = 1 << 8;

// ---------------------------------------------------------------------------
// Event descriptors
// ---------------------------------------------------------------------------

/// Pre-defined performance event IDs exposed to userland.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PerfEvent {
    Cycles         = 0,
    Instructions   = 1,
    L1dAccess      = 2,
    L1dMiss        = 3,
    L2Access       = 4,
    L2Miss         = 5,
    BranchRetired  = 6,
    BranchMispred  = 7,
    IcacheMiss     = 8,
    StallCycles    = 9,
}

impl PerfEvent {
    /// Returns (EventSelect, UnitMask) for this event.
    fn event_select_umask(self) -> (u8, u8) {
        match self {
            PerfEvent::Cycles         => (0x76, 0x00),
            PerfEvent::Instructions   => (0xC0, 0x00),
            PerfEvent::L1dAccess      => (0x40, 0xFF),
            PerfEvent::L1dMiss        => (0x41, 0xFF),
            PerfEvent::L2Access       => (0x43, 0xFF),
            PerfEvent::L2Miss         => (0x45, 0xFF),
            PerfEvent::BranchRetired  => (0xC2, 0x00),
            PerfEvent::BranchMispred  => (0xC3, 0x00),
            PerfEvent::IcacheMiss     => (0x81, 0x00),
            PerfEvent::StallCycles    => (0x87, 0x00),
        }
    }

    fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            0 => PerfEvent::Cycles,
            1 => PerfEvent::Instructions,
            2 => PerfEvent::L1dAccess,
            3 => PerfEvent::L1dMiss,
            4 => PerfEvent::L2Access,
            5 => PerfEvent::L2Miss,
            6 => PerfEvent::BranchRetired,
            7 => PerfEvent::BranchMispred,
            8 => PerfEvent::IcacheMiss,
            9 => PerfEvent::StallCycles,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Counter slot table
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct CounterSlot {
    occupied: bool,
    /// Physical counter index (0-5).
    hw_idx: usize,
    /// Owning process index (for future per-process isolation).
    proc_idx: usize,
}

static mut SLOTS: [CounterSlot; MAX_COUNTERS] = [CounterSlot {
    occupied: false,
    hw_idx: 0,
    proc_idx: 0,
}; MAX_COUNTERS];

/// Whether PCE bit has been set in CR4.
static mut PCE_ENABLED: bool = false;

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
// Initialisation
// ---------------------------------------------------------------------------

/// Enable CR4.PCE so ring-3 can RDPMC directly (fast path for profilers).
/// Call once at boot after CR4 is set.
pub fn init() {
    unsafe {
        let cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4, options(nostack, preserves_flags));
        asm!("mov cr4, {}", in(reg) cr4 | CR4_PCE, options(nostack, preserves_flags));
        PCE_ENABLED = true;
    }
    crate::arch::x86_64::serial::write_str("[PMU] CR4.PCE set — RDPMC available from ring-3.\r\n");
}

// ---------------------------------------------------------------------------
// PERF_CTL value builder
// ---------------------------------------------------------------------------

/// Build PERF_CTL value: EventSelect, UnitMask, OS=1, User=1, Enable=1.
fn build_ctl(event: u8, umask: u8) -> u64 {
    let os_bit:   u64 = 1 << 17; // count in OS (ring 0)
    let user_bit: u64 = 1 << 16; // count in user (ring 3)
    let en_bit:   u64 = 1 << 22; // enable counter
    // EventSelect[7:0] in bits 7:0; EventSelect[11:8] in bits 35:32.
    let evsel_lo = (event as u64) & 0xFF;
    let evsel_hi = 0u64; // event < 0x100 so high bits are 0
    let umask_f  = (umask as u64) << 8;
    evsel_lo | umask_f | os_bit | user_bit | en_bit | (evsel_hi << 32)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Open a performance counter. Returns a handle (0..MAX_COUNTERS-1) or error.
pub fn perf_open(event_id: u32, proc_idx: usize) -> Result<u32, ()> {
    let event = PerfEvent::from_u32(event_id).ok_or(())?;
    let (evsel, umask) = event.event_select_umask();

    unsafe {
        // Find a free hardware counter slot.
        for i in 0..MAX_COUNTERS {
            if !SLOTS[i].occupied {
                SLOTS[i] = CounterSlot { occupied: true, hw_idx: i, proc_idx };

                let ctl_msr = PERF_CTL_BASE + (i as u32) * 2;
                let ctr_msr = PERF_CTR_BASE + (i as u32) * 2;

                // Reset counter to 0.
                write_msr(ctr_msr, 0);
                // Program event.
                write_msr(ctl_msr, build_ctl(evsel, umask));

                crate::arch::x86_64::serial::write_str("[PMU] opened counter=");
                crate::arch::x86_64::serial::write_hex(i as u64);
                crate::arch::x86_64::serial::write_str(" event=");
                crate::arch::x86_64::serial::write_hex(event_id as u64);
                crate::arch::x86_64::serial::write_str("\r\n");

                return Ok(i as u32);
            }
        }
        Err(()) // all counters in use
    }
}

/// Read the current 48-bit count for a handle. Returns error if handle invalid.
pub fn perf_read(handle: u32) -> Result<u64, ()> {
    let h = handle as usize;
    if h >= MAX_COUNTERS { return Err(()); }
    unsafe {
        if !SLOTS[h].occupied { return Err(()); }
        let ctr_msr = PERF_CTR_BASE + (h as u32) * 2;
        Ok(read_msr(ctr_msr) & 0x0000_FFFF_FFFF_FFFF)
    }
}

/// Close and stop a performance counter.
pub fn perf_close(handle: u32) -> Result<(), ()> {
    let h = handle as usize;
    if h >= MAX_COUNTERS { return Err(()); }
    unsafe {
        if !SLOTS[h].occupied { return Err(()); }
        let ctl_msr = PERF_CTL_BASE + (h as u32) * 2;
        write_msr(ctl_msr, 0); // disable
        SLOTS[h].occupied = false;
        Ok(())
    }
}

/// Stop all performance counters owned by a process (called on exit).
pub fn perf_close_for_process(proc_idx: usize) {
    unsafe {
        for i in 0..MAX_COUNTERS {
            if SLOTS[i].occupied && SLOTS[i].proc_idx == proc_idx {
                let ctl_msr = PERF_CTL_BASE + (i as u32) * 2;
                write_msr(ctl_msr, 0);
                SLOTS[i].occupied = false;
            }
        }
    }
}
