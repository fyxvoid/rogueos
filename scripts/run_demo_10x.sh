#!/bin/sh
# Run the demo VM 10 times (host-side loop).
# This does not automate in-guest interactions; it just provides a repeatable launch loop.
set -e

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
N="${N:-10}"
PER_RUN_TIMEOUT="${PER_RUN_TIMEOUT:-180}"

i=1
while [ "$i" -le "$N" ]; do
  echo "demo run $i/$N"
  if command -v timeout >/dev/null 2>&1; then
    timeout "${PER_RUN_TIMEOUT}s" "$ROOT/system/build/run_qemu_demo.sh" "$@" || true
  else
    "$ROOT/system/build/run_qemu_demo.sh" "$@" || true
  fi
  i=$((i + 1))
done

