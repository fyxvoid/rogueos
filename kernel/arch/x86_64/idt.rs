//! Interrupt Descriptor Table. 256 entries. Syscall entry is via SYSCALL (MSR LSTAR), not IDT.
//! Vectors: 6=#UD, 8=#DF, 13=#GP, 14=#PF, 0x20=IRQ0.

use core::arch::asm;

use crate::memory::paging::fault;
use super::exceptions;

#[repr(C, packed)]
struct IdtPtr {
    limit: u16,
    base: u64,
}

const IDT_LEN: usize = 256;
static mut IDT: [u128; IDT_LEN] = [0; IDT_LEN];

fn set_trap_gate(slot: usize, addr: u64, selector: u16, type_attr: u8) {
    let e = (addr & 0xFFFF) as u128
        | (selector as u128) << 16
        | (type_attr as u128) << 40
        | (((addr >> 16) & 0xFFFF) as u128) << 48
        | (((addr >> 32) & 0xFFFF_FFFF) as u128) << 64;
    unsafe {
        IDT[slot] = e;
    }
}

pub fn init() {
    const UD_VECTOR: u8 = 6;
    const PAGE_FAULT_VECTOR: u8 = 14;
    const GP_FAULT_VECTOR: u8 = 13;
    const DOUBLE_FAULT_VECTOR: u8 = 8;
    const IRQ0_VECTOR: u8 = 0x20;
    const KERNEL_CS: u16 = 0x08;
    const INTR_GATE: u8 = 0x8E;  /* P=1, DPL=0, type=14 (64-bit interrupt) */

    unsafe {
        set_trap_gate(UD_VECTOR as usize, exceptions::ud_fault_stub as *const () as u64, KERNEL_CS, INTR_GATE);
        set_trap_gate(PAGE_FAULT_VECTOR as usize, fault::page_fault_stub as *const () as u64, KERNEL_CS, INTR_GATE);
        set_trap_gate(GP_FAULT_VECTOR as usize, exceptions::gp_fault_stub as *const () as u64, KERNEL_CS, INTR_GATE);
        set_trap_gate(DOUBLE_FAULT_VECTOR as usize, exceptions::double_fault_stub as *const () as u64, KERNEL_CS, INTR_GATE);
        set_trap_gate(IRQ0_VECTOR as usize, exceptions::irq0_stub as *const () as u64, KERNEL_CS, INTR_GATE);

        let ptr = IdtPtr {
            limit: (IDT_LEN * 16 - 1) as u16,
            base: IDT.as_ptr() as u64,
        };
        asm!("lidt [{}]", in(reg) &ptr, options(nostack, preserves_flags));
    }
}
