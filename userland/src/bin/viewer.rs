//! File viewer (cat): read filename from stdin, print file contents to stdout.

#![no_std]
#![no_main]

use libs::O_RDONLY;
use userland::{sys_close, sys_exit, sys_open, sys_read, sys_write};

const LINE_SIZE: usize = 256;
const BUF_SIZE: usize = 512;
const PROMPT: &[u8] = b"file: ";

fn trim_crlf(line: &[u8], len: usize) -> usize {
    let mut end = len;
    while end > 0 && (line[end - 1] == b'\r' || line[end - 1] == b'\n') {
        end -= 1;
    }
    end
}

#[no_mangle]
fn _start() -> ! {
    let mut line = [0u8; LINE_SIZE];
    let mut name = [0u8; 32];

    let _ = sys_write(1, PROMPT.as_ptr(), PROMPT.len());
    let n = sys_read(0, line.as_mut_ptr(), LINE_SIZE);
    if n <= 0 {
        sys_exit(1);
    }
    let len = trim_crlf(&line, n as usize);
    if len == 0 || len >= name.len() {
        let _ = sys_write(1, b"name too long\n".as_ptr(), 14);
        sys_exit(1);
    }
    name[..len].copy_from_slice(&line[..len]);
    name[len] = 0;

    let fd = sys_open(name.as_ptr(), 0, O_RDONLY);
    if fd < 0 {
        let _ = sys_write(1, b"open failed\n".as_ptr(), 12);
        sys_exit(1);
    }
    let fd = fd as u32;

    let mut buf = [0u8; BUF_SIZE];
    loop {
        let r = sys_read(fd, buf.as_mut_ptr(), BUF_SIZE);
        if r <= 0 {
            break;
        }
        let _ = sys_write(1, buf.as_ptr(), r as usize);
    }

    let _ = sys_close(fd);
    sys_exit(0);
}

