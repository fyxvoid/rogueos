#!/bin/sh
# RogueOS: run GPT ESP disk (build/esp_disk.img) in QEMU with UEFI (OVMF).
# Uses serial->stdio for logs and a simple virtio disk for the ESP image.
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ESP_DISK="${ESP_DISK:-$ROOT/build/esp_disk.img}"

if [ ! -f "$ESP_DISK" ]; then
  echo "[buildhall] ESP disk image not found at $ESP_DISK."
  echo "[buildhall] Please run (as root) to create/update it:"
  echo "  sudo ./buildhall/esp_disk.sh"
  exit 1
fi

OVMF="${OVMF_CODE:-}"
if [ -z "$OVMF" ]; then
  for p in /usr/share/edk2/x64/OVMF_CODE.4m.fd /usr/share/edk2/ovmf/x64/OVMF_CODE.fd /usr/share/OVMF/OVMF_CODE.fd; do
    [ -f "$p" ] && OVMF="$p" && break
  done
fi
if [ ! -f "$OVMF" ]; then
  echo "[buildhall] OVMF not found. Install edk2-ovmf or set OVMF_CODE."
  exit 1
fi

echo "[buildhall] starting QEMU (ESP_DISK=$ESP_DISK, OVMF=$OVMF)"
exec qemu-system-x86_64 \
  -smp 2 \
  -m 4096 \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF" \
  -drive "file=$ESP_DISK,if=virtio,format=raw" \
  -chardev stdio,id=con0,signal=off \
  -serial chardev:con0 \
  -display none \
  -no-reboot \
  -no-shutdown \
  "$@"

