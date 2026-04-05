#!/bin/sh
# Build kernel (and userland shell), set up UEFI boot, and run QEMU. Serial -> stdio.
# The 64-bit kernel is loaded by the UEFI bootloader (boot.efi), not -kernel.
# Exit QEMU: Ctrl-A then X, or close from another terminal.
# Set SKIP_BUILD=1 to skip build and use existing build/uefi-boot.
# Set QEMU_DEBUG_LOG=path to enable QEMU exception log (-d int,guest_errors -D path).
# Set QEMU_GDB=1 to start QEMU paused with GDB server on :1234 (-s -S).
set -e
cd "$(dirname "$0")/.."
ROOT="$PWD"

BUILD_DIR="${BUILD_DIR:-$ROOT/build/uefi-boot}"
mkdir -p "$BUILD_DIR/EFI/boot"

if [ -z "$SKIP_BUILD" ]; then
  echo "Building userland (all binaries)..."
  cargo build -p userland --release --target x86_64-unknown-none 2>/dev/null || true

  echo "Building kernel..."
  cargo build -p kernel --release --target x86_64-unknown-none --bin kernel

  echo "Building UEFI bootloader..."
  cargo build -p boot --target x86_64-unknown-uefi --release 2>/dev/null || true

  if [ -f "$ROOT/target/x86_64-unknown-uefi/release/boot.efi" ]; then
    cp "$ROOT/target/x86_64-unknown-uefi/release/boot.efi" "$BUILD_DIR/EFI/boot/bootx64.efi"
  else
    cp "$ROOT/target/x86_64-unknown-uefi/debug/boot.efi" "$BUILD_DIR/EFI/boot/bootx64.efi"
  fi
  cp "$ROOT/target/x86_64-unknown-none/release/kernel" "$BUILD_DIR/kernel.elf"
fi

OVMF="${OVMF_CODE:-}"
if [ -z "$OVMF" ]; then
  for p in /usr/share/edk2/x64/OVMF_CODE.4m.fd /usr/share/edk2/ovmf/x64/OVMF_CODE.fd /usr/share/OVMF/OVMF_CODE.fd /usr/share/OVMF/OVMF_CODE_4M.fd; do
    [ -f "$p" ] && OVMF="$p" && break
  done
fi
if [ ! -f "$OVMF" ]; then
  echo "OVMF not found. Install edk2-ovmf (e.g. edk2-ovmf on Arch) or set OVMF_CODE."
  exit 1
fi

echo "Starting QEMU (serial -> this terminal)"
echo "  Exit: Ctrl-A then X"
echo ""

QEMU_EXTRA=""
if [ -n "$QEMU_GDB" ]; then
  QEMU_EXTRA="$QEMU_EXTRA -s -S"
fi
if [ -n "$QEMU_DEBUG_LOG" ]; then
  QEMU_EXTRA="$QEMU_EXTRA -d int,guest_errors -D $QEMU_DEBUG_LOG"
  echo "QEMU debug log -> $QEMU_DEBUG_LOG"
fi

exec qemu-system-x86_64 \
  -m 512 \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF" \
  -drive "file=fat:rw:$BUILD_DIR,format=raw,media=disk" \
  -serial stdio \
  -display gtk \
  -no-reboot \
  $QEMU_EXTRA \
  "$@"
