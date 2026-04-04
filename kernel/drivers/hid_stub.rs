//! Input polling: delegates to the PS/2 keyboard driver.
//!
//! Called from `sys_poll_input` to drain the PS/2 output buffer and push
//! `KeyEvent`s into the kernel input ring queue before userland reads them.
//! USB HID can be wired in here when implemented.

/// Poll hardware input sources and push events into the kernel input queue.
/// Must be called before reading from `drivers::input`.
#[inline]
pub fn poll_input() {
    crate::arch::x86_64::ps2::poll_and_push();
}
