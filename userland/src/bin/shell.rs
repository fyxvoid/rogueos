//! Shell: read line, built-ins exit, echo, ls, pwd, cd, run.

#![no_std]
#![no_main]

use userland::{sys_exit, sys_list_root, sys_read, sys_spawn, sys_write};

const PROMPT: &[u8] = b"$ ";
const LINE_SIZE: usize = 256;
const LIST_BUF_SIZE: usize = 512;

/// Program id by name: 0=shell, 1=wm, 2=editor, 3=viewer, 4=copy, 5=monitor, 6=shutdown
fn program_id_by_name(name: &[u8]) -> Option<u32> {
    if name == b"shell" { return Some(0) }
    if name == b"wm" { return Some(1) }
    if name == b"editor" { return Some(2) }
    if name == b"viewer" { return Some(3) }
    if name == b"copy" { return Some(4) }
    if name == b"monitor" { return Some(5) }
    if name == b"shutdown" { return Some(6) }
    None
}

fn trim_line(line: &[u8], len: usize) -> &[u8] {
    let mut end = len;
    while end > 0 && (line[end - 1] == b' ' || line[end - 1] == b'\r' || line[end - 1] == b'\n') {
        end -= 1;
    }
    let mut start = 0;
    while start < end && line[start] == b' ' {
        start += 1;
    }
    &line[start..end]
}

fn cmd_is(line: &[u8], len: usize, cmd: &[u8]) -> bool {
    let t = trim_line(line, len);
    if t.len() < cmd.len() { return false; }
    if &t[..cmd.len()] != cmd { return false; }
    t.len() == cmd.len() || t[cmd.len()] == b' '
}

fn cmd_arg(line: &[u8], len: usize, cmd_len: usize) -> &[u8] {
    let t = trim_line(line, len);
    if t.len() <= cmd_len { return &[]; }
    let rest = &t[cmd_len..];
    let start = if rest[0] == b' ' { 1 } else { 0 };
    let mut end = start;
    while end < rest.len() && rest[end] != b' ' && rest[end] != b'\r' && rest[end] != b'\n' {
        end += 1;
    }
    &rest[start..end]
}

#[no_mangle]
fn _start() -> ! {
    let mut line = [0u8; LINE_SIZE];
    loop {
        let _ = sys_write(1, PROMPT.as_ptr(), PROMPT.len());
        let n = sys_read(0, line.as_mut_ptr(), LINE_SIZE);
        if n <= 0 {
            continue;
        }
        let len = n as usize;

        if cmd_is(&line, len, b"exit") {
            sys_exit(0);
        }
        if cmd_is(&line, len, b"echo") {
            let start = if len > 4 && line[4] == b' ' { 5 } else { 4 };
            let _ = sys_write(1, line[start..].as_ptr(), len.saturating_sub(start));
            continue;
        }
        if cmd_is(&line, len, b"ls") {
            let mut buf = [0u8; LIST_BUF_SIZE];
            let r = sys_list_root(buf.as_mut_ptr(), LIST_BUF_SIZE);
            if r > 0 {
                let _ = sys_write(1, buf.as_ptr(), r as usize);
            }
            continue;
        }
        if cmd_is(&line, len, b"pwd") {
            let _ = sys_write(1, b".\n".as_ptr(), 2);
            continue;
        }
        if cmd_is(&line, len, b"cd") {
            // No-op for flat root FS
            continue;
        }
        if cmd_is(&line, len, b"run") {
            let arg = cmd_arg(&line, len, 3);
            if arg.is_empty() {
                let _ = sys_write(1, b"run: need program name\n".as_ptr(), 22);
                continue;
            }
            let pid = if arg.iter().all(|&b| b.is_ascii_digit()) && !arg.is_empty() {
                let mut id: u32 = 0;
                for &b in arg {
                    id = id.wrapping_mul(10).wrapping_add((b - b'0') as u32);
                }
                sys_spawn(id)
            } else {
                match program_id_by_name(arg) {
                    Some(id) => sys_spawn(id),
                    None => {
                        let _ = sys_write(1, b"run: unknown program\n".as_ptr(), 21);
                        continue;
                    }
                }
            };
            if pid < 0 {
                let _ = sys_write(1, b"run: spawn failed\n".as_ptr(), 18);
            }
            continue;
        }

        // Unknown: echo back
        let _ = sys_write(1, line.as_ptr(), len);
    }
}
