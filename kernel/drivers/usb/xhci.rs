//! xHCI controller skeleton: map BAR and init. No device enumeration yet.
//! When complete, HID keyboard/mouse will be claimed and reports pushed to input queue.

use crate::memory::paging;

const PAGE_SIZE: u64 = 4096;

/// Map xHCI BAR (MMIO) and return virtual base. Caller uses cap/op regs from here.
pub fn map_bar(bar_phys: u64) -> bool {
    if bar_phys == 0 || (bar_phys & 1) != 0 {
        return false;
    }
    let base = bar_phys & !0xF;
    let size = 64 * 1024;
    let start = base & !(PAGE_SIZE - 1);
    let end = (base + size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let mut pa = start;
    while pa < end {
        if !paging::map_page_identity(pa, paging::EntryFlags::kernel_rw().as_u64()) {
            return false;
        }
        pa += PAGE_SIZE;
    }
    true
}

/// Init xHCI: find via PCI, map BAR. Returns true if controller found and mapped.
/// Full init (reset, run, device poll) is left for future; HID events come from poll_input.
pub fn init() -> bool {
    let (bus, dev, func) = match super::pci::find_xhci() {
        Some(t) => t,
        None => return false,
    };
    let bar0 = super::pci::read_bar0(bus, dev, func);
    if (bar0 & 1) != 0 {
        return false;
    }
    map_bar(bar0 as u64)
}
