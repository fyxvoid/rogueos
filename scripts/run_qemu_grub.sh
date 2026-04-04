#!/bin/sh
# Boot RogueOS with GRUB (Multiboot2) from a CD-ROM ISO.
# Uses SeaBIOS (no OVMF); GRUB loads the kernel from the ISO.
#
# Usage (from repo root):
#   ./scripts/run_qemu_grub.sh
#
# Set SKIP_BUILD=1 to skip building the ISO and use existing build/grub.iso.
# Exit QEMU: Ctrl-A then X.
set -e
cd "$(dirname "$0")/.."
ROOT="$PWD"

ISO="${ISO:-$ROOT/build/grub.iso}"

if [ ! -f "$ISO" ] || [ -z "$SKIP_BUILD" ]; then
  if [ ! -f "$ISO" ]; then
    echo "Building GRUB ISO..."
  else
    echo "SKIP_BUILD not set; rebuilding GRUB ISO..."
  fi
  ./scripts/build_grub_iso.sh
fi

if [ ! -f "$ISO" ]; then
  echo "ISO not found: $ISO"
  exit 1
fi

echo "Starting QEMU with GRUB (serial -> this terminal)"
echo "  ISO: $ISO"
echo "  Exit: Ctrl-A then X"
echo "  (Set QEMU_DEBUG_INT=1 for interrupt logging)"
echo ""

QEMU_EXTRA=""
[ "$QEMU_DEBUG_INT" = "1" ] && QEMU_EXTRA="-d int"

exec qemu-system-x86_64 \
  -m 512 \
  -cdrom "$ISO" \
  -serial stdio \
  -display gtk \
  -no-reboot \
  -no-shutdown \
  $QEMU_EXTRA \
  "$@"
