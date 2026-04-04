//! Kingdom OS interactive shell.
//!
//! Runs as PID 2 (spawned by cogman). Reads lines from the TTY, dispatches
//! built-in commands, and spawns userland programs (blocking until they exit).
//!
//! Built-in commands
//! -----------------
//!   help            -list commands
//!   echo [text]     -print text
//!   ls              -list root filesystem
//!   ps              -show process table
//!   cat <file>      -print file contents
//!   rm  <file>      -delete a file
//!   clear           -clear the terminal
//!   pwd             -print working directory (always /)
//!   editor [file]   -run text editor (program 2)
//!   viewer <file>   -run file viewer  (program 3)
//!   copy <src> <dst>— copy a file      (program 4)
//!   monitor         -system monitor   (program 5)
//!   reboot          -reboot the system
//!   halt            -halt the system
//!   exit [status]   -exit shell (cogman restarts it)
//!
//! Programs are spawned via sys_spawn. The shell blocks (blocking waitpid)
//! until the child exits before showing the next prompt.

#![no_std]
#![no_main]

use libs::ProcInfo;
use userland::{
    sys_close, sys_exit, sys_open, sys_read,
    sys_spawn, sys_unlink, sys_waitpid, sys_write,
};

// ── Program IDs ──────────────────────────────────────────────────────────

const PROG_SHELL:    u32 = 0;
const PROG_EDITOR:   u32 = 2;
const PROG_VIEWER:   u32 = 3;
const PROG_COPY:     u32 = 4;
const PROG_MONITOR:  u32 = 5;
const PROG_SHUTDOWN: u32 = 6;

/// Exit status cogman interprets as halt.
const HALT_STATUS: i32 = 42;
/// Exit status cogman interprets as reboot.
const REBOOT_STATUS: i32 = 43;

// ── VFS open flags (matches kernel) ──────────────────────────────────────

const O_RDONLY: u32 = 0;
const O_WRONLY: u32 = 1;
const O_RDWR:   u32 = 2;
const O_CREAT:  u32 = 0x40;
const O_TRUNC:  u32 = 0x200;

// ── Line buffer ───────────────────────────────────────────────────────────

const LINE:     usize = 512;
const READ_BUF: usize = 4096;
const LIST_BUF: usize = 2048;

// ── Helpers ───────────────────────────────────────────────────────────────

fn write_str(s: &[u8]) {
    let _ = sys_write(1, s.as_ptr(), s.len());
}

fn write_nl() {
    write_str(b"\r\n");
}

