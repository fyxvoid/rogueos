# RogueII / RogueOS — Full Development Roadmap

> A clean-room, Rust-only operating system targeting top-tier performance,  
> deterministic behaviour, and strong security — with no Linux ABI baggage.

---

## Vision

**kernel → cogman → compositor → apps**

RogueOS is built on three axioms:
1. **Rust everywhere** — kernel, bootloader, drivers, userland. Zero C. Zero glibc.
2. **Spawn, never fork** — clean process semantics, no COW shadow state.
3. **Capability first** — no ambient authority; every action requires an explicit capability token.

---

## Phase 0 — Foundation (current state)

| Area | Status |
|------|--------|
| x86_64 UEFI boot | ✅ working (OVMF + QEMU q35) |
| IDT / GDT / TSS | ✅ complete |
| Physical frame allocator | ✅ buddy-like from UEFI memory map |
| Paging (identity + kernel) | ✅ with TLB flush |
| Kernel heap (slab) | ✅ |
| PS/2 keyboard | ✅ |
| Serial debug | ✅ |
| AMD SME (memory encryption) | ✅ enabled by default |
| EEVDF scheduler (single-core) | ✅ |
| Spawn-only process model | ✅ |
| ELF loader | ✅ |
| IPC (64-byte RwmMsg, kernel queue) | ✅ |
| Framebuffer / GOP | ✅ |
| NVMe driver (basic) | ✅ |
| Simple FS (VFS + flat file table) | ✅ |
| Hardware PMU (AMD) | ✅ RDPMC ring-3 passthrough |
| Hardware breakpoints (DR0-3) | ✅ |
| Display server + compositor stub | ✅ |
| cogman init supervisor | ✅ (this release) |

---

## Phase 1 — Process Model Hardening

**Goal:** per-process address spaces, threads, TLS. No shared kernel mappings leaking to userland.

### 1.1 Per-process page tables
- Each process gets its own CR3 (currently shared identity map).
- Kernel mapped at high VA (0xFFFF_8000_0000_0000+) in every process; user code at low VA.
- `SYS_MMAP` / `SYS_MUNMAP` for dynamic user memory.
- Guard pages below every user stack; #PF kills the process cleanly.

### 1.2 Threads
- New syscall: `SYS_THREAD_SPAWN(fn_ptr, stack_ptr, arg) → tid`
- Thread shares CR3 with parent process; separate kernel stack.
- PIDs stay per-process; TIDs are per-thread (TID = PID<<16 | thread_slot).
- Join: `SYS_THREAD_JOIN(tid, status_ptr)`.

### 1.3 Thread-local storage (TLS)
- Allocate TLS block per thread; FS segment points to it.
- `SYS_SET_TLS(base)` — sets FS.base via MSR 0xC000_0100.
- Enables Rust's `thread_local!` macro in userland.

### 1.4 Futex
- `SYS_FUTEX_WAIT(addr, expected) → 0 / SYSERR_AGAIN`
- `SYS_FUTEX_WAKE(addr, count) → woken_count`
- Powers `Mutex`, `Condvar`, `RwLock` in the rogueos std library (Phase 9).

### 1.5 Yield
- `SYS_YIELD` — voluntarily give up remaining time slice.
- EEVDF accounts for this as a voluntary context switch (lower vruntime penalty).

---

## Phase 2 — Storage & Filesystem

**Goal:** robust, journaled filesystem; full POSIX-like VFS path ops.

### 2.1 NVMe driver completion
- Completion queue interrupt (MSI-X); remove polling fallback.
- Queue depth ≥ 32; multiple submission queues (one per CPU in SMP).
- Power management: NVM Express power states 1–4.

### 2.2 RogueFS (KFS)
- Log-structured filesystem with a circular journal (inspired by F2FS concept, original impl).
- Block size: 4 KiB; extents; inline small files (< 128 B in inode).
- Checksums on every metadata block (BLAKE3-32).
- Atomic rename; crash-consistent via journal replay.

### 2.3 VFS layer completion
- `SYS_MKDIR(path, mode)`, `SYS_RMDIR(path)`
- `SYS_STAT(path, out_stat)` — returns size, type, timestamps
- `SYS_READDIR(fd, out_dirent, count)`
- `SYS_RENAME(src, dst)`
- Symlinks (SYS_SYMLINK / SYS_READLINK)

### 2.4 Tmpfs
- In-memory filesystem backed by kernel heap.
- Mounted at `/tmp`; used for IPC sockets, runtime state.

### 2.5 Cogman package store
- KFS volume at `/pkg` — holds `.kpkg` package archives.
- `cogman pkg install <file.kpkg>` extracts and registers binaries into the program table.
- Package format: gzip-compressed tar of ELF binaries + metadata TOML.

---

## Phase 3 — Network Stack

**Goal:** full TCP/IP in Rust, zero-copy send path, TLS 1.3.

### 3.1 virtio-net driver
- MMIO virtqueue; scatter-gather TX/RX.
- Enables QEMU networking (NAT or tap).

