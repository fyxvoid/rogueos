# Validation and run guide

## Run complete OS

From repo root:

- **UEFI (Gatehouse)**: `./scripts/build_os.sh && ./scripts/run_qemu.sh`  
  Serial and display: serial to terminal, QEMU window. Exit: Ctrl-A then X.
- **GRUB (Multiboot2)**: `./scripts/build_grub_iso.sh && ./scripts/run_qemu_grub.sh`  
  Serial to terminal; optional `QEMU_DEBUG_INT=1` for interrupt logging. Exit: Ctrl-A then X.

Use `SKIP_BUILD=1` when re-running (e.g. `SKIP_BUILD=1 ./scripts/run_qemu.sh`) to skip rebuild.

**Debug**: Serial output and (when enabled) QEMU debug log are the primary sources for both boot paths.

## Full test suite

- **Rust only**: `./scripts/test_all_rust.sh` (libs, kernel, rwm-core).
- **UEFI boot + serial**: `./scripts/test_qemu_serial.sh`.
- **GRUB boot + serial**: `./scripts/test_qemu_serial_grub.sh`.
- **Runtime validation**: `./buildhall/validate_runtime.sh` (UEFI; optional GRUB profile).
- **Production one-shot**: `./scripts/test_production.sh` (runs all of the above).

---

## Daily driver validation checklist (plan Section 10)

- [ ] **12-hour continuous runtime**: Automated or manual; no panic, no hang.
- [ ] **Heavy terminal use**: Many commands, multiple shells, editor usage; no crash.
- [ ] **No compositor crashes**: WM and display path stable.
- [ ] **No kernel panic**: Under normal and stress use.
- [ ] **No data loss**: Files survive reboot; fsync used where required.
- [ ] **Clean reboot**: Reboot and shutdown utilities work; FS flushed.
- [ ] **Stable memory footprint**: No monotonic growth over 12 h.
- [ ] **Predictable performance**: Boot and key operations within baseline.

## Production validation (manual sign-off)

For production sign-off, complete the daily driver checklist above and ensure all automated stages pass:

- `./scripts/test_production.sh` — runs verify_build, Rust tests, UEFI serial test, GRUB serial test, and (by default) validate_runtime (UEFI). Set `VALIDATE_BOTH=1` to also run GRUB runtime validation. Set `RUN_BOOT_STRESS=1` to include a short boot stress run.

---

## Failure response (Section 11)

On any failure:

1. Kernel state (stack, registers) is dumped to serial and optionally drawn on the panic/fault screen.
2. Process table is dumped (diagnostic + process module).
3. Allocator state is dumped.
4. Scheduler queue is dumped.
5. Framebuffer state (base, size, mode) is dumped to serial in `diagnostic_halt`.

No silent hang: every panic/fault path either logs and halts or logs and reboots.
