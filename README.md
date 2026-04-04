# RogueOS

A clean-room, Rust-only operating system for x86_64. No Linux, no libc, no POSIX.

---

## What is RogueOS?

RogueOS is a minimal, research-grade operating system written entirely in Rust. Every subsystem — bootloader, kernel, drivers, window manager, shell, and init — is original code with no GPL or POSIX derivation.

The design makes four bets:

1. **Rust everywhere.** Memory safety by construction. No unsafe data races. Panic = halt.
2. **Spawn, never fork.** Processes are created by ID, not cloned. Clean semantics, no COW ghosts.
3. **IPC-first.** Processes talk through typed 64-byte messages. No shared memory by default.
4. **Cogman init.** The first userland process is a full supervisor that watches, heals, and controls everything beneath it.

---

## Repository Layout

```
rogueos/
├── lib/            Shared ABI: syscall numbers, RwmMsg, BootInfo, error codes
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
| IPC queues | `ipc.rs` | Per-process 64-slot RwmMsg ring; enqueue/dequeue |
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
    handle_ipc()               [drain RwmMsg queue: CogList/Status/Start/Stop/Restart]
    spin(1000 ticks)
```

Restart policies: `Never`, `OnFailure` (non-zero exit), `Always`.

Any process can send a `CogCtrl` IPC message to PID 1 to query or control services. See [docs/cogman.md](docs/cogman.md).

---

## IPC Protocol

Messages are fixed 64-byte `RwmMsg` structs (cache-line aligned):

