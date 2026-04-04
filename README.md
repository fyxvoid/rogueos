# Kingdom OS

A clean-room, Rust-only operating system for x86_64. No Linux, no libc, no POSIX.

---

## What is Kingdom?

Kingdom is a minimal, research-grade operating system written entirely in Rust. Every subsystem — bootloader, kernel, drivers, window manager, shell, and init — is original code with no GPL or POSIX derivation.

The design makes four bets:

1. **Rust everywhere.** Memory safety by construction. No unsafe data races. Panic = halt.
2. **Spawn, never fork.** Processes are created by ID, not cloned. Clean semantics, no COW ghosts.
3. **IPC-first.** Processes talk through typed 64-byte messages. No shared memory by default.
4. **Cogman init.** The first userland process is a full supervisor that watches, heals, and controls everything beneath it.

---

## Repository Layout

```
kingdom/
├── lib/            Shared ABI: syscall numbers, KwmMsg, BootInfo, error codes
├── boot/           UEFI bootloader (exits boot services, jumps to kernel)
├── kernel/         Kernel crate (no_std, x86_64-unknown-none)
│   ├── arch/       x86_64: GDT, IDT, TSS, SYSCALL MSR, PS/2, serial, SME
│   ├── memory/     Physical allocator, paging, heap, virtual address spaces
│   ├── process/    Process table, PID, EEVDF scheduler, ELF loader, IPC queues
│   ├── syscall/    Syscall dispatch and user-pointer validation
│   ├── drivers/    Framebuffer, NVMe, TTY, USB/HID, input ring
│   ├── display/    Display server, compositor, surface protocol
│   ├── fs/         VFS abstraction, simple filesystem
│   └── init/       kernel_main, panic handler, embedded ELF registry
├── userland/       Userland crate (no_std, x86_64-unknown-none)
│   ├── src/lib.rs          Syscall wrappers + bump allocator
│   └── src/bin/
│       ├── cogman.rs       Init supervisor (PID 1) — spawn, watch, heal
│       ├── init.rs         Legacy steward (fallback)
│       ├── shell.rs        Interactive shell
│       ├── session.rs      Compositor + WM session
│       ├── rwm.rs          RogueWM window manager
│       ├── editor.rs       Terminal text editor
│       ├── monitor.rs      Process and perf monitor
│       └── shutdown.rs     Reboot / halt
└── docs/           Architecture, design notes, build status
```

---

## Quick Start

### Requirements

- Rust nightly (`rustup target add x86_64-unknown-none`)
- `lld` linker (`apt install lld` or `brew install llvm`)
- QEMU ≥ 7.0 with SDL (`qemu-system-x86_64`)
- OVMF firmware (`apt install ovmf` or `brew install qemu`)

### Build and Run

```sh
# Build all (lib → userland → kernel → UEFI image)
./scripts/build_os.sh

# Run under QEMU
export OVMF_CODE=/usr/share/ovmf/OVMF_CODE.fd
export OVMF_VARS=/usr/share/ovmf/OVMF_VARS.fd
qemu-system-x86_64 \
  -machine q35 -cpu qemu64,+sse4.2,+avx \
  -smp 2 -m 4096 \
  -drive if=pflash,format=raw,readonly=on,file=$OVMF_CODE \
  -drive if=pflash,format=raw,file=$OVMF_VARS \
  -drive file=fat:rw:build/uefi-boot,if=virtio,format=raw \
  -serial stdio -display sdl

# Or via Makefile
make all && make run
```

### Debug

```sh
# GDB attach (kernel pauses at entry)
make debug
# In another terminal:
rust-gdb \
  -ex 'target remote :1234' \
  -ex 'symbol-file target/x86_64-unknown-none/release/kernel' \
  target/x86_64-unknown-none/release/kernel

# QEMU exception log (triple faults, GPF, etc.)
QEMU_DEBUG_LOG=build/qemu_debug.log make run
```

Serial output is the primary debug channel. All kernel and userland subsystems write prefixed lines:

