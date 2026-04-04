#!/bin/sh
# Build kernel with GRUB Multiboot2 and create a bootable ISO using grub-mkrescue.
#
# Usage (from repo root):
#   ./scripts/build_grub_iso.sh
#
# Produces: build/grub.iso (boot with QEMU: qemu-system-x86_64 -cdrom build/grub.iso -m 512)
#
# Requires: grub-mkrescue (or grub2-mkrescue), xorriso. Optional: userland built first.
set -e
SCRIPT_DIR="$(dirname "$0")"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

BUILD_DIR="${BUILD_DIR:-$ROOT/build}"
GRUB_DIR="${BUILD_DIR}/grub"
ISO_OUT="${BUILD_DIR}/grub.iso"
ISO_ROOT="${BUILD_DIR}/grub-iso"

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export RUSTFLAGS_USERLAND="${RUSTFLAGS_USERLAND:--C relocation-model=static -C link-arg=-no-pie}"

# Build userland so kernel can embed shell/init/wm
echo "=== Building userland (for embedded ELFs) ==="
for bin in shell wm init; do
  if ! RUSTFLAGS="$RUSTFLAGS_USERLAND" cargo build -p userland --release --target x86_64-unknown-none --bin "$bin" 2>/dev/null; then
    echo "WARNING: userland $bin build failed (kernel will still boot)."
  fi
done
touch "$ROOT/kernel/audits/main.rs" 2>/dev/null || true

# Build kernel with Multiboot2 (must use non-PIE; build from kernel so .cargo/config applies)
echo "=== Building kernel (Multiboot2) ==="
export RUSTFLAGS="${RUSTFLAGS:--C relocation-model=static -C link-arg=-no-pie}"
(cd "$ROOT/kernel" && cargo build --target x86_64-unknown-none --features multiboot2 --release)
cp "$ROOT/target/x86_64-unknown-none/release/kernel" "$BUILD_DIR/kernel-multiboot2.elf"

# Layout for grub-mkrescue: iso/boot/grub/grub.cfg and iso/kernel.elf
echo "=== Preparing GRUB ISO tree ==="
mkdir -p "$ISO_ROOT/boot/grub"
cp "$GRUB_DIR/grub.cfg" "$ISO_ROOT/boot/grub/grub.cfg"
cp "$BUILD_DIR/kernel-multiboot2.elf" "$ISO_ROOT/kernel.elf"

echo "=== Building ISO (grub-mkrescue) ==="
if ! command -v grub-mkrescue >/dev/null 2>&1; then
  if command -v grub2-mkrescue >/dev/null 2>&1; then
    GRUB_MKRESCUE=grub2-mkrescue
  else
    echo "Missing: grub-mkrescue or grub2-mkrescue. Install grub (e.g. Arch: grub, Debian: grub-pc-bin)."
    exit 1
  fi
else
  GRUB_MKRESCUE=grub-mkrescue
fi
if ! command -v xorriso >/dev/null 2>&1; then
  echo "Missing: xorriso (grub-mkrescue uses it). Install libisoburn."
  exit 1
fi

$GRUB_MKRESCUE -o "$ISO_OUT" "$ISO_ROOT" --compress=xz

echo "ISO ready: $ISO_OUT"
echo "Run: qemu-system-x86_64 -cdrom $ISO_OUT -m 512"
