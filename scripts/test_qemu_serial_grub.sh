#!/bin/sh
# QEMU integration test for GRUB (Multiboot2) boot path.
# Build GRUB ISO (unless SKIP_BUILD=1), run QEMU with serial to file, assert on kernel markers.
# Exit 0 if all pass, 1 otherwise. No OVMF; uses SeaBIOS.
set -e
cd "$(dirname "$0")/.."
ROOT="$PWD"

BUILD_DIR="${BUILD_DIR:-$ROOT/build}"
ISO="${ISO:-$BUILD_DIR/grub.iso}"
SERIAL_LOG="${SERIAL_LOG:-$BUILD_DIR/serial_grub.log}"
TIMEOUT="${TIMEOUT:-25}"

if [ -z "$SKIP_BUILD" ]; then
  echo "Building GRUB ISO..."
  ./scripts/build_grub_iso.sh
fi
if [ ! -f "$ISO" ]; then
  echo "Missing $ISO. Run ./scripts/build_grub_iso.sh"
  exit 1
fi

mkdir -p "$(dirname "$SERIAL_LOG")"
: > "$SERIAL_LOG"

(
  exec qemu-system-x86_64 \
    -m 512 \
    -cdrom "$ISO" \
    -serial "file:$SERIAL_LOG" \
    -display none \
    -no-reboot
) &
QEMU_PID=$!
sleep "$TIMEOUT"
kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true

FAIL=0
check() {
  if grep -q "$1" "$SERIAL_LOG" 2>/dev/null; then
    echo "  OK: $2"
  else
    echo "  FAIL: $2 (missing: $1)"
    FAIL=1
  fi
}

echo "Checking GRUB boot serial output in $SERIAL_LOG..."
check "\[KRN\] kernel_main_entry" "Kernel main reached (Multiboot2 path)"
check "\[KRN\] step1: tty_ready" "TTY ready"
check "\[KRN\] step2: heap_ready" "Heap ready"
check "\[KRN\] step7: user_init_spawn" "User init spawn"

if [ "$FAIL" -eq 0 ]; then
  echo "GRUB serial tests passed."
  exit 0
fi
echo "GRUB serial tests failed."
exit 1
