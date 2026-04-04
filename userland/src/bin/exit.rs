//! Minimal binary for SYS_EXIT stress test (STEP 5). Just exits with status 0.
#![no_std]
#![no_main]

use userland::sys_exit;

#[no_mangle]
fn _start() -> ! {
    sys_exit(0);
}