### 3.2 TCP/IP stack (no_std)
- Ethernet → ARP → IPv4 → TCP/UDP — written from scratch, no smoltcp dependency.
- Zero-copy: TX uses guest-physical DMA descriptors directly from user buffers.
- Congestion control: CUBIC (original impl).

### 3.3 Syscall API
- `SYS_SOCK_OPEN(af, type) → fd` (af: 2=IPv4; type: 1=STREAM, 2=DGRAM)
- `SYS_SOCK_BIND(fd, addr, port)`
- `SYS_SOCK_CONNECT(fd, addr, port)`
- `SYS_SOCK_SEND(fd, buf, len, flags)` / `SYS_SOCK_RECV(fd, buf, len, flags)`
- `SYS_SOCK_CLOSE(fd)`

### 3.4 DNS stub resolver
- Hard-coded root servers; iterative resolution.
- `SYS_GETADDRINFO(name, out_ipv4)` — synchronous.

### 3.5 TLS 1.3
- Pure-Rust, no_std TLS using RustCrypto primitives (AES-GCM, ECDH P-256, SHA-256).
- Wraps SYS_SOCK_* automatically when `SYS_SOCK_OPEN` flag `SOCK_TLS` is set.

---

## Phase 4 — Security Model

**Goal:** capability-based security with no ambient authority.

### 4.1 Capability tokens
- Every process starts with a minimal token set (read own memory, IPC to parent).
- Tokens are unforgeable u128 handles minted by the kernel.
- `SYS_CAP_GRANT(target_pid, cap_id)` — delegate a capability.
- `SYS_CAP_REVOKE(cap_id)` — revoke from all holders.

### 4.2 File capability
- `FD_CAP` token required to open any file; granted to cogman, delegated down.
- Capability scope: path prefix (e.g., `/home` grants read of anything under `/home`).

### 4.3 IPC capability
- IPC to a specific PID requires `IPC_CAP(target_pid)`.
- Prevents arbitrary process enumeration or message injection.

### 4.4 Kernel isolation enforcement
- SMEP + SMAP enabled (currently not set); userland cannot exec or read kernel pages.
- KPTI-like isolation: kernel VA not mapped in user CR3 (except minimal syscall trampoline).

### 4.5 Process sandboxing
- `SYS_SANDBOX(policy_flags)` — irreversible; restricts process to a subset of syscalls.
- Inspired by seccomp-BPF but simpler: a 128-bit bitmask of allowed syscall categories.

---

## Phase 5 — SMP (Multi-core)

**Goal:** symmetric multiprocessing; linear throughput scaling.

### 5.1 AP boot (SIPI)
- Boot Application Processors via LAPIC SIPI sequence.
- Each AP gets its own GDT, IDT, TSS, kernel stack.
- `smp::init(num_cpus)` from BSP after Phase 1 is stable.

### 5.2 Per-CPU runqueue
- Each CPU has its own EEVDF runqueue.
- Work stealing: idle CPU checks neighbor queues (steal half).

### 5.3 Spinlock + ticket lock
- Replace `static mut` kernel globals with `SpinLock<T>` wrappers.
- Low-contention paths use seqlock (read-mostly scheduler state).

### 5.4 IPI (inter-processor interrupt)
- `IPI_RESCHEDULE` — kick a CPU to reschedule.
- `IPI_TLB_SHOOTDOWN` — used by paging when unmapping shared pages.

---

## Phase 6 — Graphics & Display

**Goal:** hardware-accelerated compositor, rich window manager.

### 6.1 virtio-gpu driver
- Virtio GPU 2D commands: `RESOURCE_CREATE_2D`, `TRANSFER_TO_HOST`, `SET_SCANOUT`.
- Enables QEMU `virtio-gpu-gl` for GPU-accelerated rendering.
- Resource zero-copy: maps GPU-side resource directly into user virtual address.

### 6.2 Surface protocol v2
- Add `SYS_SURFACE_MAP(id, out_ptr)` — map surface pixels directly to user VA (zero-copy).
- Apps write pixels to mapped VA; single `SYS_SURFACE_COMMIT` flushes to compositor.
- Damage tracking: commit carries dirty rectangle list.

### 6.3 RWM compositor upgrade
- Scene graph: z-ordered surface tree, alpha compositing, blur effects.
- Keyboard/mouse grab API.
- Multi-monitor support (multiple GOP/virtio-gpu outputs).

### 6.4 Font rendering
- Embedded bitmap font (current) + optional TrueType renderer (no_std, original impl).
- Subpixel rendering (RGB BGR) for LCD targets.

---

## Phase 7 — Performance Targets

**Goal:** measurable performance no other OS achieves at equivalent scale.

| Metric | Target | Rationale |
|--------|--------|-----------|
| Cold boot to shell prompt | < 100 ms | No legacy BIOS init, no initrd |
| IPC round-trip (same core) | < 500 ns | Lock-free ring buffer |
| Context switch latency | < 1 µs | No TLB flush on same-CR3 threads |
| Syscall overhead | < 80 ns | SYSCALL fast path, no KPTI overhead |
| NVMe 4K random read | > 800K IOPS | Interrupt-driven, queue depth 32 |
| TCP throughput (loopback) | > 10 Gbps | Zero-copy DMA send |

