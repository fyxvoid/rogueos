//! System monitor: list processes (pid, state) using SYS_GET_PROC_INFO.

#![no_std]
#![no_main]

use libs::ProcInfo;
use userland::{sys_exit, sys_get_proc_info, sys_write};

const MAX_PROCS: u32 = 16;
const STATE_NAMES: [&[u8]; 5] = [
    b"Empty",
    b"Runnable",
    b"Running",
    b"Blocked",
    b"Dead",
];

#[no_mangle]
fn _start() -> ! {
    let mut infos = [ProcInfo { pid: 0, state: 0 }; 16];
    let n = sys_get_proc_info(infos.as_mut_ptr(), MAX_PROCS);
    if n < 0 {
        let _ = sys_write(1, b"get_proc_info failed\n".as_ptr(), 20);
        sys_exit(1);
    }
    let n = n as usize;
    let header = b"pid   state\n";
    let _ = sys_write(1, header.as_ptr(), header.len());
    for info in infos.iter().take(n) {
        let st = info.state as usize;
        let name = if st < 5 { STATE_NAMES[st] } else { b"?" };
        let mut line = [0u8; 32];
        let mut pos = 0;
        let pid = info.pid;
        if pid >= 100 {
            line[pos] = b'0' + (pid / 100) as u8;
            pos += 1;
        }
        if pid >= 10 {
            line[pos] = b'0' + ((pid / 10) % 10) as u8;
            pos += 1;
        }
        line[pos] = b'0' + (pid % 10) as u8;
        pos += 1;
        line[pos] = b' ';
        pos += 1;
        line[pos] = b' ';
        pos += 1;
        for &b in name {
            if pos < line.len() {
                line[pos] = b;
                pos += 1;
            }
        }
        line[pos] = b'\n';
        pos += 1;
        let _ = sys_write(1, line.as_ptr(), pos);
    }
    sys_exit(0);
}
