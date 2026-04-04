//! x86_64 hardware debug registers (DR0–DR7) management.
//!
//! Hardware breakpoints let ring-3 code (pentesters, debuggers, profilers) set
//! break-on-execute, break-on-write, and break-on-read/write conditions on up
//! to 4 virtual addresses — WITHOUT patching target memory (invisible to code
//! integrity checks, works on ROM/MMIO too).
//!
//! ## Architecture
//!
//! - DR0–DR3: linear (virtual) addresses for each breakpoint.
//! - DR6: debug status — which breakpoint fired (read after #DB trap).
//! - DR7: control — enable/disable each DR, condition (execute/write/rw), length.
//!
//! ## Kernel responsibilities
//!
//! 1. Save/restore DR0–DR7 per-process on context switch.
//! 2. Validate userland-requested addresses (must be in user VA range).
//! 3. Expose set/clear/query via syscall so ring-3 tools can use hardware BP
//!    without needing a kernel debugger stub.
//!
//! ## DR7 encoding
//!
//! ```text
//! Bit 0   (L0): local enable for DR0
//! Bit 2   (L1): local enable for DR1
//! Bit 4   (L2): local enable for DR2
//! Bit 6   (L3): local enable for DR3
//! Bits 16-17 (C0): condition for DR0 (00=exec, 01=write, 11=rw)
//! Bits 18-19 (S0): size for DR0 (00=1B, 01=2B, 10=8B, 11=4B)
//! ... repeated for C1/S1 at 20-23, C2/S2 at 24-27, C3/S3 at 28-31
//! ```

use core::arch::asm;

/// Maximum hardware breakpoints per process.
pub const MAX_HW_BREAKPOINTS: usize = 4;

/// Breakpoint condition type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum BpCondition {
    /// Break on instruction execution (DR7 condition = 00).
    Execute  = 0b00,
    /// Break on data write (DR7 condition = 01).
    Write    = 0b01,
    /// Break on I/O read/write — requires CR4.DE (not set here; falls back to Write).
    IoRw     = 0b10,
    /// Break on data read or write (not execute) (DR7 condition = 11).
    ReadWrite = 0b11,
}

/// Breakpoint length (bytes to watch).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum BpLen {
    Byte  = 0b00,
    Word  = 0b01,
    Qword = 0b10,
    Dword = 0b11,
}

/// Per-process hardware breakpoint state (saved/restored on context switch).
#[derive(Clone, Copy)]
pub struct HwBpState {
    pub dr0: u64,
    pub dr1: u64,
    pub dr2: u64,
    pub dr3: u64,
    /// DR7 control register value.
    pub dr7: u64,
    /// DR6 status (populated after a #DB fires; readable via SYS_HW_BP_QUERY).
    pub dr6: u64,
}

impl HwBpState {
    pub const fn new() -> Self {
        HwBpState { dr0: 0, dr1: 0, dr2: 0, dr3: 0, dr7: 0x400, dr6: 0 }
    }

    /// Returns true if breakpoint slot `idx` is currently enabled.
    pub fn is_enabled(&self, idx: usize) -> bool {
        if idx >= MAX_HW_BREAKPOINTS { return false; }
        let local_enable_bit = 1u64 << (idx * 2); // L0=bit0, L1=bit2, L2=bit4, L3=bit6
        self.dr7 & local_enable_bit != 0
    }

    /// Set a hardware breakpoint.
    ///
    /// `addr`  — virtual address to watch.
    /// `cond`  — Execute / Write / ReadWrite.
    /// `len`   — 1/2/4/8 bytes (ignored for Execute; must be Byte).
    pub fn set(&mut self, idx: usize, addr: u64, cond: BpCondition, len: BpLen) {
        if idx >= MAX_HW_BREAKPOINTS { return; }

        // Store address in appropriate DR.
        match idx {
            0 => self.dr0 = addr,
            1 => self.dr1 = addr,
            2 => self.dr2 = addr,
            3 => self.dr3 = addr,
            _ => unreachable!(),
        }

        // DR7: clear existing condition/size bits for this slot, then set new.
        // Local enable bit position: idx * 2.
        // Condition bits: 16 + idx*4, size bits: 18 + idx*4.
        let local_bit = 1u64 << (idx * 2);
        let cond_shift = 16 + idx * 4;
        let len_shift  = 18 + idx * 4;

        let mask = local_bit
            | (0b11u64 << cond_shift)
            | (0b11u64 << len_shift);
        self.dr7 &= !mask;
        self.dr7 |= local_bit;
        self.dr7 |= (cond as u64) << cond_shift;
        self.dr7 |= (len  as u64) << len_shift;

        // GD bit (13) must stay clear; GE bits not needed for local.
        // Bit 10 (GE) reserved=1 per Intel/AMD spec.
        self.dr7 |= 1 << 10; // always set reserved bit 10
    }

    /// Clear a hardware breakpoint slot.
    pub fn clear(&mut self, idx: usize) {
        if idx >= MAX_HW_BREAKPOINTS { return; }
        match idx {
            0 => self.dr0 = 0,
            1 => self.dr1 = 0,
            2 => self.dr2 = 0,
            3 => self.dr3 = 0,
            _ => {}
        }
        let local_bit = 1u64 << (idx * 2);
        let cond_shift = 16 + idx * 4;
        let len_shift  = 18 + idx * 4;
        self.dr7 &= !(local_bit | (0b11u64 << cond_shift) | (0b11u64 << len_shift));
    }
}

// ---------------------------------------------------------------------------
// Raw DR read/write
// ---------------------------------------------------------------------------

pub unsafe fn write_dr(state: &HwBpState) {
    asm!(
        "mov dr0, {dr0}",
        "mov dr1, {dr1}",
        "mov dr2, {dr2}",
        "mov dr3, {dr3}",
        "mov dr7, {dr7}",
        dr0 = in(reg) state.dr0,
        dr1 = in(reg) state.dr1,
        dr2 = in(reg) state.dr2,
        dr3 = in(reg) state.dr3,
        dr7 = in(reg) state.dr7,
        options(nostack, preserves_flags)
    );
}

pub unsafe fn read_dr(state: &mut HwBpState) {
    asm!(
        "mov {dr0}, dr0",
        "mov {dr1}, dr1",
        "mov {dr2}, dr2",
        "mov {dr3}, dr3",
        "mov {dr6}, dr6",
        "mov {dr7}, dr7",
        dr0 = out(reg) state.dr0,
        dr1 = out(reg) state.dr1,
        dr2 = out(reg) state.dr2,
        dr3 = out(reg) state.dr3,
        dr6 = out(reg) state.dr6,
        dr7 = out(reg) state.dr7,
        options(nostack, preserves_flags)
    );
}

/// Load a zeroed-out debug state (disable all BPs) into the hardware registers.
/// Called on entry to kernel context / on process switch to a task with no BPs.
pub fn clear_dr_hardware() {
    let state = HwBpState::new();
    unsafe { write_dr(&state); }
}
