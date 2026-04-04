//! Minimal 64-bit TSS for kernel RSP on ring 3 -> ring 0 transition.

use core::arch::asm;

/// 64-bit TSS: only RSP0 is used when interrupt from ring 3.
#[repr(C, packed)]
pub struct Tss {
    _reserved0: u32,
    pub rsp0_low: u32,
    pub rsp0_high: u32,
    _rsp1: u64,
    _rsp2: u64,
    _reserved1: u64,
    _ist1: u64,
    _ist2: u64,
    _ist3: u64,
    _ist4: u64,
    _ist5: u64,
    _ist6: u64,
    _ist7: u64,
    _reserved2: u64,
    _reserved3: u16,
    iomap_base: u16,
}

impl Tss {
    pub const fn new() -> Self {
        Tss {
            _reserved0: 0,
            rsp0_low: 0,
            rsp0_high: 0,
            _rsp1: 0,
            _rsp2: 0,
            _reserved1: 0,
            _ist1: 0,
            _ist2: 0,
            _ist3: 0,
            _ist4: 0,
            _ist5: 0,
            _ist6: 0,
            _ist7: 0,
            _reserved2: 0,
            _reserved3: 0,
            iomap_base: core::mem::size_of::<Tss>() as u16,
        }
    }

    /// Set kernel stack pointer for next iretq to user. Call before entering user.
    pub fn set_rsp0(&mut self, rsp: u64) {
        self.rsp0_low = (rsp & 0xFFFF_FFFF) as u32;
        self.rsp0_high = (rsp >> 32) as u32;
    }
}

static mut TSS: Tss = Tss::new();

/// TSS selector (GDT index 5).
pub const TSS_SELECTOR: u16 = 0x28;

/// Address of TSS for GDT descriptor.
pub fn tss_address() -> u64 {
    core::ptr::addr_of!(TSS) as u64
}

pub fn init() {
    unsafe {
        asm!("ltr ax", in("ax") TSS_SELECTOR, options(nostack, preserves_flags));
    }
}

/// Set kernel stack for the process we're about to run. Call before enter_user.
pub fn set_kernel_rsp(rsp: u64) {
    unsafe {
        TSS.set_rsp0(rsp);
    }
}
