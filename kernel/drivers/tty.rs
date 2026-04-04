//! TTY: serial driver → line discipline (N_TTY-style) → single console TTY for fd 0/1/2.
//! One ring buffer for input; output goes straight to serial. Line discipline: echo, backspace.

use crate::arch::x86_64::serial;

const RING_SIZE: usize = 512;
const BACKSPACE: u8 = 0x7f;
const BS: u8 = 0x08;

// Output scrollback: last N lines, fixed width.
const SCROLL_LINES: usize = 256;
const SCROLL_COLS: usize = 128;
static mut SCROLL: [[u8; SCROLL_COLS]; SCROLL_LINES] = [[0; SCROLL_COLS]; SCROLL_LINES];
static mut SCROLL_LEN: [u8; SCROLL_LINES] = [0; SCROLL_LINES];
static mut SCROLL_LINE: usize = 0;
static mut SCROLL_COL: usize = 0;

static mut RING: [u8; RING_SIZE] = [0; RING_SIZE];
static mut HEAD: usize = 0;
static mut TAIL: usize = 0;
static mut INIT: bool = false;

fn scroll_push_byte(c: u8) {
    unsafe {
        if c == b'\n' || c == b'\r' {
            SCROLL_LINE = (SCROLL_LINE + 1) % SCROLL_LINES;
            SCROLL_COL = 0;
            SCROLL_LEN[SCROLL_LINE] = 0;
            return;
        }
        if SCROLL_COL < SCROLL_COLS {
            SCROLL[SCROLL_LINE][SCROLL_COL] = c;
            SCROLL_COL += 1;
            SCROLL_LEN[SCROLL_LINE] = SCROLL_COL as u8;
        }
    }
}

fn ring_count() -> usize {
    let (h, t) = unsafe { (HEAD, TAIL) };
    if h >= t {
        h - t
    } else {
        RING_SIZE - t + h
    }
}

fn ring_push(b: u8) -> bool {
    let (next, tail) = unsafe { ((HEAD + 1) % RING_SIZE, TAIL) };
    if next == tail {
        return false;
    }
    unsafe {
        RING[HEAD] = b;
        HEAD = next;
    }
    true
}

fn ring_pop() -> Option<u8> {
    let (tail, head) = unsafe { (TAIL, HEAD) };
    if tail == head {
        return None;
    }
    let b = unsafe { RING[tail] };
    unsafe {
        TAIL = (tail + 1) % RING_SIZE;
    }
    Some(b)
}

/// Initialize TTY (call once after serial::init).
pub fn init() {
    unsafe {
        HEAD = 0;
        TAIL = 0;
        SCROLL_LINE = 0;
        SCROLL_COL = 0;
        SCROLL_LEN = [0; SCROLL_LINES];
        INIT = true;
    }
}

/// Poll serial and push received bytes into ring buffer; apply line discipline (echo, backspace).
pub fn poll() {
    while let Some(b) = serial::read_byte() {
        if b == BACKSPACE || b == BS {
            if ring_count() > 0 {
                let _ = ring_pop();
                write_str("\x08 \x08");
            }
            continue;
        }
        if b >= 0x20 || b == b'\n' || b == b'\r' || b == b'\t' {
            if ring_push(b) {
                putchar(b);
            }
        }
    }
}

/// One character to TTY (serial output).
pub fn putchar(c: u8) {
    if c == b'\n' {
        serial::putchar(b'\r');
    }
    scroll_push_byte(c);
    serial::putchar(c);
}

/// Write string to TTY.
pub fn write_str(s: &str) {
    for b in s.bytes() {
        putchar(b);
    }
}

/// Read one byte from TTY if available.
pub fn getchar() -> Option<u8> {
    poll();
    ring_pop()
}

/// Read until newline or buffer full; return length. Does not include newline in buffer.
pub fn getline(buf: &mut [u8]) -> usize {
    let mut i = 0;
    while i < buf.len() {
        poll();
        match ring_pop() {
            None => continue,
            Some(b'\n') | Some(b'\r') => break,
            Some(b) => {
                buf[i] = b;
                i += 1;
            }
        }
    }
    i
}
