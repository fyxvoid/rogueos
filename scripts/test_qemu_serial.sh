#!/bin/sh
# QEMU integration test: build (unless SKIP_BUILD=1), run QEMU with serial to file,
# timeout, then assert on expected lines. Exit 0 if all pass, 1 otherwise.
# Requires: qemu-system-x86_64, OVMF (edk2-ovmf).
set -e
cd "$(dirname "$0")/.."
ROOT="$PWD"

# #region agent log (ndjson)
LOG_PATH="$ROOT/.cursor/debug.log"
RUN_ID="${RUN_ID:-pre}"
ts_ms() { echo $(( $(date +%s) * 1000 )); }
log_ndjson() {
  # Best-effort logging; never fail the test because of logging
  _ts="$(ts_ms)"
  _hyp="$1"
  _loc="$2"
  _msg="$3"
  _data="$4"
  printf '%s\n' "{\"id\":\"test_qemu_serial_${_ts}\",\"timestamp\":${_ts},\"runId\":\"${RUN_ID}\",\"hypothesisId\":\"${_hyp}\",\"location\":\"${_loc}\",\"message\":\"${_msg}\",\"data\":${_data}}" >> "$LOG_PATH" 2>/dev/null || true
}
# #endregion agent log (ndjson)

BUILD_DIR="${BUILD_DIR:-$ROOT/build/uefi-boot}"
SERIAL_LOG="${SERIAL_LOG:-$ROOT/build/serial.log}"
TIMEOUT="${TIMEOUT:-15}"
QEMU_DEBUG="${QEMU_DEBUG:-0}"
QEMU_DEBUG_LOG="${QEMU_DEBUG_LOG:-$ROOT/build/qemu_debug.log}"
QEMU_DEBUG_ARGS=""

log_ndjson "A" "scripts/test_qemu_serial.sh:setup" "starting qemu serial test" "{\"SKIP_BUILD\":\"${SKIP_BUILD}\",\"BUILD_DIR\":\"${BUILD_DIR}\",\"SERIAL_LOG\":\"${SERIAL_LOG}\",\"TIMEOUT\":\"${TIMEOUT}\"}"

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
  log_ndjson "C" "scripts/test_qemu_serial.sh:ovmf" "ovmf not found" "{\"OVMF_CODE\":\"${OVMF_CODE}\"}"
  exit 1
fi
log_ndjson "C" "scripts/test_qemu_serial.sh:ovmf" "using ovmf" "{\"OVMF\":\"${OVMF}\"}"

mkdir -p "$(dirname "$SERIAL_LOG")"
: > "$SERIAL_LOG"
if [ "$QEMU_DEBUG" = "1" ]; then
  : > "$QEMU_DEBUG_LOG"
  log_ndjson "D" "scripts/test_qemu_serial.sh:qemu" "qemu debug enabled" "{\"QEMU_DEBUG_LOG\":\"${QEMU_DEBUG_LOG}\"}"
  QEMU_DEBUG_ARGS="-d int,cpu_reset,guest_errors -D $QEMU_DEBUG_LOG"
fi

# Run QEMU with serial to file; timeout then kill
(
  exec qemu-system-x86_64 \
    -m 128 \
    -drive "if=pflash,format=raw,readonly=on,file=$OVMF" \
    -drive "file=fat:rw:$BUILD_DIR,format=raw,media=disk" \
    -serial "file:$SERIAL_LOG" \
    $QEMU_DEBUG_ARGS \
    -display none \
    -no-reboot
) &
QEMU_PID=$!
sleep "$TIMEOUT"
kill "$QEMU_PID" 2>/dev/null || true
wait "$QEMU_PID" 2>/dev/null || true

