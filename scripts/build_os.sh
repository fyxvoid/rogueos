#!/bin/sh
# Build entire OS: kernel + userland (shell/init) plus UEFI bootloader, and
# optionally a bootable ISO image and host desktop utilities.
#
# Usage (from repo root):
#   ./scripts/build_os.sh             # kernel + userland + bootloader
#   ./scripts/build_os.sh --iso       # also build bootable ISO (build/os.iso)
#   ./scripts/build_os.sh --desktop   # also build host desktop crates
#   ./scripts/build_os.sh --run       # build then run QEMU (if scripts/run_qemu.sh exists)
set -e
SCRIPT_DIR="$(dirname "$0")"
# Repo root: scripts/ lives directly under the workspace root.
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

BUILD_DESKTOP=false
RUN_QEMU=false
BUILD_ISO=false
for arg in "$@"; do
  case "$arg" in
    --desktop) BUILD_DESKTOP=true ;;
    --run)     RUN_QEMU=true ;;
    --iso)     BUILD_ISO=true ;;
  esac
done

BUILD_DIR="${BUILD_DIR:-$ROOT/build/uefi-boot}"
mkdir -p "$BUILD_DIR/EFI/boot"

# Build everything in a clearly ordered, one-by-one fashion.
# Also keep Cargo itself single-threaded unless the user overrides it.
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
echo "CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS (set this env var to change parallelism)"

# Userland must be non-PIE (relocation-model=static) so kernel loader can map at p_vaddr without applying relocations.
export RUSTFLAGS_USERLAND="${RUSTFLAGS_USERLAND:--C relocation-model=static -C link-arg=-no-pie}"

echo "=== Step 1/4: Building userland shell (must be first; kernel embeds it) ==="
if ! RUSTFLAGS="$RUSTFLAGS_USERLAND" cargo build -p userland --release --target x86_64-unknown-none --bin shell; then
  echo "WARNING: userland shell build failed. Kernel will fall back to kernel console."
fi

echo "=== Step 2/4: Building userland WM (optional, for graphics mode) ==="
if ! RUSTFLAGS="$RUSTFLAGS_USERLAND" cargo build -p userland --release --target x86_64-unknown-none --bin wm; then
  echo \"WARNING: userland WM build failed. System will fall back to shell or kernel console.\"
fi

echo "=== Step 3/4: Building userland init (optional) ==="
# Force init rebuild so kernel always embeds EXEC init (entry 0x400000); otherwise stale DYN may have wrong entry.
touch "$ROOT/userland/src/bin/init.rs" 2>/dev/null || true
if ! RUSTFLAGS="$RUSTFLAGS_USERLAND" cargo build -p userland --release --target x86_64-unknown-none --bin init; then
  echo "WARNING: userland init build failed (if not used, this is harmless)."
fi
# Force kernel rebuild so build.rs re-embeds the init we just built.
touch "$ROOT/kernel/audits/main.rs" 2>/dev/null || true

echo "=== Step 4/4: Building kernel ==="
RUSTFLAGS="-C relocation-model=static -C link-arg=-no-pie" cargo build -p kernel --release --target x86_64-unknown-none --bin kernel
echo "Kernel build finished."

echo "=== Step 5/5: Building UEFI bootloader ==="
if ! cargo build -p boot --target x86_64-unknown-uefi --release; then
  echo "WARNING: UEFI bootloader release build failed, trying debug build instead."
fi

BOOT_SRC="$ROOT/target/x86_64-unknown-uefi/release/boot.efi"
if [ ! -f "$BOOT_SRC" ]; then
  BOOT_SRC="$ROOT/target/x86_64-unknown-uefi/debug/boot.efi"
fi

# Populate UEFI boot tree:
# - Canonical default path: \EFI\BOOT\BOOTX64.EFI
# - Keep lowercase alias for tools that expect it.
mkdir -p "$BUILD_DIR/EFI/BOOT"
cp "$BOOT_SRC" "$BUILD_DIR/EFI/BOOT/BOOTX64.EFI"
mkdir -p "$BUILD_DIR/EFI/boot"
cp "$BOOT_SRC" "$BUILD_DIR/EFI/boot/bootx64.efi"

cp "$ROOT/target/x86_64-unknown-none/release/kernel" "$BUILD_DIR/kernel.elf"
echo "UEFI image ready in $BUILD_DIR"

if [ "$BUILD_ISO" = true ]; then
  echo "=== Building bootable ISO ==="
  "$SCRIPT_DIR/mkiso.sh"
fi

if [ "$BUILD_DESKTOP" = true ]; then
  echo "=== Building unified userland (session, init, wm, ...) ==="
  cargo build --release -p userland --target x86_64-unknown-none 2>/dev/null || echo "  (userland RogueOS target build failed or skipped)"
  echo "Unified userland build done (binaries in target/x86_64-unknown-none/release)."
fi

if [ "$RUN_QEMU" = true ]; then
  echo "=== Starting QEMU ==="
  if [ -f "$SCRIPT_DIR/run_qemu.sh" ]; then
    exec env SKIP_BUILD=1 "$SCRIPT_DIR/run_qemu.sh"
  else
    echo "run_qemu.sh not found under scripts/; skipping QEMU run."
  fi
fi
