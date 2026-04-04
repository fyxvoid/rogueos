#!/bin/sh
# Verify that build artifacts exist (no QEMU). Exit 0 if all present, 1 otherwise.
# Run from repo root after ./scripts/build_os.sh
set -e
cd "$(dirname "$0")/.."
ROOT="$PWD"

missing=0
if [ ! -f "$ROOT/build/uefi-boot/kernel.elf" ]; then
  echo "Missing: build/uefi-boot/kernel.elf"
  missing=1
fi
if [ ! -f "$ROOT/build/uefi-boot/EFI/boot/bootx64.efi" ]; then
  echo "Missing: build/uefi-boot/EFI/boot/bootx64.efi"
  missing=1
fi
if [ ! -f "$ROOT/target/x86_64-unknown-none/release/shell" ]; then
  echo "Missing: target/x86_64-unknown-none/release/shell"
  missing=1
fi

if [ "$VERIFY_ISO" = "1" ]; then
  if [ ! -f "$ROOT/build/os.iso" ]; then
    echo "Missing: build/os.iso (set VERIFY_ISO=1 to require ISO). Run ./scripts/mkiso.sh or ./scripts/build_os.sh --iso."
    missing=1
  fi
fi

if [ "$missing" -eq 0 ]; then
  echo "All build artifacts present."
  exit 0
fi
exit 1
