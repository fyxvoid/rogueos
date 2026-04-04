#!/bin/sh
# QEMU demo runner (plan Section 8):
# - UEFI (OVMF)
# - 2 cores, 4 GiB RAM
# - NVMe disk image for persistence
# - Serial -> stdio
set -e

# Repo root: scripts/ lives directly under the workspace root.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_DIR="${BUILD_DIR:-$ROOT/build/uefi-demo}"
NVME_IMG="${NVME_IMG:-$BUILD_DIR/nvme.img}"

mkdir -p "$BUILD_DIR/EFI/boot"

if [ -z "$SKIP_BUILD" ]; then
  echo "Building userland..."
  cargo build -p userland --release --target x86_64-unknown-none

  echo "Building kernel..."
  RUSTFLAGS="-C relocation-model=static -C link-arg=-no-pie" cargo build -p kernel --release --target x86_64-unknown-none --bin kernel

  echo "Building UEFI bootloader..."
  cargo build -p boot --target x86_64-unknown-uefi --release

  cp "$ROOT/target/x86_64-unknown-uefi/release/boot.efi" "$BUILD_DIR/EFI/boot/bootx64.efi"
  cp "$ROOT/target/x86_64-unknown-none/release/kernel" "$BUILD_DIR/kernel.elf"
fi

if [ ! -f "$NVME_IMG" ]; then
  echo "Creating NVMe image: $NVME_IMG"
  mkdir -p "$(dirname "$NVME_IMG")"
  dd if=/dev/zero of="$NVME_IMG" bs=1M count=256 status=none
fi

OVMF="${OVMF_CODE:-}"
if [ -z "$OVMF" ]; then
  for p in /usr/share/edk2/x64/OVMF_CODE.4m.fd /usr/share/edk2/ovmf/x64/OVMF_CODE.fd /usr/share/OVMF/OVMF_CODE.fd; do
    [ -f "$p" ] && OVMF="$p" && break
  done
fi
if [ ! -f "$OVMF" ]; then
  echo "OVMF not found. Install edk2-ovmf or set OVMF_CODE."
  exit 1
fi

echo "Starting QEMU demo (serial -> this terminal)"
echo "  Exit: Ctrl-A then X"
echo ""

exec qemu-system-x86_64 \
  -smp 2 \
  -m 4096 \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF" \
  -drive "file=fat:rw:$BUILD_DIR,format=raw,media=disk" \
  -drive "file=$NVME_IMG,if=none,format=raw,id=nvm" \
  -device nvme,drive=nvm,serial=deadbeef \
  -device qemu-xhci,id=xhci \
  -device usb-kbd,bus=xhci.0 \
  -device usb-mouse,bus=xhci.0 \
  -serial stdio \
  -display gtk \
  -no-reboot \
  "$@"

