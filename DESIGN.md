# System Design Overview

This operating system stack is implemented as an independent, clean-room design for proprietary licensing compliance. No code, structure, or naming is copied from Linux, BSD, or other GPL systems.

## Principles

- **Spawn-only process model**: No fork or clone. Process creation is via spawn-by-program-id only.
- **Original ABIs**: Syscall numbering and error codes are project-defined. No replication of external syscall tables or ABIs.
- **Original subsystem layout**: Process, memory, filesystem, and driver modules use project-specific types and structure. Each subsystem has a DESIGN.md describing concept-level origin and independent implementation.

## Subsystem Documentation

- [kernel/memory/DESIGN.md](kernel/memory/DESIGN.md) — Physical, paging, virtual, heap, debug
- [kernel/process/DESIGN.md](kernel/process/DESIGN.md) — Process, pid, scheduler, context, loader, lifecycle, debug
- [kernel/fs/DESIGN.md](kernel/fs/DESIGN.md) — Volume header, file record table, VFS
- [kernel/drivers/DESIGN.md](kernel/drivers/DESIGN.md) — Traits and hardware-spec-based drivers
- [kernel/syscall/DESIGN.md](kernel/syscall/DESIGN.md) — Syscall layer and ABI
- [kernel/display/DESIGN.md](kernel/display/DESIGN.md) — Display server
- [userland/DESIGN.md](userland/DESIGN.md) — Rust userland, shell, utilities

All DESIGN documents describe algorithm origin at concept level only and state that implementation is independent. No references to external source paths or structural mapping to GPL internals.