```
offset  0  msg_type   u8        (RwmType enum value)
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

---

## Vision & Evolution Blueprint

### Executive Vision

RogueOS exists to be the final operating system a developer, pentester, or power user ever needs. It is a clean-room, 100% Rust, capability-first OS that treats memory protection, process isolation, and reliability as non-negotiable hardware-enforced invariants — exactly what Multics promised in 1969 but could never fully deliver because it was written in PL/I on 36-bit hardware. RogueOS finishes what Multics started, but in 2026-era Rust on x86_64: zero ambient authority, spawn-only processes, typed IPC, Cogman as immortal PID 1, and a surgical Rust-native dwm clone called roguewm as the only desktop environment. The entire system is engineered for extreme focus, auditability, and raw speed.

---

### Core Dominance Pillars

#### Process Model
- **Current:** Spawn-only processes with basic Cogman supervision.
- **Peak target:** Every process is a sealed capability container. No `fork()`, only `SYS_SPAWN` with an explicit capability mask. Processes have no ambient authority whatsoever — they can only do what their capability tokens explicitly allow. Cogman is the only process allowed to grant/revoke capabilities and can restart any service in < 5 ms with zero data loss (journaled state).
- **Domination:** Linux fork/exec is fundamentally broken for security; Windows job objects are coarse; seL4 is correct but unusable. RogueOS makes every process as isolated as a VM but with zero overhead and full Rust type safety.

#### Memory Management
- **Current:** Global buddy allocator + basic paging.
- **Peak target:** Per-process page tables with strict user/kernel split. Every allocation is backed by a capability. Guard pages everywhere. On any page fault the offending process is instantly terminated (Multics-style fail-fast). SME + SMAP + KPTI-style kernel mappings + optional AMD memory encryption for all user pages. Single-level-store semantics via capability-mapped objects — no "files" vs "memory" distinction at the kernel level.
- **Domination:** No other OS gives you Multics rings + Rust borrow-checker + sub-page capabilities in one package.

#### Security Model
- **Current:** Basic rings via paging.
- **Peak target:** Pure capability system (`u128` unforgeable tokens with embedded rights bitmap). `SYS_CAP_GRANT`, `SYS_CAP_REVOKE`, `SYS_SANDBOX(policy)`. File descriptors, IPC ports, surfaces, network sockets — everything is a capability. No UID/GID, no sudo, no setuid. Kernel itself runs with the minimal capability set possible.
- **Domination:** More secure than seL4 (because Rust eliminates entire classes of bugs) and more usable than any capability OS ever shipped.

#### Reliability / Supervision
- **Current:** Cogman as PID 1.
- **Peak target:** Cogman is immortal. Every service declares its dependencies and recovery policy in a declarative manifest. On crash, Cogman replays the journal and restarts the exact dependency graph. Kernel never panics — it only kills the offending process. Watchdog + triple modular redundancy for critical kernel paths where possible.
- **Domination:** The system literally cannot be crashed by user code. Ever.

#### Performance
- **Current:** EEVDF scheduler.
- **Peak target:** Per-CPU EEVDF with work-stealing, NUMA-aware, latency-prioritised for interactive tasks. Zero-copy IPC, zero-copy surface mapping to GPU, lock-free everything possible.
- **Domination:** Faster than Linux on the workloads that matter to power users.

#### Storage
- **Current:** SimpleFS on NVMe (polling).
- **Peak target:** RogueFS — log-structured, checksummed (BLAKE3), capability-addressed, crash-consistent, copy-on-write. Single-level store: files are just capability-mapped regions.
- **Domination:** Faster and safer than ZFS + btrfs combined, with zero syscall tax.

#### Networking
- **Peak target:** Full TCP/IP + QUIC + TLS 1.3 in-kernel (Rust), capability-controlled sockets, sandboxed network namespaces by default.
- **Domination:** No more userland network stack compromises.

---

### Desktop & User Experience — roguewm

roguewm is the only allowed desktop environment. Philosophy: *"If it needs a mouse for normal use, it is broken."*

**Implementation:** 100% Rust, zero dependencies, ~3k LOC target. Uses the kernel compositor directly via surface capabilities.

**Features (deliberately minimal):**

- 9 tags (workspaces) with dynamic tiling — master-stack, grid, monocle
- Keyboard-only operation (exactly like dwm: `MOD+1-9`, `MOD+Enter` spawns terminal, `MOD+Shift+c` kills client, `MOD+Space` toggles monocle, etc.)
- No title bars by default — tiny border only, colour-coded by capability level or process group
- No animations, no blur, no transparency — nothing that costs latency
- Perfect integration with Cogman: every window has a capability handle; roguewm can ask Cogman to restart a crashed client instantly
- Global keybindings for pentesting tools — one key to spawn burp, gdb, wireshark-in-sandbox, etc.
- Status bar is a tiny, scriptable IPC client that shows only what you tell it: CPU, memory pressure, active capabilities, audit log summary

This is not for "enjoying" the OS. It is for shipping code, finding bugs, and owning the machine at 200 WPM. Casual users can use it later via optional "simple mode" overlays — never the other way around.

---

### Phased Evolution Roadmap

| Phase | Name | Difficulty | Goal |
|-------|------|-----------|------|
| 1 | Memory | Hard | Per-process page tables, guard pages, `#PF` → kill process, `SYS_MMAP`/`MUNMAP` |
| 2 | Capability Kernel | Hard | `u128` capability tokens, grant/revoke, `SYS_SANDBOX`, capability-mapped objects |
| 3 | Storage Revolution | Medium | RogueFS + full NVMe MSI-X + journaled single-level store |
| 4 | roguewm | Medium | Full Rust dwm clone with kernel surface protocol v2 |
| 5 | SMP & Scheduler God Mode | Hard | Per-CPU runqueues, work-stealing, NUMA |
| 6 | Networking & Sandboxed Services | Medium | TCP/QUIC + capability sockets |
| 7 | Toolchain & Package System | Low | rogue-pkg + reproducible `.kpkg` + official dev/pentest tool repository |
| 8 | Formal Verification Layer | Hard | Rust + Prusti + seL4-style invariants for critical paths |
| 9 | Self-hosting & Dogfooding | Low | Compile RogueOS on RogueOS, daily driver for kernel team |
| 10 | Audit & Release | Low | Public capability audit, reproducible builds, "RogueOS 1.0 — Untouchable" |

---

## Licence

RogueOS is **free for individuals, students, researchers, non-profits, and
small businesses** (annual revenue under USD $1M).

Enterprise use, commercial products, embedded/OEM, and SaaS deployments
require a commercial licence.

See [LICENSE](LICENSE) for the full terms and
[COMMERCIAL.md](COMMERCIAL.md) for pricing guidance and how to get in touch.
