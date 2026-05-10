#!/bin/sh
# Build a UEFI-bootable ISO from build/uefi-boot (FAT image + xorriso).
# Requires: mtools (mformat, mmd, mcopy), xorriso.
# Run after scripts/build_os.sh has populated build/uefi-boot.
set -e
cd "$(dirname "$0")/.."
ROOT="$PWD"

BUILD_DIR="${BUILD_DIR:-$ROOT/build/uefi-boot}"
EFI_IMG="${EFI_IMG:-$ROOT/build/efi.img}"
ISO_OUT="${ISO_OUT:-$ROOT/build/os.iso}"

# Dependency checks
for cmd in mformat mmd mcopy; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing: $cmd (from mtools). Install mtools (e.g. Arch: mtools, Debian: mtools)."
    exit 1
  fi
done
if ! command -v xorriso >/dev/null 2>&1; then
  echo "Missing: xorriso. Install libisoburn (e.g. Arch: libisoburn, Debian: xorriso)."
  exit 1
fi

# Require build/uefi-boot with kernel and bootloader
if [ ! -f "$BUILD_DIR/kernel.elf" ]; then
  echo "Missing $BUILD_DIR/kernel.elf. Run ./scripts/build_os.sh first."
  exit 1
fi
if [ ! -f "$BUILD_DIR/EFI/BOOT/BOOTX64.EFI" ]; then
  echo "Missing $BUILD_DIR/EFI/BOOT/BOOTX64.EFI. Run ./scripts/build_os.sh first."
  exit 1
fi

mkdir -p "$(dirname "$EFI_IMG")" "$(dirname "$ISO_OUT")"

# Create FAT32 EFI system partition image (32 MiB)
echo "Creating FAT image $EFI_IMG..."
dd if=/dev/zero of="$EFI_IMG" bs=1M count=32 status=none
mformat -F -v "ROGUEOS" -i "$EFI_IMG" ::
mmd -i "$EFI_IMG" ::EFI
mmd -i "$EFI_IMG" ::EFI/BOOT
mcopy -i "$EFI_IMG" "$BUILD_DIR/kernel.elf" ::kernel.elf
mcopy -i "$EFI_IMG" "$BUILD_DIR/EFI/BOOT/BOOTX64.EFI" ::EFI/BOOT/BOOTX64.EFI

# Build ISO with El Torito UEFI boot, using the FAT image as the EFI System Partition.
echo "Building ISO $ISO_OUT..."
xorriso -as mkisofs -o "$ISO_OUT" -r -V "ROGUEOS" \
  -append_partition 2 0xef "$EFI_IMG" \
  -appended_part_as_gpt \
  -e --interval:appended_partition_2:all:: \
  -no-emul-boot \
  -partition_offset 16 \
  "$BUILD_DIR"

echo "ISO ready: $ISO_OUT"