fn u32_to_dec(n: u32, buf: &mut [u8; 12]) -> &[u8] {
    let mut i = 12usize;
    let mut v = n;
    if v == 0 {
        i -= 1;
        buf[i] = b'0';
    } else {
        while v > 0 && i > 0 {
            i -= 1;
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
    }
    &buf[i..]
}

/// Trim leading/trailing spaces and strip CR/LF.
fn trim(line: &[u8]) -> &[u8] {
    let mut s = line;
    while s.first() == Some(&b' ') { s = &s[1..]; }
    while s.last() == Some(&b' ')
       || s.last() == Some(&b'\r')
       || s.last() == Some(&b'\n') {
        s = &s[..s.len() - 1];
    }
    s
}

/// Split `cmd_name <rest>` -returns (cmd, rest).
fn split_cmd(line: &[u8]) -> (&[u8], &[u8]) {
    let mut i = 0;
    while i < line.len() && line[i] != b' ' { i += 1; }
    let cmd = &line[..i];
    let rest = if i < line.len() { trim(&line[i + 1..]) } else { &[] };
    (cmd, rest)
}

/// Parse `arg1 arg2` -returns (arg1, arg2) splitting on first space.
fn split_two(s: &[u8]) -> (&[u8], &[u8]) {
    let mut i = 0;
    while i < s.len() && s[i] != b' ' { i += 1; }
    let a = &s[..i];
    let b = if i < s.len() { trim(&s[i + 1..]) } else { &[] };
    (a, b)
}

/// Parse decimal integer from ascii bytes. Returns None on empty / non-numeric.
fn parse_u32(s: &[u8]) -> Option<u32> {
    if s.is_empty() { return None; }
    let mut n = 0u32;
    for &b in s {
        if b < b'0' || b > b'9' { return None; }
        n = n.wrapping_mul(10).wrapping_add((b - b'0') as u32);
    }
    Some(n)
}

// ── Blocking child spawn + wait ───────────────────────────────────────────

/// Spawn program `id`, block until it exits, return exit status.
fn run_program(id: u32) -> i32 {
    let pid = sys_spawn(id);
    if pid < 0 {
        write_str(b"shell: spawn failed\r\n");
        return -1;
    }
    let mut status: i32 = 0;
    let _ = sys_waitpid(pid as u32, &mut status as *mut i32, 0);
    status
}

// ── Built-in: ps ─────────────────────────────────────────────────────────

fn cmd_ps() {
    use userland::sys_get_proc_info;
    let mut buf = [ProcInfo { pid: 0, state: 0 }; 64];
    let n = sys_get_proc_info(buf.as_mut_ptr(), 64);
    if n < 0 {
        write_str(b"ps: error\r\n");
        return;
    }
    write_str(b"PID   STATE\r\n");
    write_str(b"----  -----\r\n");
    let state_name = |s: u8| match s {
        0 => b"empty    " as &[u8],
        1 => b"runnable ",
        2 => b"running  ",
        3 => b"blocked  ",
        4 => b"dead     ",
        _ => b"unknown  ",
    };
    for i in 0..n as usize {
        let p = &buf[i];
        if p.pid == 0 { continue; }
        let mut nbuf = [0u8; 12];
        let pid_s = u32_to_dec(p.pid, &mut nbuf);
        write_str(pid_s);
        // pad to 6 chars
        for _ in pid_s.len()..6 { write_str(b" "); }
        write_str(state_name(p.state));
        write_nl();
    }
}

// ── Built-in: cat ─────────────────────────────────────────────────────────

fn cmd_cat(name: &[u8]) {
    if name.is_empty() {
        write_str(b"usage: cat <file>\r\n");
        return;
    }
    let fd = sys_open(name.as_ptr(), name.len(), O_RDONLY);
    if fd < 0 {
        write_str(b"cat: cannot open file\r\n");
        return;
    }
    let fd = fd as u32;
    let mut buf = [0u8; READ_BUF];
    loop {
        let n = sys_read(fd, buf.as_mut_ptr(), READ_BUF);
        if n <= 0 { break; }
        let _ = sys_write(1, buf.as_ptr(), n as usize);
    }
    write_nl();
    let _ = sys_close(fd);
}

// ── Built-in: rm ──────────────────────────────────────────────────────────

fn cmd_rm(name: &[u8]) {
    if name.is_empty() {
        write_str(b"usage: rm <file>\r\n");
        return;
    }
    let r = sys_unlink(name.as_ptr(), name.len());
    if r < 0 {
        write_str(b"rm: failed\r\n");
    }
}

// ── Built-in: ls ─────────────────────────────────────────────────────────

fn cmd_ls() {
    use userland::sys_list_root;
    let mut buf = [0u8; LIST_BUF];
    let n = sys_list_root(buf.as_mut_ptr(), LIST_BUF);
    if n > 0 {
        let _ = sys_write(1, buf.as_ptr(), n as usize);
    }
    write_nl();
}

// ── Built-in: help ────────────────────────────────────────────────────────

fn cmd_help() {
    write_str(b"\r\nKingdom OS shell commands:\r\n\r\n");
    write_str(b"  help              -show this help\r\n");
    write_str(b"  ls                -list filesystem root\r\n");
    write_str(b"  cat <file>        -print file contents\r\n");
    write_str(b"  rm  <file>        -delete a file\r\n");
    write_str(b"  echo [text]       -print text\r\n");
    write_str(b"  ps                -show process table\r\n");
    write_str(b"  pwd               -print working directory\r\n");
    write_str(b"  clear             -clear the terminal\r\n");
    write_str(b"  editor [file]     -open text editor (id=2)\r\n");
    write_str(b"  viewer <file>     -view a file (id=3)\r\n");
    write_str(b"  copy <src> <dst>  -copy a file (id=4)\r\n");
    write_str(b"  monitor           -system monitor (id=5)\r\n");
    write_str(b"  run <id>          -spawn program by numeric id\r\n");
    write_str(b"  reboot            -reboot the system\r\n");
    write_str(b"  halt              -halt the system\r\n");
    write_str(b"  exit [code]       -exit shell\r\n\r\n");
}

// ── Built-in: clear ───────────────────────────────────────────────────────

fn cmd_clear() {
    // ANSI clear screen + cursor home
    write_str(b"\x1b[2J\x1b[H");
}

// ── Main loop ─────────────────────────────────────────────────────────────

#[no_mangle]
fn _start() -> ! {
    cmd_clear();
    write_str(b"Kingdom OS shell. Type 'help' for commands.\r\n\r\n");

    let mut line_buf = [0u8; LINE];

    loop {
        // Print prompt.
        write_str(b"/ $ ");

        // Read a line (blocking until newline arrives).
        let n = sys_read(0, line_buf.as_mut_ptr(), LINE);
        if n <= 0 {
            continue;
        }
        let line = trim(&line_buf[..n as usize]);
        if line.is_empty() {
            continue;
        }

        let (cmd, rest) = split_cmd(line);

        if cmd == b"exit" {
            let code = parse_u32(rest).unwrap_or(0) as i32;
            sys_exit(code);
        }

        if cmd == b"halt" {
            write_str(b"Halting...\r\n");
            sys_exit(HALT_STATUS);
        }

        if cmd == b"reboot" {
            write_str(b"Rebooting...\r\n");
            sys_exit(REBOOT_STATUS);
        }

        if cmd == b"help" {
            cmd_help();
            continue;
        }

        if cmd == b"clear" {
            cmd_clear();
            continue;
        }

        if cmd == b"echo" {
            write_str(rest);
            write_nl();
            continue;
        }

        if cmd == b"pwd" {
            write_str(b"/\r\n");
            continue;
        }

        if cmd == b"ls" {
            cmd_ls();
            continue;
        }

        if cmd == b"ps" {
            cmd_ps();
            continue;
        }

        if cmd == b"cat" {
            cmd_cat(rest);
            continue;
        }

        if cmd == b"rm" {
            cmd_rm(rest);
            continue;
        }

        if cmd == b"editor" || cmd == b"ed" {
            let _ = run_program(PROG_EDITOR);
            continue;
        }

        if cmd == b"viewer" || cmd == b"view" {
            let _ = run_program(PROG_VIEWER);
            continue;
        }

        if cmd == b"copy" || cmd == b"cp" {
            let _ = run_program(PROG_COPY);
            continue;
        }

        if cmd == b"monitor" || cmd == b"top" {
            let _ = run_program(PROG_MONITOR);
            continue;
        }

        if cmd == b"run" {
            let id = match parse_u32(rest) {
                Some(v) => v,
                None => {
                    write_str(b"run: usage: run <program_id>\r\n");
                    continue;
                }
            };
            let _ = run_program(id);
            continue;
        }

        // Unknown command.
        write_str(b"shell: unknown command: ");
        write_str(cmd);
        write_str(b"\r\n       (type 'help' for a list of commands)\r\n");
    }
}
