#!/bin/sh
# Run all Rust unit and integration tests (libs, unified userland).
# Exit 0 if all pass, non-zero if any fail.
# From repo root: ./scripts/test_all_rust.sh
set -e
cd "$(dirname "$0")/.."
ROOT="$PWD"

FAIL=0
run_test() {
  if cargo test -p "$1" 2>&1; then
    echo "  OK: $1"
  else
    echo "  FAIL: $1"
    FAIL=1
  fi
}

echo "=== Rust unit/integration tests ==="
run_test libs
run_test userland-core
run_test userland-compositor
# kernel crate uses no_std; host tests conflict with std. Omit from this runner.

if [ "$FAIL" -eq 0 ]; then
  echo "All Rust tests passed."
  exit 0
fi
echo "One or more Rust test suites failed."
exit 1
