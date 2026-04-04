# RogueOS — Documentation Index

---

## Reference

| Document | Description |
|----------|-------------|
| [syscall-abi.md](syscall-abi.md) | Complete syscall table: namespaces, arguments, return values, error codes |
| [ipc.md](ipc.md) | IPC message format, all RwmMsg types, payload layouts, usage examples |
| [cogman.md](cogman.md) | Cogman init supervisor: service table, restart policies, control protocol |

## Architecture

| Document | Description |
|----------|-------------|
| [arch-paging.md](arch-paging.md) | Memory model: physical allocator, paging, address space layout |
| [arch-scheduling.md](arch-scheduling.md) | Process model: EEVDF scheduler, runqueue, context switch |
| [arch-display-stack.md](arch-display-stack.md) | Display server: surface protocol, compositor, framebuffer |
| [userland-display-host.md](userland-display-host.md) | Host display stack: X11 + dwm-rs, running on Linux for development |

## Design Roadmaps

| Document | Description |
|----------|-------------|
| [design-paging-evolution.md](design-paging-evolution.md) | Per-process address spaces, stronger kernel/user separation |
| [design-smp-and-threads.md](design-smp-and-threads.md) | SMP bring-up and kernel thread abstraction |
| [design-display-evolution.md](design-display-evolution.md) | Richer compositor and surface model |
| [design-rogue-desktop-integration.md](design-rogue-desktop-integration.md) | Cogman + display server integration design |

## Development

| Document | Description |
|----------|-------------|
| [dev-notes.md](dev-notes.md) | Coding standards, logging conventions, panic/diagnostic_halt policy |
| [build-status.md](build-status.md) | Current build and clippy status across all crates |
| [status-summary.md](status-summary.md) | Feature completion checklist |

---

## Quick Links

- **Main README:** [../README.md](../README.md) — overview, build instructions, boot sequence
- **Full Roadmap:** [../ROADMAP.md](../ROADMAP.md) — 10-phase development plan
- **Shared ABI source:** [../lib/src/lib.rs](../lib/src/lib.rs) — all syscall numbers, RwmMsg, BootInfo
- **Cogman source:** [../userland/src/bin/cogman.rs](../userland/src/bin/cogman.rs)