SERIAL_BYTES="$(wc -c < "$SERIAL_LOG" 2>/dev/null || echo 0)"
HAS_BOOT_LOAD="$(grep -q "custom_kernel boot: loading kernel" "$SERIAL_LOG" 2>/dev/null && echo 1 || echo 0)"
HAS_EXIT_BS="$(grep -q "Exiting boot services" "$SERIAL_LOG" 2>/dev/null && echo 1 || echo 0)"
HAS_KERNEL_MAIN="$(grep -q "\[KRN\] kernel_main_entry" "$SERIAL_LOG" 2>/dev/null && echo 1 || echo 0)"
HAS_TTY="$(grep -q "TTY ready" "$SERIAL_LOG" 2>/dev/null && echo 1 || echo 0)"
HAS_SHELL="$(grep -q "Starting shell in userspace" "$SERIAL_LOG" 2>/dev/null && echo 1 || echo 0)"
HAS_KCONSOLE="$(grep -q "No user process; kernel console" "$SERIAL_LOG" 2>/dev/null && echo 1 || echo 0)"
HAS_GFX="$(grep -q "\[GFX] init ok" "$SERIAL_LOG" 2>/dev/null && echo 1 || echo 0)"
HAS_WM="$(grep -q "Starting WM in userspace" "$SERIAL_LOG" 2>/dev/null && echo 1 || echo 0)"
log_ndjson "B" "scripts/test_qemu_serial.sh:post_qemu" "captured serial" "{\"serialBytes\":${SERIAL_BYTES},\"hasBootLoad\":${HAS_BOOT_LOAD},\"hasExitBootServices\":${HAS_EXIT_BS},\"hasKernelMain\":${HAS_KERNEL_MAIN},\"hasTty\":${HAS_TTY},\"hasShell\":${HAS_SHELL},\"hasKernelConsole\":${HAS_KCONSOLE}}"
if [ "$QEMU_DEBUG" = "1" ]; then
  DEBUG_BYTES="$(wc -c < "$QEMU_DEBUG_LOG" 2>/dev/null || echo 0)"
  log_ndjson "D" "scripts/test_qemu_serial.sh:post_qemu" "captured qemu debug" "{\"debugBytes\":${DEBUG_BYTES}}"
fi

# Assert required lines (one per test case for CI)
FAIL=0
check() {
  if grep -q "$1" "$SERIAL_LOG" 2>/dev/null; then
    echo "  OK: $2"
  else
    echo "  FAIL: $2 (missing: $1)"
    FAIL=1
  fi
}

echo "Checking serial output in $SERIAL_LOG..."
check "custom_kernel boot: loading kernel" "Boot loader loads kernel"
check "Exiting boot services" "Boot exits and jumps"
check "\[KRN\] kernel_main_entry" "Kernel main reached"
check "TTY ready" "TTY ready"
# Either shell or kernel console (optional for early milestone)
if grep -q "Starting shell in userspace" "$SERIAL_LOG" 2>/dev/null; then
  echo "  OK: Userland shell started"
elif grep -q "No user process; kernel console" "$SERIAL_LOG" 2>/dev/null; then
  echo "  OK: Kernel console fallback"
else
  echo "  WARN: No shell or kernel console message (kernel main + TTY reached)"
fi

if [ "$HAS_GFX" -eq 1 ]; then
  echo "  OK: Graphics initialized"
else
  echo "  WARN: Graphics init log not found (running in text-only mode?)"
fi

if [ "$HAS_WM" -eq 1 ]; then
  echo "  OK: WM process started"
else
  echo "  WARN: WM start log not found (falling back to shell or kernel console)"
fi
if grep -q "\[WM\] started" "$SERIAL_LOG" 2>/dev/null; then
  echo "  OK: WM userland reached ([WM] started)"
else
  echo "  WARN: [WM] started not found (WM may not have reached event loop)"
fi

if [ "$FAIL" -eq 0 ]; then
  echo "QEMU serial tests passed."
  log_ndjson "A" "scripts/test_qemu_serial.sh:result" "tests passed" "{\"FAIL\":0}"
  exit 0
fi
log_ndjson "B" "scripts/test_qemu_serial.sh:result" "tests failed" "{\"FAIL\":1}"
exit 1
