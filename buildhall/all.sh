#!/bin/sh
# Kingdom OS: one-shot build + ESP image + QEMU run.
# Usage (from repo root or any dir):
#   ./buildhall/all.sh        # build everything, refresh ESP disk, run QEMU
#   ./buildhall/all.sh --arg  # extra args passed through to qemu.sh

set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "=== [kingdom] Step 1/3: build kernel + userland + bootloader ==="
./scripts/build_os.sh

echo "=== [kingdom] Step 2/3: rebuild GPT ESP disk image (sudo may prompt) ==="
if [ "$(id -u)" -ne 0 ]; then
  sudo ./buildhall/esp_disk.sh
else
  ./buildhall/esp_disk.sh
fi

echo "=== [kingdom] Step 3/3: run QEMU on fresh ESP image ==="
exec ./buildhall/qemu.sh "$@"

