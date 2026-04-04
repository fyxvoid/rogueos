//! PS/2 keyboard driver — polling, scan code set 1 (QEMU default).
//!
//! Called from `sys_poll_input` to drain the PS/2 output buffer into the
//! kernel input ring queue before the userland WM reads from it.
//! No interrupts required; purely polling-based.

use super::port::{inb, outb};
use libs::KeyEvent;
use libs::keycodes::*;

const PS2_DATA:   u16 = 0x60;
const PS2_STATUS: u16 = 0x64;
/// Bit 0 of status register: output buffer full (data ready to read).
const OUTPUT_FULL: u8 = 0x01;

/// Track extended-key prefix (0xE0) between scan code bytes.
static mut EXTENDED: bool = false;

/// Track raw modifier bits for internal driver use.
/// bit 0 = shift, bit 1 = ctrl, bit 2 = alt, bit 3 = super.
static mut MOD_BITS: u8 = 0;

/// Initialize the PS/2 controller: flush the output buffer.
pub fn init() {
    unsafe {
        // Drain any stale bytes in the output buffer.
        for _ in 0..16u8 {
            if inb(PS2_STATUS) & OUTPUT_FULL == 0 {
                break;
            }
            let _ = inb(PS2_DATA);
        }
        // Enable keyboard (command 0xAE).
        outb(PS2_STATUS, 0xAE);
    }
    crate::arch::serial::write_str("[PS2] keyboard driver ready\r\n");
}

/// Poll the PS/2 output buffer and push any pending `KeyEvent`s into the
/// kernel input queue.  Call this from `sys_poll_input` before reading.
pub fn poll_and_push() {
    // Drain up to 32 scan codes per poll to avoid starving the syscall.
    for _ in 0..32u8 {
        let status = unsafe { inb(PS2_STATUS) };
        if status & OUTPUT_FULL == 0 {
            break;
        }
        let sc = unsafe { inb(PS2_DATA) };
        process_scancode(sc);
    }
}

fn process_scancode(sc: u8) {
    // Extended prefix — next byte is an extended scan code.
    if sc == 0xE0 {
        unsafe { EXTENDED = true; }
        return;
    }

    let extended = unsafe { EXTENDED };
    unsafe { EXTENDED = false; }

    let pressed = (sc & 0x80) == 0;
    let sc_base = sc & 0x7F; // Strip the break bit.

    let keycode: u8 = if extended {
        match sc_base {
            0x48 => KEY_UP,
            0x50 => KEY_DOWN,
            0x4B => KEY_LEFT,
            0x4D => KEY_RIGHT,
            0x5B => KEY_MOD,   // Left Super (Win key)
            0x5C => KEY_MOD,   // Right Super
            0x1C => KEY_ENTER, // Numpad Enter
            0x35 => KEY_SLASH, // Numpad /
            _ => 0,
        }
    } else {
        // PS/2 scan code set 1 — standard keyboard layout.
        match sc_base {
            0x01 => KEY_ESC,
            0x02 => KEY_1,
            0x03 => KEY_2,
            0x04 => KEY_3,
            0x05 => KEY_4,
            0x06 => KEY_5,
            0x07 => KEY_6,
            0x08 => KEY_7,
            0x09 => KEY_8,
            0x0A => KEY_9,
            0x0B => KEY_0,
            0x0C => KEY_MINUS,
            0x0D => KEY_EQUAL,
            0x0E => KEY_BACKSPACE,
            0x0F => KEY_TAB,
            0x10 => KEY_Q,
            0x11 => KEY_W,
            0x12 => KEY_E,
            0x13 => KEY_R,
            0x14 => KEY_T,
            0x15 => KEY_Y,
            0x16 => KEY_U,
            0x17 => KEY_I,
            0x18 => KEY_O,
            0x19 => KEY_P,
            0x1A => KEY_LBRACE,
            0x1B => KEY_RBRACE,
            0x1C => KEY_ENTER,
            0x1D => KEY_CTRL,
            0x1E => KEY_A,
            0x1F => KEY_S,
            0x20 => KEY_D,
            0x21 => KEY_F,
            0x22 => KEY_G,
            0x23 => KEY_H,
            0x24 => KEY_J,
            0x25 => KEY_K,
            0x26 => KEY_L,
            0x27 => KEY_SEMI,
            0x28 => KEY_QUOTE,
            0x29 => KEY_GRAVE,
            0x2A => KEY_SHIFT,  // Left Shift
            0x2B => KEY_BSLASH,
            0x2C => KEY_Z,
            0x2D => KEY_X,
            0x2E => KEY_C,
            0x2F => KEY_V,
            0x30 => KEY_B,
            0x31 => KEY_N,
            0x32 => KEY_M,
            0x33 => KEY_COMMA,
            0x34 => KEY_PERIOD,
            0x35 => KEY_SLASH,
            0x36 => KEY_SHIFT,  // Right Shift
            0x38 => KEY_ALT,
            0x39 => KEY_SPACE,
            0x3B => KEY_F1,
            0x3C => KEY_F2,
            0x3D => KEY_F3,
            0x3E => KEY_F4,
            0x3F => KEY_F5,
            0x40 => KEY_F6,
            0x41 => KEY_F7,
            0x42 => KEY_F8,
            0x43 => KEY_F9,
            0x44 => KEY_F10,
            0x57 => KEY_F11,
            0x58 => KEY_F12,
            _ => 0,
        }
    };

    if keycode == 0 {
        return;
    }

    // Track modifier state for internal use.
    unsafe {
        match keycode {
            k if k == KEY_SHIFT => {
                if pressed { MOD_BITS |= 0x01; } else { MOD_BITS &= !0x01; }
            }
            k if k == KEY_CTRL => {
                if pressed { MOD_BITS |= 0x02; } else { MOD_BITS &= !0x02; }
            }
            k if k == KEY_ALT => {
                if pressed { MOD_BITS |= 0x04; } else { MOD_BITS &= !0x04; }
            }
            k if k == KEY_MOD => {
                if pressed { MOD_BITS |= 0x08; } else { MOD_BITS &= !0x08; }
            }
            _ => {}
        }
    }

    // Push every key event (press and release) so the WM can track modifiers.
    crate::drivers::input::push_event(KeyEvent { keycode, pressed });
}
