# Userland Design

## Purpose

Userland is a minimal set of Rust binaries (init, shell, wm, editor, viewer, copy, monitor, shutdown) and syscall wrappers. It is no_std, depends only on the `libs` crate for the kernel ABI, and does not link against glibc or any POSIX library. All code is original.

## Design Choices

- **Rust-only**: All binaries and the shared userland library are written in Rust. No C runtime or coreutils.
- **no_std**: No standard library; only core and the project’s `libs` for syscall numbers, error codes, and ABI types.
- **Shell**: Minimal shell with built-in commands (exit, echo, ls, run, etc.). Implementation is original; no code derived from bash, zsh, dash, or other GPL shells.
- **Utilities**: Each binary (editor, viewer, copy, monitor, shutdown, wm) is an independent implementation. No copying of GNU coreutils or BusyBox code.
- **Syscall interface**: Wrappers in userland call the kernel via the ABI defined in `libs` (syscall numbers, calling convention). No POSIX compliance layer.

## Implementation

- **lib.rs**: Syscall wrappers (sys_read, sys_write, sys_open, etc.) using inline asm and constants from libs.
- **bin/init**: First user process; starts WM and shell.
- **bin/shell**: Line read loop, built-in dispatch, program_id_by_name for run.
- **bin/wm, editor, viewer, copy, monitor, shutdown**: Each implements its own behavior against the kernel ABI.

No references to external shell or utility source. No GPL or POSIX source derivation.
