//! Serial port (COM1) for kernel input and output. Safe for early boot.

const COM1: u16 = 0x3F8;
const RBR: u16 = 0;  // receive buffer (read)
const THR: u16 = 0;  // transmit hold (write)
const LSR: u16 = 5;  // line status
const LSR_THRE: u8 = 0x20;  // transmitter holding register empty
const LSR_DR: u8 = 0x01;    // data ready (receive)

/// Initialize COM1 at 115200 8N1, no FIFO (polling mode).
pub fn init() {
    use crate::arch::x86_64::port::outb;
    unsafe {
        outb(COM1 + 1, 0x00); // disable UART interrupts
        outb(COM1 + 3, 0x80); // DLAB on to set baud divisor
        outb(COM1 + 0, 0x01); // divisor lo byte: 1 = 115200 baud
        outb(COM1 + 1, 0x00); // divisor hi byte
        outb(COM1 + 3, 0x03); // 8 data bits, no parity, 1 stop bit; DLAB off
        outb(COM1 + 2, 0x00); // FIFO disabled (avoid threshold-trigger issues)
        outb(COM1 + 4, 0x03); // DTR + RTS, no OUT2 (polling, no IRQ needed)
    }
}

/// Returns true if at least one byte is available to read.
pub fn has_byte() -> bool {
    unsafe { (crate::arch::x86_64::port::inb(COM1 + LSR) & LSR_DR) != 0 }
}

/// Read one byte from COM1 if available.
pub fn read_byte() -> Option<u8> {
    if has_byte() {
        Some(unsafe { crate::arch::x86_64::port::inb(COM1 + RBR) })
    } else {
        None
    }
}

pub(crate) fn putchar(c: u8) {
    unsafe {
        while (crate::arch::x86_64::port::inb(COM1 + LSR) & LSR_THRE) == 0 {}
        crate::arch::x86_64::port::outb(COM1 + THR, c);
    }
}

pub fn write_str(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            putchar(b'\r');
        }
        putchar(b);
    }
}

const HEX: [u8; 16] = *b"0123456789abcdef";

pub fn write_hex(mut n: u64) {
    if n == 0 {
        putchar(b'0');
        return;
    }
    let mut buf = [0u8; 16];
    let mut i = 0;
    while n > 0 && i < 16 {
        buf[i] = HEX[(n & 0xF) as usize];
        n >>= 4;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        putchar(buf[i]);
    }
}

pub fn write_fmt(args: core::fmt::Arguments) {
    use core::fmt::Write;
    struct SerialWriter;
    impl Write for SerialWriter {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            write_str(s);
            Ok(())
        }
    }
    SerialWriter.write_fmt(args).ok();
}
