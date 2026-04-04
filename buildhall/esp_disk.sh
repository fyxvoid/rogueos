#!/bin/sh
# Build a GPT-based EFI System Partition disk image that OVMF will mount as fs0:.
#
# Creates a standalone GPT disk image with a single FAT32 EFI System Partition:
#   \EFI\BOOT\BOOTX64.EFI
#   \kernel.elf
#
# Run as regular user: uses mtools (mformat, mcopy) — no sudo.
# Run as root: uses losetup + mkfs.fat + mount (same result).
# Requirements: sgdisk (gptfdisk). For no-sudo: mtools. For sudo path: dosfstools, util-linux.

set -e

SCRIPT_DIR="$(dirname "$0")"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

ESP_DISK="$ROOT/build/esp_disk.img"
UEFI_BOOT_DIR="$ROOT/build/uefi-boot"

echo "[esp_disk] using existing build/uefi-boot contents (run scripts/build_os.sh first)"

if [ ! -f "$UEFI_BOOT_DIR/EFI/BOOT/BOOTX64.EFI" ]; then
  echo "[esp_disk] ERROR: $UEFI_BOOT_DIR/EFI/BOOT/BOOTX64.EFI not found"
  echo "[esp_disk] Hint: run ./scripts/build_os.sh as your regular user before this script."
  exit 1
fi
if [ ! -f "$UEFI_BOOT_DIR/kernel.elf" ]; then
  echo "[esp_disk] ERROR: $UEFI_BOOT_DIR/kernel.elf not found"
  echo "[esp_disk] Hint: run ./scripts/build_os.sh as your regular user before this script."
  exit 1
fi

echo "[esp_disk] creating 64 MiB disk image at $ESP_DISK..."
mkdir -p "$ROOT/build"
rm -f "$ESP_DISK"
dd if=/dev/zero of="$ESP_DISK" bs=1M count=64 status=none

echo "[esp_disk] creating GPT with single EFI System partition..."
sgdisk "$ESP_DISK" -o >/dev/null
sgdisk "$ESP_DISK" -n 1:2048: -t 1:ef00 -c 1:"EFI System" >/dev/null

if [ "$(id -u)" -eq 0 ]; then
  # Root path: losetup + mkfs + mount
  echo "[esp_disk] attaching loop device..."
  LOOPDEV="$(losetup -f)"
  losetup -fP "$ESP_DISK"
  LOOPDEV="$(losetup -j "$ESP_DISK" | cut -d: -f1)"
  PART="${LOOPDEV}p1"
  if [ ! -b "$PART" ]; then
    echo "[esp_disk] ERROR: partition device $PART not found after losetup -fP"
    losetup -d "$LOOPDEV" || true
    exit 1
  fi
  echo "[esp_disk] formatting EFI System partition as FAT32..."
  mkfs.fat -F32 "$PART" >/dev/null
  MNT="/mnt/kingdom-esp"
  mkdir -p "$MNT"
  echo "[esp_disk] mounting $PART at $MNT..."
  mount "$PART" "$MNT"
  echo "[esp_disk] copying BOOTX64.EFI and kernel.elf..."
  mkdir -p "$MNT/EFI/BOOT"
  cp "$UEFI_BOOT_DIR/EFI/BOOT/BOOTX64.EFI" "$MNT/EFI/BOOT/BOOTX64.EFI"
  cp "$UEFI_BOOT_DIR/kernel.elf" "$MNT/kernel.elf"
  sync
  umount "$MNT"
  losetup -d "$LOOPDEV"
  [ -n "$SUDO_USER" ] && chown "$SUDO_USER":"$SUDO_USER" "$ESP_DISK" || true
else
  # No-sudo path: mtools (format and copy into image at partition offset)
  if ! command -v mformat >/dev/null 2>&1 || ! command -v mcopy >/dev/null 2>&1; then
    echo "[esp_disk] ERROR: mtools (mformat, mcopy) required when not root. Install: mtools"
    exit 1
  fi
  echo "[esp_disk] formatting partition 1 as FAT32 with mtools (no sudo)..."
  # Partition 1 starts at sector 2048; use explicit byte offset so FAT is at correct LBA (mtools partition=1 can mis-place on file images).
  PART1_OFFSET=$((2048 * 512))
  MTOOLSRC="$(mktemp -t mtoolsrc.XXXXXX)"
  trap "rm -f $MTOOLSRC" EXIT
  echo "drive a: file=\"$ESP_DISK\" offset=$PART1_OFFSET" > "$MTOOLSRC"
  export MTOOLSRC
  mformat -F -v ESP a:
  echo "[esp_disk] copying BOOTX64.EFI and kernel.elf..."
  mmd a:/EFI a:/EFI/BOOT 2>/dev/null || true
  mcopy -s "$UEFI_BOOT_DIR/EFI/BOOT/BOOTX64.EFI" a:/EFI/BOOT/BOOTX64.EFI
  mcopy -s "$UEFI_BOOT_DIR/kernel.elf" a:/kernel.elf
fi

echo "[esp_disk] GPT-based ESP disk image ready at $ESP_DISK"
echo ""
echo "To boot it in QEMU with OVMF and see fs0: in the UEFI Shell, run (as your user):"
echo ""
echo "  qemu-system-x86_64 \\"
echo "    -smp 2 \\"
echo "    -m 4096 \\"
echo "    -drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/x64/OVMF_CODE.4m.fd \\"
echo "    -drive if=pflash,format=raw,file=\$HOME/.ovmf/OVMF_VARS.fd \\"
echo "    -drive file=build/esp_disk.img,if=virtio,format=raw \\"
echo "    -serial stdio \\"
echo "    -display gtk \\"
echo "    -no-reboot"
echo ""
echo "In the UEFI Shell:"
echo "  Shell> map -r"
echo "  Shell> fs0:"
echo "  fs0:\\> cd EFI\\BOOT"
echo "  fs0:\\EFI\\BOOT> BOOTX64.EFI"

