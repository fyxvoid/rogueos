#!/bin/sh
# Production test suite: runs all automated checks in order.
# Reports PASS/FAIL per stage; exits non-zero if any stage fails.
# From repo root: ./scripts/test_production.sh
# Set SKIP_BUILD=1 to skip rebuilds where scripts support it.
# Set VALIDATE_BOTH=1 to run UEFI + GRUB runtime validation.
set -e
cd "$(dirname "$0")/.."
ROOT="$PWD"

FAIL=0
run_stage() {
  _name="$1"
  shift
  echo ""
  echo "=== Stage: $_name ==="
  if "$@"; then
    echo "  PASS: $_name"
  else
    echo "  FAIL: $_name"
    FAIL=1
  fi
}

echo "=== RogueOS production test suite ==="

if [ -z "$SKIP_BUILD" ]; then
  echo "=== Build (UEFI + GRUB) ==="
  "$ROOT/scripts/build_os.sh" || { echo "  FAIL: build_os.sh"; exit 1; }
  "$ROOT/scripts/build_grub_iso.sh" || { echo "  FAIL: build_grub_iso.sh"; exit 1; }
  echo "=== ESP disk (for validate_runtime) ==="
  "$ROOT/buildhall/esp_disk.sh" 2>/dev/null || true
fi

run_stage "Verify build artifacts" "$ROOT/scripts/verify_build.sh"
run_stage "Rust tests" "$ROOT/scripts/test_all_rust.sh"
run_stage "UEFI serial test" env SKIP_BUILD=1 "$ROOT/scripts/test_qemu_serial.sh"
run_stage "GRUB serial test" env SKIP_BUILD=1 "$ROOT/scripts/test_qemu_serial_grub.sh"

if [ "${RUN_VALIDATE_RUNTIME:-1}" = "1" ]; then
  if [ "$VALIDATE_BOTH" = "1" ]; then
    run_stage "Validate runtime (UEFI + GRUB)" env VALIDATE_BOTH=1 SKIP_REBUILD=1 "$ROOT/buildhall/validate_runtime.sh"
  else
    run_stage "Validate runtime (UEFI)" env SKIP_REBUILD=1 "$ROOT/buildhall/validate_runtime.sh"
  fi
fi

if [ "${RUN_BOOT_STRESS:-0}" = "1" ]; then
  run_stage "Boot stress (UEFI)" env SKIP_BUILD=1 ITER=5 "$ROOT/scripts/boot_stress.sh"
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
  echo "=== Overall: PASS ==="
  exit 0
fi
echo "=== Overall: FAIL ==="
exit 1
