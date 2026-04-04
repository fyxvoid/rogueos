//! Global Descriptor Table. Kernel and user (ring 3) code/data, plus TSS for kernel stack.

use core::arch::asm;

use super::serial;
use super::tss;

static mut GDT: [u64; 7] = [0; 7];

/// Kernel code selector (index 1).
pub const KERNEL_CS: u16 = 0x08;
/// Kernel data selector (index 2).
pub const KERNEL_DS: u16 = 0x10;
/// User code selector (index 3, DPL 3).
pub const USER_CS: u16 = 0x18;
/// User data selector (index 4, DPL 3).
pub const USER_SS: u16 = 0x20;

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
        GDT[1] = 0x00_9a_20_00_00_00_00_00; // kernel code
        // Data segments must NOT have the 64-bit "L" bit set (that bit is for code segments).
        GDT[2] = 0x00_92_00_00_00_00_00_00; // kernel data
        GDT[3] = 0x00_fa_20_00_00_00_00_00; // user code (DPL 3)
        GDT[4] = 0x00_f2_00_00_00_00_00_00; // user data (DPL 3)
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
