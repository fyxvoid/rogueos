# RogueOS — Current Development Stage

> Last updated: 2026-04-04

**Overall: Early Alpha / Proof of Concept** — the kernel boots, runs userland, and has real subsystems, but is still single-core, single-address-space, and lacks persistent storage.

---

## What works today

| Subsystem | Status |
|-----------|--------|
| UEFI boot | Solid — GOP framebuffer, BootInfo handoff |
| Physical memory | Working — buddy allocator, slab allocator with canary checks |
| Virtual memory | Kernel-only — paging mapped, but **no per-process address spaces** |
| Scheduler | EEVDF, single core, nice levels, preemptive (timer IRQ) |
| Process model | Spawn-by-ID, exit, waitpid, basic lifecycle |
| IPC | Per-process 32-message ring, non-blocking recv, confirmed working |
| Syscall ABI | ~30 syscalls across spawn / process / IPC / display / fs |
| Display | Framebuffer blitting, surface ownership (RDP), compositor enforcement |
| WM (rwm) | Tiling layout, keyboard input, RDP compositor integration |
| PS/2 keyboard | Working — scancode set 2, modifier tracking |
| NVMe driver | Partial — MMIO, admin + IO queues, BlockDevice trait; **not wired to VFS** |
| Filesystem | Custom flat VFS (simple_fs + vfs layer), **RAM-only, not persistent** |
| Userland programs | Shell, editor, viewer, monitor, cogman init supervisor (11 registered) |
| Cogman | PID 1 supervisor — respawns session/shell, handles halt/reboot exit codes |
| RDP client lib | `rdp.rs` — surface lifecycle, attach, commit, event polling |

---

## What's missing / broken

| Gap | Impact |
|-----|--------|
| Per-process page tables | All processes share kernel address space — no memory isolation |
| SMP / multi-core | Single core only; no spinlocks, no cross-core IPI |
| Threads + futex | No threading primitive; one execution context per process |
| NVMe ↔ VFS bridge | NVMe driver exists but filesystem never reads from disk |
| USB (xhci.rs) | Skeleton only — no USB keyboard/mouse support |
| Network stack | Nothing — no driver, no TCP/IP |
| Capability security | No per-process token system; all syscalls are ambient |
| Signal handling | No POSIX signals; cogman uses exit-code conventions instead |
| Dynamic linking | All userland is statically linked ELF, no shared libs |
| Time / RTC | No wall clock; scheduler has ticks but no `gettimeofday` |

---

## Progress by layer

```
[UEFI + Boot]     ████████░░  solid, reliable
[Memory Mgmt]     ██████░░░░  physical/slab done; VM isolation missing
[Scheduler]       ███████░░░  EEVDF works; no SMP/threads
[IPC]             ████████░░  working, used in production by wm + cogman
[Display/RDP]     ███████░░░  compositor + surface model done; no GPU accel
[Filesystem]      ████░░░░░░  VFS layer exists; no disk persistence
[Drivers]         █████░░░░░  framebuffer ✓, PS/2 ✓, NVMe partial, USB none
[Userland]        ██████░░░░  shell, wm, cogman working; no libc equiv
[Security]        ████░░░░░░  compositor enforcement done; no process isolation
[Network]         ░░░░░░░░░░  not started
```

---

## Next highest-leverage work

1. **Wire NVMe → VFS** — unlocks persistent storage; filesystem already exists, just needs a real block backend
2. **Per-process page tables** — actual memory isolation; design doc is at `docs/design-paging-evolution.md`
3. **Threads + futex** — needed before any real multi-threaded app can run; design at `docs/design-smp-and-threads.md`

Those three would move RogueOS from "interesting demo" to "real OS."

---

## Build & runtime notes

- `./scripts/build_os.sh` builds the full OS (kernel + userland) for `x86_64-unknown-none`
- `./scripts/run_qemu.sh` boots in QEMU/UEFI; serial output confirms boot to userland
- Host `cargo build` is not meaningful for userland/boot (bare-metal ELFs with custom `_start`)
- Detailed boot log analysis: see `docs/build-status.md`
- Architecture docs: `docs/arch-*.md`
- Full roadmap: `ROADMAP.md`