### 7.1 Lock-free IPC ring
- Replace kernel-side IPC queue (Vec-backed) with a lock-free SPSC ring.
- 256-slot ring per PID; overflow returns `SYSERR_NOMEM`.
- Sender never acquires a lock; receiver drains with atomic compare-and-swap.

### 7.2 PMU telemetry daemon
- `monitor` binary (already in userland) reads PMU via `SYS_PERF_OPEN/READ`.
- Displays: cycles, IPC, L1/L2 misses, branch mispredicts, stall cycles.
- Cogman exposes `/perf` IPC channel for performance queries.

### 7.3 Batch syscalls
- `SYS_BATCH(ring_ptr, count)` — submit up to 64 syscall descriptors atomically.
- Amortizes context-switch overhead for I/O-heavy userland.

---

## Phase 8 — Developer Toolchain

**Goal:** build RogueOS apps natively on RogueOS.

### 8.1 rogueos-std
- Implement Rust `std` for the `x86_64-unknown-rogueos` target.
- Backed by RogueOS syscalls: `alloc` via `SYS_MMAP`, `thread` via `SYS_THREAD_SPAWN`, etc.
- Distribute as a pre-compiled sysroot embedded in the OS image.

### 8.2 Custom Rust target (`x86_64-unknown-rogueos`)
- Target spec JSON: `os = "rogueos"`, `env = ""`, `abi = "sysv64"`.
- Merged into rust-lang/rust upstream (long-term goal).

### 8.3 kargo — package manager
- Port of cogman-planner concept to rogueos-native execution.
- `.kpkg` registry at a known URL (network via Phase 3).
- Build recipes in TOML; deterministic builds via hash-locked deps.
- `kargo build`, `kargo install`, `kargo run`.

### 8.4 Cogman AI assistant (on-device)
- Port `cogman-advisor` from rogue-linux to rogueos userland.
- Model: 1-3B parameter, 4-bit quantized — fits in 2 GB RAM.
- Inference via rogueos-native matrix kernel (AVX2 GEMM, no_std).
- `cogman ask "why is session crashing?"` → advisor reads IPC logs, suggests fix.

---

## Phase 9 — Formal Verification & Hardening

**Goal:** provably correct critical subsystems.

### 9.1 Memory allocator verification (Kani)
- Prove buddy allocator never double-frees or leaks frames.
- Property: `alloc(n); free(n)` leaves state identical to before alloc.

### 9.2 Scheduler proof (Prusti)
- Prove EEVDF scheduler terminates (no livelock) and respects nice-level ordering.

### 9.3 Fuzzing
- LibAFL-based syscall fuzzer: generates random syscall sequences, checks for kernel panics.
- Coverage-guided; runs in QEMU with KVM.

### 9.4 Cryptographic hardening
- All kernel random numbers from RDRAND + RDSEED (already present via x86 perf module).
- ASLR: randomise base of every process ELF load.
- Stack canaries: compiler-enforced (`-Z stack-protector=strong`).

---

## Phase 10 — Production Targets

### 10.1 Hardware targets beyond QEMU
- Bare-metal x86_64: Intel NUC, AMD Ryzen mini-PC.
- ACPI parser (DSDT traversal) for PCI enumeration beyond hardcoded BAR.
- USB HID driver completion (currently stub) for real keyboards/mice.

### 10.2 ARM64 (AArch64) port
- Second architecture target: `aarch64-unknown-none`.
- Raspberry Pi 4 / Apple M-series (via hypervisor).
- Shared `libs` crate with arch-specific syscall entry point.

### 10.3 Hypervisor guest hardening
- VirtIO balloon driver for memory hot-plug.
- Virtio-vsock for host↔guest communication.
- Sealed measured boot: TPM 2.0 attestation of kernel image hash.

### 10.4 Public release
- Open-source the kernel under a permissive license (BSL or MIT).
- Published benchmark suite: boot time, IPC latency, syscall overhead vs Linux, seL4, Redox.
- Documentation site generated from cogman-style SSG.

---

## Milestone Summary

| Milestone | Phases | Key Deliverable |
|-----------|--------|----------------|
| **M1** Alpha | 0 + 1 | Per-process address spaces, threads, cogman stable |
| **M2** Beta | 2 + 3 | KFS filesystem, TCP/IP network stack |
| **M3** Security | 4 + 5 | Capability tokens, SMP boot |
| **M4** Desktop | 6 | virtio-gpu, composited WM, font rendering |
| **M5** Perf | 7 | Lock-free IPC, < 100 ms boot, all PMU targets met |
| **M6** Toolchain | 8 | rogueos-std, kargo, on-device AI assistant |
| **M7** Verified | 9 | Kani/Prusti proofs, LibAFL fuzzer clean |
| **M8** Production | 10 | Bare-metal, ARM64, public release |

---

## Non-goals

- POSIX compatibility layer (no `libc`, no `fork`, no `/proc`)
- Linux driver compatibility (no DRM/KMS, no ALSA, no V4L2)
- Interpreted languages in the kernel (no eBPF, no Lua)
- Microkernel architecture (monolithic is the right call for performance at this scale)
