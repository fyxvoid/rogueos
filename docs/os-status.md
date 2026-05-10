# RogueOS — Current State

## What Actually Works

### Boot & Core Kernel
- UEFI boot via custom `boot.efi` (OVMF + FAT ESP image)
- Multiboot2 boot via GRUB ISO (alternative path)
- x86_64 GDT, IDT, TSS, syscall entry (`SYSCALL`/`SYSRET`)
- Serial output (COM1 → stdio in QEMU)
- Physical memory: buddy allocator over UEFI memory map
- Virtual memory: 4-level page table mapper, TLB flush
- Heap: slab allocator + kmalloc
- Process: EEVDF scheduler, lifecycle (spawn/exit/wait), ELF loader
- IPC: fixed-size 64-byte message ring (KwmMsg protocol)
- Syscall dispatch: full table wired (fs, gfx, process, IPC, misc)
- PS2 keyboard driver (scancode → KeyEvent)

### What Exists But Is Unverified / Stub
- Framebuffer driver (`drivers/framebuffer.rs`) — code complete, pixel output to QEMU GTK window not confirmed end-to-end from userland
- Compositor (`kernel/display/compositor/`) — stub, no real compositing
- Display server (`kernel/display/display_server/`) — stub
- NVMe driver — code exists, untested on real hardware
- USB/xHCI driver — partial, not integrated
- Mouse input (`POLL_MOUSE`) — PS2 mouse handler exists, routing to userland unconfirmed
- Simple filesystem — reads/writes work in theory, not stress tested

### Userland (Code Exists, Untested End-to-End)
- `shell` binary — basic interactive shell
- `wm` binary (1375 lines) — WM logic written
- `rogue_ds` binary (819 lines) — display server written
- `rwm-core` crate — tiling layout engine (522 lines, has unit tests)
- `rwm-config` crate — key bindings + config
- `rwm-desktop` crate — Linux host runner for dev/testing
- `cogman` binary — AI advisor stub (TODOs throughout)
- `apps/rogue-{lock,shot,clip}` — written, untested on OS

## Where We Are in the Stack

```
[UEFI boot]        ✓ works
[Kernel init]      ✓ works
[Memory]           ✓ works
[Scheduler]        ✓ works (EEVDF)
[Syscalls]         ✓ wired up
[Framebuffer]      ~ driver written, not confirmed from userland
[Input]            ~ keyboard driver exists, end-to-end unconfirmed
[Display server]   ✗ stub
[Compositor]       ✗ stub
[Window manager]   ~ logic written, not running on OS yet
[Apps]             ✗ not running on OS
```

## One-Line Summary

The kernel boots, schedules, allocates memory, and handles syscalls. The GUI stack is written but not connected — framebuffer → compositor → WM → app pipeline needs to be built and verified step by step.
