### Development Notes

#### Coding standards

- **Panics vs `diagnostic_halt`**
  - Runtime kernel code should avoid `panic!` and `.unwrap()` in favor of:
    - `crate::kernel::diagnostic::diagnostic_halt("reason")` for fatal invariants.
    - Explicit `match` on `Option`/`Result` with clear error handling.
  - `panic_handler` is implemented in `[kernel/init/panic.rs]` to halt the CPU; it is not a control-flow mechanism.

- **Unsafe code**
  - Every `unsafe` block should have a `// SAFETY:` comment explaining:
    - Why pointer dereferences are valid.
    - Why aliasing/reentrancy is not a problem (single-core assumption, ownership model).
  - Avoid spreading `unsafe` across large scopes; keep it as small and local as possible.

- **Logging**
  - Use consistent prefixes for serial output:
    - `[KRN]` — high-level kernel messages.
    - `[PF]` — page faults and paging diagnostics.
    - `[sched]` — scheduler/runqueue messages.
    - `[physical]` — buddy/frame allocator.
    - `[INIT]`, `[WM]`, `[DIR]` — userland/init, window manager, director.
  - Debug-only logs should be gated with `#[cfg(not(test))]` where they pull in non-test-only helpers.

#### Tests and host tooling

- **Kernel**
  - `cargo test -p kernel` is **not** supported on the host:
    - Kernel is `no_std` with its own `panic_handler`, which conflicts with `std`’s `panic_impl` in the test harness.
  - Testing strategy is via:
    - QEMU + UEFI boot (`scripts/run_qemu.sh`, `make run`).
    - Serial logs and QEMU `-d int,cpu_reset,guest_errors` traces.

- **Userland**
  - `userland` is `no_std` with a `#[panic_handler]` gated under `#[cfg(not(test))]`, allowing it to be linked from test code if desired.
  - `cargo build -p userland --lib` is supported; top-level `cargo build` for the workspace is not meaningful for userland bins, because they are bare-metal ELF images with their own `_start`.

#### Build and images

- **Standard workflow**
  - `./scripts/build_os.sh`:
    - Builds userland binaries (release, `x86_64-unknown-none`).
    - Builds the kernel (release, `x86_64-unknown-none`).
    - Produces a UEFI-bootable tree in `build/uefi-boot/`.
  - For testing without `sudo`, QEMU can mount `build/uefi-boot` as a FAT drive.

- **ESP disk vs live `build/uefi-boot`**
  - `build/esp_disk.img` is a prebuilt FAT disk used by some validation scripts; it is updated via `buildhall/esp_disk.sh` (requires `sudo`).
  - To ensure you are running the **current kernel** without touching the ESP:
    - Run QEMU with `-drive file=fat:rw:build/uefi-boot,if=virtio,format=raw`.

