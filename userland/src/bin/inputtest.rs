//! inputtest — Stage 2: keyboard input pipeline verification.
//!
//! Polls SYS_POLL_INPUT in a loop and writes each keycode as a hex byte
//! to fd 1 (serial). Exit on ESC.
//!
//! Done when: pressing a key in QEMU produces output on the serial console.

#![no_std]
#![no_main]

use userland::{sys_exit, sys_poll_input, sys_write};
use libs::{KeyEvent, keycodes::KEY_ESC};

static MSG_READY: &[u8] = b"[inputtest] ready - press keys (ESC to exit)\r\n";
static MSG_KEY:   &[u8] = b"[inputtest] key=0x";
static MSG_CRLF:  &[u8] = b"\r\n";

fn write_hex_byte(v: u8) {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let buf = [HEX[(v >> 4) as usize], HEX[(v & 0xF) as usize]];
    let _ = sys_write(1, buf.as_ptr(), 2);
}

fn write_str(s: &[u8]) {
    let _ = sys_write(1, s.as_ptr(), s.len());
}

#[no_mangle]
fn _start() -> ! {
    write_str(MSG_READY);

    let mut ev = KeyEvent { keycode: 0, pressed: false };
    loop {
        let n = sys_poll_input(&mut ev);
        if n > 0 {
            write_str(MSG_KEY);
            write_hex_byte(ev.keycode);
            let press = if ev.pressed { b" dn" } else { b" up" };
            let _ = sys_write(1, press.as_ptr(), 3);
            write_str(MSG_CRLF);

            if ev.pressed && ev.keycode == KEY_ESC {
                write_str(b"[inputtest] ESC pressed, exiting.\r\n");
                sys_exit(0);
            }
        }
    }
}
