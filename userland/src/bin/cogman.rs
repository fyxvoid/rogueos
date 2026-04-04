//! cogman -Kingdom OS init supervisor (PID 1).
//!
//! Text-mode supervisor: spawns the shell, blocks until it exits, then
//! respawns it. If the shell exits with status 42 (halt code), cogman
//! reboots the system. All other exits cause a restart.
//!
//! Constraints: no_std, no heap, no signals. Uses blocking sys_waitpid.

#![no_std]
#![no_main]

use userland::{sys_reboot, sys_spawn, sys_waitpid, sys_write};

// ── Program IDs (match kernel/init/main.rs registration) ─────────────────

const PROG_SHELL:    u32 = 0;
const PROG_EDITOR:   u32 = 2;
const PROG_VIEWER:   u32 = 3;
const PROG_COPY:     u32 = 4;
const PROG_MONITOR:  u32 = 5;
const PROG_SHUTDOWN: u32 = 6;

/// Exit status the shell uses to request a system halt (shutdown -h).
const HALT_STATUS: i32 = 42;
/// Exit status the shell uses to request a reboot.
const REBOOT_STATUS: i32 = 43;

// ── Logging ───────────────────────────────────────────────────────────────

fn log(msg: &[u8]) {
    let _ = sys_write(1, msg.as_ptr(), msg.len());
}

fn log_u32(n: u32) {
    let mut buf = [b'0'; 10];
    let mut i = 10usize;
    let mut v = n;
    if v == 0 {
        let _ = sys_write(1, b"0".as_ptr(), 1);
        return;
    }
    while v > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    let _ = sys_write(1, buf[i..].as_ptr(), 10 - i);
}

// ── Main ──────────────────────────────────────────────────────────────────

#[no_mangle]
fn _start() -> ! {
    log(b"\r\n[COGMAN] Kingdom OS supervisor started\r\n");
    log(b"[COGMAN] Programs: shell=0 editor=2 viewer=3 copy=4 monitor=5 shutdown=6\r\n");

    let mut restarts: u32 = 0;

    loop {
        // Spawn the interactive shell.
        let pid = sys_spawn(PROG_SHELL);
        if pid < 0 {
            log(b"[COGMAN] ERROR: failed to spawn shell\r\n");
            // Brief spin then retry.
            for _ in 0..1_000_000u64 {
                unsafe { core::arch::asm!("pause", options(nomem, nostack)); }
            }
            continue;
        }
        let shell_pid = pid as u32;

        log(b"[COGMAN] shell pid=");
        log_u32(shell_pid);
        if restarts > 0 {
            log(b" (restart #");
            log_u32(restarts);
            log(b")");
        }
        log(b"\r\n");

        // Block until the shell exits. Blocking waitpid: kernel suspends cogman
        // and runs the shell. When shell exits, cogman is rescheduled, re-executes
        // this syscall, and gets the exit status.
        let mut status: i32 = 0;
        let reaped = sys_waitpid(shell_pid, &mut status as *mut i32, 0);

        if reaped < 0 {
            log(b"[COGMAN] waitpid error, retrying\r\n");
            continue;
        }

        log(b"[COGMAN] shell exited status=");
        log_u32(status as u32);
        log(b"\r\n");

        if status == HALT_STATUS {
            log(b"[COGMAN] halt requested -shutting down\r\n");
            sys_reboot(0); // halt
            loop {}
        }
        if status == REBOOT_STATUS {
            log(b"[COGMAN] reboot requested\r\n");
            sys_reboot(1); // reboot
            loop {}
        }

        restarts = restarts.wrapping_add(1);
        log(b"[COGMAN] restarting shell\r\n");
    }
}
