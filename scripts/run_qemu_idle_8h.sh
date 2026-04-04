#!/bin/sh
# 8-hour idle stability run (plan Section 4.5).
# Runs QEMU with demo settings and kills after 8h if still running.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if command -v timeout >/dev/null 2>&1; then
  exec timeout 8h "$SCRIPT_DIR/run_qemu_demo.sh" "$@"
fi

echo "timeout(1) not found; running without 8h limit."
exec "$SCRIPT_DIR/run_qemu_demo.sh" "$@"

