//! Copy file: read src and dst names from stdin, copy src to dst.

#![no_std]
#![no_main]

use libs::{O_RDONLY, O_WRONLY, O_CREAT};
use userland::{sys_close, sys_exit, sys_open, sys_read, sys_write};

const BUF_SIZE: usize = 512;
const PROMPT1: &[u8] = b"src: ";
const PROMPT2: &[u8] = b"dst: ";

fn trim_crlf(line: &[u8], len: usize) -> usize {
    let mut end = len;
    while end > 0 && (line[end - 1] == b'\r' || line[end - 1] == b'\n') {
        end -= 1;
    }
    end
}

fn read_filename(prompt: &[u8], name: &mut [u8; 32]) -> bool {
    let _ = sys_write(1, prompt.as_ptr(), prompt.len());
    let mut line = [0u8; 256];
    let n = sys_read(0, line.as_mut_ptr(), 256);
    if n <= 0 {
        return false;
    }
    let len = trim_crlf(&line, n as usize);
    if len >= name.len() {
        return false;
    }
    name[..len].copy_from_slice(&line[..len]);
    name[len] = 0;
    true
}

#[no_mangle]
fn _start() -> ! {
    let mut src_name = [0u8; 32];
    let mut dst_name = [0u8; 32];
    if !read_filename(PROMPT1, &mut src_name) || !read_filename(PROMPT2, &mut dst_name) {
        let _ = sys_write(1, b"bad input\n".as_ptr(), 10);
        sys_exit(1);
    }

    let src_fd = sys_open(src_name.as_ptr(), 0, O_RDONLY);
    if src_fd < 0 {
        let _ = sys_write(1, b"open src failed\n".as_ptr(), 16);
        sys_exit(1);
    }
    let dst_fd = sys_open(dst_name.as_ptr(), 0, O_WRONLY | O_CREAT);
    if dst_fd < 0 {
        let _ = sys_close(src_fd as u32);
        let _ = sys_write(1, b"open dst failed\n".as_ptr(), 16);
        sys_exit(1);
    }

    let mut buf = [0u8; BUF_SIZE];
    loop {
        let r = sys_read(src_fd as u32, buf.as_mut_ptr(), BUF_SIZE);
        if r <= 0 {
            break;
        }
        let _ = sys_write(dst_fd as u32, buf.as_ptr(), r as usize);
    }
    let _ = sys_close(src_fd as u32);
    let _ = sys_close(dst_fd as u32);
    sys_exit(0);
}