```
[KRN]     kernel subsystems
[COGMAN]  cogman supervisor
[SHELL]   shell
[WM]      window manager
```

---

## Boot Sequence

```
UEFI firmware
  └── boot/ (UEFI bootloader)
        Locates kernel.elf on the ESP
        Queries GOP framebuffer
        Probes NVMe BAR
        Captures UEFI memory map
        Writes BootInfo to 0x8000
        ExitBootServices → jumps to kernel_main
          └── kernel/init/main.rs :: kernel_main()
                Phase 0:  IDT, GDT, TSS, SYSCALL MSR
                Phase 0b: AMD SME (memory encryption)
                Phase 1:  TTY + PS/2 keyboard
                Phase 2:  Kernel heap + page-fault PID hook
                Phase 2b: PMU (AMD performance counters)
                Phase 3:  NVMe driver
                Phase 4:  Simple filesystem mount
                Phase 5:  Framebuffer (GOP)
                Phase 6:  Register userland ELFs (program IDs 0–10)
                Phase 7:  create_user_process(COGMAN_ELF) → run_first_process()
                            └── cogman :: _start()   [PID 1, ring 3]
                                  Spawn session (program_id 8)   → PID 2
                                  Supervisor loop: reap → restart → IPC
```

---

## Architecture Overview

### Memory (`kernel/memory/`)

| Subsystem | File | Role |
|-----------|------|------|
| Physical allocator | `physical/frame_allocator.rs` | Buddy-like; seeded from UEFI memory map |
| Paging | `paging/mapper.rs` | Identity-maps kernel + frame region; switches CR3 |
| Page fault | `paging/fault.rs` | Kills offending process; does not panic the kernel |
| Kernel heap | `heap/allocator.rs` | Slab allocator backing `alloc::` in the kernel |
| Virtual spaces | `virtual/address_space.rs` | Per-process VA bookkeeping (in progress) |

Address layout:
```
0x0010_0000          Kernel load base (1 MiB)
0x0090_0000          End of 8 MiB kernel identity window
0x0040_0000          User ELF load base (USER_LOAD_BASE)
0x7fff_ffff_f000     User stack top (USER_STACK_TOP)
```

### Process (`kernel/process/`)

| Subsystem | File | Role |
|-----------|------|------|
| Descriptor | `process.rs` | PID, state, CR3, kernel stack, saved TrapFrame |
| PID table | `pid.rs` | Static `[Option<ProcessDescriptor>; 10]` |
| Lifecycle | `lifecycle.rs` | `create_user_process`, `exit_current_and_schedule` |
| Scheduler | `scheduler/eevdf.rs` | EEVDF (Earliest Eligible Virtual Deadline First) |
| ELF loader | `loader/elf.rs` | Loads PT_LOAD segments; sets entry RIP |
| IPC queues | `ipc.rs` | Per-process 64-slot KwmMsg ring; enqueue/dequeue |
| Context | `context/mod.rs` | `enter_user()` — iretq into ring 3 |

Process states: `Empty → Runnable → Running → Dead`

### Syscall (`kernel/syscall/`)

The SYSCALL instruction transfers control to `arch/x86_64/syscall_entry.rs`. The dispatcher in `syscall/dispatcher/mod.rs` routes by `rax` to per-namespace handlers. See [docs/syscall-abi.md](docs/syscall-abi.md) for the full table.

### Drivers (`kernel/drivers/`)

| Driver | File | Status |
|--------|------|--------|
| Framebuffer | `framebuffer.rs` | GOP linear buffer, blit, fill, clear |
| NVMe | `nvme.rs` | MMIO, submission/completion queues |
| TTY | `tty.rs` | Serial-backed terminal for kernel shell |
| PS/2 | `arch/x86_64/ps2.rs` | Keyboard scan-code translation |
| USB/HID | `usb/xhci.rs` | XHCI controller stub |
| Input ring | `input.rs` | Shared `KeyEvent` ring; fed by PS/2 / HID |

