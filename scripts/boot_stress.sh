#!/bin/sh
# 100-boot validation in QEMU. Deterministic pass/fail via serial markers.
set -e

cd "$(dirname "$0")/.."
ROOT="$PWD"

ITER="${ITER:-100}"
TIMEOUT="${TIMEOUT:-30}"
BUILD_DIR="${BUILD_DIR:-$ROOT/build/uefi-boot}"
OUT_DIR="${OUT_DIR:-$ROOT/build/boot_stress}"
CPU="${CPU:-qemu64}"

if [ -z "$SKIP_BUILD" ]; then
  ./scripts/build_os.sh
fi

OVMF="${OVMF_CODE:-}"
if [ -z "$OVMF" ]; then
  for p in /usr/share/edk2/x64/OVMF_CODE.4m.fd /usr/share/edk2/ovmf/x64/OVMF_CODE.fd /usr/share/OVMF/OVMF_CODE.fd; do
    [ -f "$p" ] && OVMF="$p" && break
  done
fi
if [ ! -f "$OVMF" ]; then
  echo "OVMF not found. Install edk2-ovmf or set OVMF_CODE."
  exit 1
fi

mkdir -p "$OUT_DIR"

find_line() {
  # prints first line number for pattern, or empty
  grep -n "$1" "$2" 2>/dev/null | head -n 1 | cut -d: -f1
}

require_in_order() {
  _log="$1"
  shift
  _prev=0
  for pat in "$@"; do
    ln="$(find_line "$pat" "$_log")"
    if [ -z "$ln" ]; then
      echo "FAIL: missing marker: $pat"
      return 1
    fi
    if [ "$ln" -le "$_prev" ]; then
      echo "FAIL: out-of-order marker: $pat (line $ln <= $_prev)"
      return 1
    fi
    _prev="$ln"
  done
  return 0
}

i=1
while [ "$i" -le "$ITER" ]; do
  log="$OUT_DIR/serial_$(printf "%03d" "$i").log"
  : > "$log"

  (
    exec qemu-system-x86_64 \
      -m 256 \
      -cpu "$CPU" \
      -drive "if=pflash,format=raw,readonly=on,file=$OVMF" \
      -drive "file=fat:rw:$BUILD_DIR,format=raw,media=disk" \
      -serial "file:$log" \
      -display none \
      -no-reboot
  ) &
  pid=$!
  sleep "$TIMEOUT"
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true

  if grep -q "\\[KERNEL PANIC\\]" "$log" 2>/dev/null || grep -q "\\[DIAG\\] HALT" "$log" 2>/dev/null; then
    echo "FAIL: panic/diagnostic halt detected on boot $i (see $log)"
    exit 1
  fi

  # Required markers in strict order.
  if ! require_in_order "$log" \
    "\\[BOOT\\] uefi_entry" \
    "Exiting boot services" \
    "\\[KRN\\] paging_init_done" \
    "\\[KRN\\] kernel_main_entry" \
    "\\[KRN\\] heap_ready" \
    "\\[NVMe\\] " \
    "\\[KRN\\] step7: user_init_spawn"; then
    echo "FAIL: marker check failed on boot $i (see $log)"
    exit 1
  fi

  echo "OK: boot $i/$ITER"
  i=$((i + 1))
done

echo "PASS: $ITER/$ITER boots"

