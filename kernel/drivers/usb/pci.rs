//! Minimal PCI config space access (port I/O 0xCF8 / 0xCFC) for xHCI discovery.

use core::arch::asm;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

fn config_address(bus: u8, device: u8, func: u8, offset: u16) -> u32 {
    (1 << 31)
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC)
}

/// Read a 32-bit PCI config register.
pub fn read_config(bus: u8, device: u8, func: u8, offset: u16) -> u32 {
    let addr = config_address(bus, device, func, offset);
    unsafe {
        asm!("out dx, eax", in("dx") CONFIG_ADDRESS, in("eax") addr, options(nostack, preserves_flags));
        let mut val: u32;
        asm!("in eax, dx", in("dx") CONFIG_DATA, out("eax") val, options(nostack, preserves_flags));
        val
    }
}

/// PCI class code: offset 0x08, upper 16 bits. 0x0C0330 = USB xHCI.
pub const CLASS_XHCI: u32 = 0x0C0330;

/// Get class/subclass/prog_if from config header (offset 8).
pub fn read_class(bus: u8, device: u8, func: u8) -> u32 {
    read_config(bus, device, func, 0x08) >> 8
}

/// Get BAR0 (offset 0x10). Returns physical address (may be 64-bit; lower 32 here).
pub fn read_bar0(bus: u8, device: u8, func: u8) -> u32 {
    read_config(bus, device, func, 0x10)
}

/// Scan first bus for xHCI controller. Returns (bus, device, func) or None.
pub fn find_xhci() -> Option<(u8, u8, u8)> {
    for device in 0..32u8 {
        for func in 0..8u8 {
            let class = read_class(0, device, func);
            if class == CLASS_XHCI {
                return Some((0, device, func));
            }
        }
    }
    None
}
