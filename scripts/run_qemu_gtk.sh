#!/bin/sh
# Run QEMU with a GTK window and serial -> this terminal, using the
# same FAT disk layout as run_qemu.sh but without rebuilding.
# Exit QEMU: Ctrl-A then X, or close the GTK window.
set -e
cd "$(dirname "$0")/.."
ROOT="$PWD"

BUILD_DIR="${BUILD_DIR:-$ROOT/build/uefi-boot}"
mkdir -p "$BUILD_DIR"

# Prefer explicit OVMF_CODE if set; otherwise try common paths.
OVMF="${OVMF_CODE:-}"
if [ -z "$OVMF" ]; then
  for p in /usr/share/edk2/x64/OVMF_CODE.4m.fd /usr/share/edk2/ovmf/x64/OVMF_CODE.fd /usr/share/OVMF/OVMF_CODE.fd; do
    [ -f "$p" ] && OVMF="$p" && break
  done
fi
if [ ! -f "$OVMF" ]; then
  echo "OVMF not found. Install edk2-ovmf (e.g. edk2-ovmf on Arch) or set OVMF_CODE."
  exit 1
fi

echo "Starting QEMU (GTK window, serial -> this terminal)"
echo "  Exit: Ctrl-A then X, or close the window"
echo ""

exec qemu-system-x86_64 \
  -m 128 \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF" \
  -drive "file=fat:rw:$BUILD_DIR,format=raw,media=disk" \
  -serial stdio \
  -display gtk \
  -no-reboot \
  "$@"

