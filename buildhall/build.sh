#!/bin/sh
# RogueOS: full workspace build (kernel + userland + bootloader) with strict warnings.
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export RUSTFLAGS="${RUSTFLAGS:--D warnings}"

echo "[buildhall] cargo clean"
cargo clean

echo "[buildhall] building workspace (kernel + userland + bootloader helpers)"
# Build shared libs and all binary crates for their intended targets.
cargo build -p libs
cargo build -p userland --release --target x86_64-unknown-none
cargo build -p kernel --release --target x86_64-unknown-none --bin kernel
cargo build -p boot --release --target x86_64-unknown-uefi

echo "[buildhall] build complete"

