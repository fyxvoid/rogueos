//! Global Descriptor Table. Kernel and user (ring 3) code/data, plus TSS for kernel stack.
//!
//! GDT layout (indices):
//!   [0] null      0x00
//!   [1] kern code 0x08  KERNEL_CS
//!   [2] kern data 0x10  KERNEL_DS
//!   [3] user data 0x18  USER_SS  ← data BEFORE code so STAR[63:48]=0x10 gives correct SYSRET selectors
//!   [4] user code 0x20  USER_CS
//!   [5] TSS low   0x28
//!   [6] TSS high  0x30
//!
//! Descriptor u64 byte layout (little-endian): byte[5]=access, byte[6]=flags+limit_hi.
//!   access byte sits at bits 47:40, flags+limit at bits 55:48 of the u64.
//!   Wrong encoding (original): 0x00_9a_20_… → byte[5]=0x20 (P=0, wrong type), byte[6]=0x9a
//!   Correct encoding: 0x00_20_9a_… → byte[5]=0x9a (P=1,DPL=0,code), byte[6]=0x20 (L=1)

use core::arch::asm;

use super::serial;
use super::tss;

static mut GDT: [u64; 7] = [0; 7];

/// Kernel code selector (index 1).
pub const KERNEL_CS: u16 = 0x08;
/// Kernel data selector (index 2).
pub const KERNEL_DS: u16 = 0x10;
/// User data selector (index 3, DPL 3). Placed before user code for correct SYSRET (STAR[63:48]+8).
pub const USER_SS: u16 = 0x18;
/// User code selector (index 4, DPL 3). SYSRET loads CS = STAR[63:48]+16 = 0x10+16 = 0x20.
pub const USER_CS: u16 = 0x20;

#[repr(C, packed)]
struct DescriptorPtr {
    limit: u16,
    base: u64,
}

pub fn init() {
    unsafe {
        let tss_addr = tss::tss_address();
        let limit = 0x67u64; // 64-bit TSS size - 1
        GDT[0] = 0;
        // Byte[5]=access (bits 47:40), byte[6]=flags+limit_hi (bits 55:48).
        // Code: access=0x9A/0xFA (P,DPL,S=1,Type=exec/read), flags=0x20 (L=1,64-bit).
        // Data: access=0x92/0xF2 (P,DPL,S=1,Type=rw),       flags=0x00 (D=0,limit ignored in 64-bit).
        GDT[1] = 0x00_20_9a_00_00_00_00_00; // kernel code  (P=1,DPL=0,S=1,Type=A,L=1)
        GDT[2] = 0x00_00_92_00_00_00_00_00; // kernel data  (P=1,DPL=0,S=1,Type=2)
        GDT[3] = 0x00_00_f2_00_00_00_00_00; // user data    (P=1,DPL=3,S=1,Type=2)  ← USER_SS=0x18
        GDT[4] = 0x00_20_fa_00_00_00_00_00; // user code    (P=1,DPL=3,S=1,Type=A,L=1) ← USER_CS=0x20
        // 64-bit TSS descriptor (two 8-byte entries)
        GDT[5] = limit | (tss_addr & 0xFFFFFF) << 16 | 0x89 << 40 | ((tss_addr >> 24) & 0xFF) << 56;
        GDT[6] = tss_addr >> 32;
        let ptr = DescriptorPtr {
            limit: (GDT.len() * 8 - 1) as u16,
            base: GDT.as_ptr() as u64,
        };
        // #region agent log
        serial::write_str("[DBG][GDT] gdt1=");
        serial::write_hex(GDT[1]);
        serial::write_str(" gdt2=");
        serial::write_hex(GDT[2]);
        serial::write_str(" gdt3=");
        serial::write_hex(GDT[3]);
        serial::write_str(" gdt4=");
        serial::write_hex(GDT[4]);
        serial::write_str("\r\n");
        // #endregion

        asm!("lgdt [{}]", in(reg) &ptr, options(nostack, preserves_flags));
        // #region agent log
        serial::write_str("[DBG][GDT] after lgdt\r\n");
        // #endregion

        // NOTE: In 64-bit long mode, DS/ES/FS/GS base addresses are ignored for
        // flat kernels, and SS is largely unused. To avoid subtle #GP faults
        // during early bring-up, we currently keep the segment registers as
        // configured by the firmware/bootloader and only rely on our GDT for
        // the TSS descriptor used by ltr.

        tss::init();
        // #region agent log
        serial::write_str("[DBG][GDT] after ltr\r\n");
        // #endregion
    }
}
