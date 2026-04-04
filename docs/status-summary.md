### Kingdom OS — Status vs Modern Requirements

#### Build & tooling

- **Kernel & userland**
  - `./scripts/build_os.sh` builds userland (init, shell, wm, etc.) and the kernel for `x86_64-unknown-none` successfully.
  - UEFI bootloader builds successfully and produces a bootable tree in `build/uefi-boot/`.
- **Host cargo workflow**
  - `cargo build` for the entire workspace is not meaningful for userland/boot:
    - Userland bins are bare-metal ELFs with their own `_start`; linking them as host binaries causes CRT `_start` conflicts.
    - `cargo test -p kernel` is not supported due to conflicting `panic_impl` between kernel’s `no_std` handler and `std`.

#### Runtime validation (QEMU, UEFI)

- **Boot path (latest run, `serial-plan.log`)**
  - Bootloader:
    - `[BOOT] uefi_entry` and kernel ELF entry logged from `boot/src/main.rs`.
  - Kernel:
    - `[KRN] kernel_main_entry`
    - `[physical] conventional_region start=0x1780000 pages=0x10000`
    - `[KRN] physical_init_from_bootinfo_ok`
    - `[KRN] paging_init_start`
    - `[KRN] switched to kernel CR3`
    - `[KRN] identity_map_range done ok=1`
    - `[physical] build_freelist_start ...` with valid RSP and stack bounds
    - `debug_walk` confirms first frame PTE is Present+Writable with PA==VA.
  - System continues to service timer interrupts (INT 0x20) according to `qemu-plan.log`; no immediate invalid opcode or page fault is observed at boot in this run.

#### Paging

- **Current model**
  - Kernel has its own PML4 and identity-maps:
    - 8 MiB around the kernel image.
    - A large “frame region” chosen from UEFI conventional memory.
  - Buddy allocator is tied to this frame region; invariants are enforced with:
    - Stack-bound checks at `build_initial_freelist` entry.
    - Canary write/read and `push_free` bounds checks for the first free block.
  - User processes share the kernel CR3; ELF loader and process creation enforce:
    - Entry PTE is `PRESENT|USER|EXEC`, not writable.
    - Data PTE is `PRESENT|USER|WRITABLE|NX`.
    - First 16 bytes at text entry match the ELF file.
- **Gaps vs modern OS**
  - No per-process address spaces (single CR3).
  - No ASLR or guard pages.
  - Buddy assumes identity-mapped frame region.
  - Evolution path is spelled out in `docs/design-paging-evolution.md`.

#### Multithreading / SMP

- **Current model**
  - Single-core (BSP-only) execution; no AP startup code runs kernel on additional cores.
  - Single global runqueue with two priority buckets (`runqueue.rs`), scheduling whole processes with one kernel stack each.
  - Timer interrupts (INT 0x20) drive round-robin scheduling, but there is no thread abstraction distinct from processes.
- **Gaps vs modern OS**
  - No SMP support (no per-CPU data, no per-CPU runqueues).
  - No threads; only processes with one kernel stack and trap frame.
  - A staged SMP/threading roadmap is documented in `docs/design-smp-and-threads.md`.

#### Display server, compositor, WM

- **Current pipeline**
  - Kernel:
    - `drivers::framebuffer` owns the GOP framebuffer, providing `clear`, `fill_rect`, `blit`, and `flush`.
    - `display::display_server` implements a simple surface API on top, but this API is currently **kernel-internal**.
  - Userland:
    - WM (`userland/src/bin/wm.rs`) draws directly into the framebuffer via syscalls:
      - `sys_fb_clear`, `sys_fb_fill_rect`, `sys_fb_flush`.
    - WM handles focus and window decorations inside a single process.
- **Gaps vs modern OS**
  - No user-visible surface protocol (no Wayland/X11-style compositing).
  - No multi-client composition; WM and apps are not separated at the kernel level.
  - No damage tracking, vsync-awareness, or double-buffering.
  - Evolution path is described in `docs/design-display-evolution.md`.

#### Overall readiness

- **Boots reliably under QEMU/UEFI** with the current kernel image and userland, reaching early kernel initialization and paging setup.
- **Paging and buddy invariants are instrumented and hold** in the current run (stack bounds, first frame mapping, canary path up to the first `push_free`).
- **Modern requirements status (high level)**
  - Paging: **Partially modern** (NX, user/kernel split at PTE level, large-page identity mapping) but missing per-process address spaces and ASLR.
  - Multithreading/SMP: **Single-core only**, no SMP; design roadmap prepared.
  - Display/compositor/WM: **Single-client, single-framebuffer model**, toy WM; evolution plan to introduce surfaces and multi-client composition.

