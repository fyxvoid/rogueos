#!/bin/sh
# 50-boot consistency runner (plan Section 7).
# Collects serial output for each boot into build/boot-metrics/.
set -e

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT/build/boot-metrics}"
mkdir -p "$OUT_DIR"

if [ -z "$SKIP_BUILD" ]; then
  "$ROOT/system/build/run_qemu_demo.sh" -display none -serial stdio -S -no-reboot >/dev/null 2>&1 || true
  # Note: the above is just to ensure the build happens; it exits immediately due to -S.
fi

N="${N:-50}"
BOOT_TIMEOUT="${BOOT_TIMEOUT:-12}"

echo "Running $N boots; timeout=${BOOT_TIMEOUT}s each"

i=1
while [ "$i" -le "$N" ]; do
  log="$OUT_DIR/boot_$i.log"
  echo "boot $i/$N -> $log"
  if command -v timeout >/dev/null 2>&1; then
    timeout "${BOOT_TIMEOUT}s" "$ROOT/system/build/run_qemu_demo.sh" -display none -serial stdio >"$log" 2>&1 || true
  else
    echo "timeout(1) not found; running a single boot without limit"
    "$ROOT/system/build/run_qemu_demo.sh" -display none -serial stdio >"$log" 2>&1 || true
    break
  fi
  i=$((i + 1))
done

echo "Done. Extract metrics with:"
echo "  rg \"\\[METRIC\\]\" \"$OUT_DIR\""

