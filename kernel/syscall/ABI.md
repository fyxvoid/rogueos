# Syscall ABI

## Calling Convention

- **Trigger**: SYSCALL instruction (only supported entry).
- **Syscall number**: In `rax` (u64).
- **Arguments**: `rdi`, `rsi`, `rdx`, **r10**, `r8`, `r9` (u64). The 4th argument is in **r10** (rcx is overwritten by the CPU with user RIP on SYSCALL).
- **Return**: In `rax`. Non-negative for success (e.g. bytes read, fd, pid, count). Negative for error (see error codes).

## Error Codes

Returned as negative values in `rax`:

| Code       | Value | Meaning                          |
|-----------|-------|----------------------------------|
| SYSERR_INVAL | -1  | Invalid argument / bad request   |
| SYSERR_NOENT | -2  | No such file or entry           |
| SYSERR_BADFD | -3  | Bad file descriptor / handle    |
| SYSERR_MFILE | -4  | Too many open files             |

## Descriptor Model

- **0**: stdin (TTY read).
- **1**: stdout (TTY write).
- **2**: stderr (TTY write).
- **3 and above**: VFS file handles from open(); close, read, write, seek, fsync apply.

## Syscall Number Ranges

- **0x100–0x109**: I/O and file (read, write, open, close, lseek, unlink, fsync, list_root, reboot, exit).
- **0x200–0x204**: Graphics and input (poll_input, fb_clear, fb_fill_rect, fb_flush, poll_mouse).
- **0x300–0x304**: Process and debug (debug_dump_ptes, spawn, get_proc_info, getpid, waitpid).

Exact constants are defined in the `libs` crate; kernel and userland both use those definitions.

## Process syscalls

- **SYS_GETPID (0x303)**: No args. Returns current process ID or error.
- **SYS_WAITPID (0x304)**: Args: pid (0 or u32::MAX = reap any dead process), status_ptr (*mut i32 or null to receive exit status), options (0). Returns reaped pid or error. Reaps one dead (zombie) process; exit status is stored in the descriptor until reaped.

## Debug syscall

- **SYS_DEBUG_DUMP_PTES (0x300)**: Args: va_start, va_end (a2, a3; a1/cr3 is ignored). Dumps page table entries for the **current process’s** address space only.
