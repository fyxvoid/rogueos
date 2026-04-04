#!/bin/sh
# Run QEMU with the bootable ISO (build/os.iso). Serial -> stdio.
# Build the ISO first: ./scripts/build_os.sh && ./scripts/mkiso.sh (or ./scripts/build_os.sh --iso).
# Set AUTO_MKISO=1 to run mkiso.sh if os.iso is missing.
# Exit QEMU: Ctrl-A then X.
set -e
cd "$(dirname "$0")/.."
ROOT="$PWD"

ISO="${ISO:-$ROOT/build/os.iso}"

if [ ! -f "$ISO" ]; then
  if [ "$AUTO_MKISO" = "1" ]; then
    ./scripts/mkiso.sh
  else
    echo "ISO not found: $ISO"
echo "Run: ./scripts/build_os.sh && ./scripts/mkiso.sh"
  echo "Or:  ./scripts/build_os.sh --iso"
    echo "Or set AUTO_MKISO=1 to build ISO automatically."
    exit 1
  fi
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

echo "Starting QEMU from ISO (serial -> this terminal)"
echo "  Exit: Ctrl-A then X"
echo ""

exec qemu-system-x86_64 \
  -m 128 \
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF" \
  -cdrom "$ISO" \
  -serial stdio \
  -display none \
  -no-reboot \
  "$@"
