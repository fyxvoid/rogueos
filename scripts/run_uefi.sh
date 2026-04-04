#!/bin/sh
# Build boot + kernel, create FAT layout, run QEMU with OVMF.
set -e
cd "$(dirname "$0")/.."
ROOT="$PWD"

# Build boot (EFI) and kernel
cargo build -p boot --target x86_64-unknown-uefi --release 2>/dev/null || cargo build -p boot --target x86_64-unknown-uefi
./scripts/build_kernel.sh

# FAT directory for QEMU (fat:rw:dir)
BUILD_DIR="${BUILD_DIR:-$ROOT/build/uefi-boot}"
mkdir -p "$BUILD_DIR/EFI/boot"
if [ -f "$ROOT/target/x86_64-unknown-uefi/release/boot.efi" ]; then
  cp "$ROOT/target/x86_64-unknown-uefi/release/boot.efi" "$BUILD_DIR/EFI/boot/bootx64.efi"
else
  cp "$ROOT/target/x86_64-unknown-uefi/debug/boot.efi" "$BUILD_DIR/EFI/boot/bootx64.efi"
fi
cp "$ROOT/target/x86_64-unknown-none/release/kernel" "$BUILD_DIR/kernel.elf"

OVMF="${OVMF_CODE:-}"
if [ -z "$OVMF" ]; then
  for p in /usr/share/edk2/x64/OVMF_CODE.4m.fd /usr/share/edk2/ovmf/x64/OVMF_CODE.fd /usr/share/OVMF/OVMF_CODE.fd; do
    [ -f "$p" ] && OVMF="$p" && break
  done
fi
if [ ! -f "$OVMF" ]; then
  echo "OVMF not found. Install edk2-ovmf or set OVMF_CODE path."
  exit 1
fi

echo "Starting QEMU (FAT: $BUILD_DIR, OVMF: $OVMF)"
exec qemu-system-x86_64 \
  -enable-kvm \
  -m 128 \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF" \
  -drive "file=fat:rw:$BUILD_DIR,format=raw,media=disk" \
  -serial stdio \
  -no-reboot \
  -display none \
  "$@"
