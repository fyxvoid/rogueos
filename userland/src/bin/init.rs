//! Steward (init): spawns the single unified userland session binary only.
//! Session runs server + compositor + WM; it may spawn shell (program_id 0) for terminal.

#![no_std]
#![no_main]

use libs::KeyEvent;
use userland::{sys_poll_input, sys_spawn, sys_write};

const SESSION_PID: u32 = 1; // unified session (server + compositor + WM)

fn log(msg: &[u8]) {
    let _ = sys_write(1, msg.as_ptr(), msg.len());
}

fn delay_ready(iterations: u32) {
    let mut ev = KeyEvent { keycode: 0, pressed: false };
    for _ in 0..iterations {
        let _ = sys_poll_input(&mut ev);
    }
}

#[no_mangle]
fn _start() -> ! {
    log(b"[INIT] steward start\r\n");

    log(b"[INIT] spawn session\r\n");
    let r = sys_spawn(SESSION_PID);
    if r < 0 {
        log(b"[INIT] session spawn failed\r\n");
    }
    delay_ready(15000);

    // Steward blocks; do not exit.
    let mut ev = KeyEvent { keycode: 0, pressed: false };
    loop {
        let _ = sys_poll_input(&mut ev);
    }
}
