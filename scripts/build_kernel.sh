#!/bin/sh
# Build the bare-metal kernel for x86_64-unknown-none (non-PIE).
set -e
cd "$(dirname "$0")/.."
export RUSTFLAGS="${RUSTFLAGS:--C relocation-model=static -C link-arg=-no-pie}"
exec cargo build -p kernel --bin kernel --release --target x86_64-unknown-none "$@"
