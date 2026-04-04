//! Debug: canary and stack checks for process subsystem.

use super::pid;

/// Check canary at bottom of current process kernel stack. Halt on corruption.
pub fn check_current_kernel_stack_canary() {
    pid::check_kernel_stack_canary();
}
