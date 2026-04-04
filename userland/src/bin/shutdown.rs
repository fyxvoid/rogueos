//! Shutdown/reboot utility. Calls SYS_REBOOT (0=halt, 1=reboot). Stub until kernel implements SYS_REBOOT.

#![no_std]
#![no_main]

use userland::{sys_exit, sys_reboot, sys_write};

#[no_mangle]
fn _start() -> ! {
    let _ = sys_write(1, b"rebooting...\n".as_ptr(), 13);
    let _ = sys_reboot(1);
    sys_exit(0);
}
