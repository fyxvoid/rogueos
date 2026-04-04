#!/bin/sh
# Run userland tests (unified userland: core, compositor). No kernel or display required.
# Usage: ./scripts/run_display_stack.sh
# See docs/userland-display-host.md for unified userland context.
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "[run_display_stack] Running unified userland tests (no kernel)..."
if cargo test -p userland-core -p userland-compositor 2>&1; then
  echo "[run_display_stack] userland-core and userland-compositor tests OK"
else
  echo "[run_display_stack] Tests failed"
  exit 1
fi
echo "[run_display_stack] Done."
exit 0