### Display (`kernel/display/`)

The kernel hosts a lightweight display server. Userland apps create surfaces via syscall, attach pixel buffers, and commit them. The compositor blits surfaces to the GOP framebuffer in z-order.

See [docs/display.md](docs/display.md) for the surface protocol.

### Filesystem (`kernel/fs/`)

- **VFS layer** (`vfs.rs`): open, read, write, close, lseek, unlink, fsync, list_root.
- **SimpleFS** (`simple_fs.rs`): flat file table on NVMe, no directories, no permissions.
- Mount point: `mount_root()` tries NVMe volume 0 after NVMe init.

### Init (`kernel/init/`)

`kernel_main` lives here. Also contains:
- `programs.rs` — `register(program_id, elf)` / `get_elf(id)` static program table.
- `panic.rs` — prints backtrace to serial and halts.
- `diagnostic.rs` — `diagnostic_halt(reason)` for non-fatal kernel stops.

---

## Userland

All binaries are compiled to `x86_64-unknown-none` (no OS, no libc). The kernel embeds them as `include_bytes!` and loads them on demand via `SYS_SPAWN`.

### Program IDs

| ID | Binary | Description |
|----|--------|-------------|
| 0  | shell   | Interactive command shell |
| 1  | rwm     | RogueWM window manager |
| 2  | editor  | Terminal text editor |
| 3  | viewer  | File viewer |
| 4  | copy    | File copy utility |
| 5  | monitor | Process and PMU monitor |
| 6  | shutdown | Reboot / halt |
| 7  | exit    | Minimal exit stub (testing) |
| 8  | session | Compositor + WM session |
| 9  | wm      | Legacy WM (fallback) |
| 10 | cogman  | Init supervisor (self-spawn slot) |

### Cogman Supervisor (PID 1)

Cogman is the init process. It replaces the minimal steward with a full supervisor loop:

```
_start()
  register_services()          [session=auto, shell=on-demand, monitor=on-demand]
  start_pending()              [spawns session → PID 2]
  loop:
    reap_dead()                [sys_waitpid(ANY, WNOHANG)]
      → update service state
      → schedule restart per policy
    tick_restarts()            [countdown timers]
    start_pending()            [spawn anything due]
    handle_ipc()               [drain KwmMsg queue: CogList/Status/Start/Stop/Restart]
    spin(1000 ticks)
```

Restart policies: `Never`, `OnFailure` (non-zero exit), `Always`.

Any process can send a `CogCtrl` IPC message to PID 1 to query or control services. See [docs/cogman.md](docs/cogman.md).

---

## IPC Protocol

Messages are fixed 64-byte `KwmMsg` structs (cache-line aligned):

```
offset  0  msg_type   u8        (KwmType enum value)
offset  1  flags      u8
offset  2  seq        u16       (monotonic, wraps at u16::MAX)
offset  4  sender_pid u32       (filled by kernel)
offset  8  payload    [u8; 56]  (union of typed payload structs)
```

Send: `SYS_IPC_SEND(target_pid, msg_ptr, flags)` → 0 or negative error  
Receive: `SYS_IPC_RECV(out_ptr, flags)` → 0 or `SYSERR_AGAIN` (if `IPC_NONBLOCK`)

See [docs/ipc.md](docs/ipc.md) for all message types and payload layouts.

---

## Contributing

- All new code goes in the appropriate kernel subsystem directory.
- Each directory with significant logic should have a `DESIGN.md` explaining algorithm origin.
- No GPL code, no POSIX derivation. If in doubt, write a `DESIGN.md` first.
- Kernel panics are not acceptable for user-caused errors — return a `SysErr` instead.
- Run `cargo clippy --target x86_64-unknown-none` before submitting.

See [docs/dev-notes.md](docs/dev-notes.md) for coding standards.

---

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the full 10-phase development plan covering SMP, network stack, capability security, GPU acceleration, and formal verification.
