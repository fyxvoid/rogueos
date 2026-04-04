### Kingdom OS — Build & Lint Status (dev host)

#### Workspace `cargo build`

- **Command:** `cargo build`
- **Status:** ❌ Fails
- **Primary issues:**
  - **Userland bins (`shell`, `viewer`, `copy`)**: host linker error `duplicate symbol: _start`.
    - Cause: host CRT object `Scrt1.o` defines `_start`, and each no_std userland bin also defines its own `_start` while linking with `userland/linker.ld`.
    - Impact: host-target `cargo build` for the whole workspace is not supported; userland binaries are intended to be built via `scripts/build_os.sh` / UEFI image, not run as host processes.

#### `cargo test -p kernel`

- **Status:** ❌ Fails
- **Issue:** `duplicate lang item \`panic_impl\`` between `kernel` and `std` (which `test` depends on).
  - Kernel is a `no_std` bare-metal OS crate with its own `#[panic_handler]`.
  - The host test harness links `std`, which also defines `panic_impl`, causing a conflict.
  - Conclusion: host `cargo test` for `kernel` is not currently supported; testing is done via QEMU/UEFI boot runs instead.

#### `cargo build -p userland --lib`

- **Status:** ✅ Passes
- **Notes:**
  - `userland/src/lib.rs` is `no_std` and now gates its `#[panic_handler]` with `#[cfg(not(test))]`, avoiding conflicts when linked from test code.

#### `cargo clippy -p userland`

- **Status:** ✅ (no denied lints)
- **Notes:**
  - Fixed all `clippy::unnecessary_cast` errors in syscall wrappers by removing redundant `as u64` casts from `SYS_*` constants.
  - Remaining lints are **warnings** in individual bins (`monitor`, `shell`, `wm`) about style (e.g. `needless_range_loop`, `manual_range_contains`, `manual_is_multiple_of`); they do not break the build.

#### `cargo clippy -p kernel`

- **Status:** ⚠️ Warnings only
- **Notes:**
  - `kernel/build.rs`: multiple `clippy::needless_borrows_for_generic_args` warnings for `std::fs::write(&path, &[])`. (Harmless; can be cleaned up later.)
  - `kernel/memory/physical/buddy.rs`: logging-only variables are now gated under `#[cfg(not(test))]`, removing prior unused-variable warnings in test builds.

