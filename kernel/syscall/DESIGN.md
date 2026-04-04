# Syscall Layer Design

## Purpose

The syscall layer provides the only controlled entry from userland into the kernel. It is an original interface: numbering, calling convention, and error semantics are defined for this system only.

## Algorithm / Concept Origin

- **Trap-based entry**: SYSCALL/SYSRET is the only syscall entry. MSR LSTAR and kernel stack are set at process run; implementation and dispatch table are independent.
- **Error returns**: Negative return values indicate errors; small positive identifiers are used (SYSERR_*). This is a common pattern; the specific codes and names are project-defined.

## Design Choices

- **Spawn-only process model**: No fork/clone. Process creation is via a single spawn-by-program-id syscall.
- **Descriptor model**: Handles 0 and 1 are TTY (stdin/stdout); 2 is TTY (stderr); 3 and above are VFS file handles. No replication of external ABI.
- **Namespaces**: Syscall numbers are grouped (I/O 0x100, graphics 0x200, process 0x300) for clarity and future extension without collision.

## Implementation

- Dispatcher reads syscall number from rax, arguments from rdi, rsi, rdx, r10, r8, r9. Return value is written to rax.
- **Typed dispatch**: Handlers return `Result<u64, SysErr>`; a single `result_to_rax` maps Ok/Err to the rax return value.
- **Central user memory validation**: All syscalls that accept user pointers use `validate_user_range(cr3, ptr, len)` (and `current_cr3()`) from the `user_ptr` module before dereferencing. This ensures user addresses are in the current address space and mapped.
- Each handler validates arguments, calls the appropriate kernel subsystem (process, fs, drivers), and returns a result or negative SYSERR_*.
- No references to external syscall tables or ABIs. See ABI.md for the concrete calling convention and descriptor semantics.

## Rust and assembly only

The OS is **Rust and assembly only**: no C or C++ source.

- **Rust**: All kernel, bootloader (boot), and userland logic.
- **Assembly**: One optional file, `arch/x86_64/boot_multiboot2.S`, used only for the GRUB/Multiboot2 boot path (32-bit bootstrap and long-mode entry). All other low-level code uses Rust and `core::arch::asm!` where needed.
- **Linker scripts** (`.ld`) are for layout only, not "source" in the language sense.
- **Build**: With `multiboot2` feature, the boot stub is assembled by an external assembler (e.g. `gcc -c`); default UEFI build compiles no .S files.

## Single-user OS

The OS is **single-user** (like macOS): one session, no login or multi-user. All user processes run under one identity (`DEFAULT_SESSION_UID` / UID_PRINCE in libs). There are no separate user accounts or permission checks beyond kernel vs user. The single session runs init, Director (WM), Throne (shell), and spawned programs all with the same UID.
