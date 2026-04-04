#!/bin/sh
# Kingdom OS: assemble bootable ISO into buildhall/output/kingdom-debug.iso.
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUTDIR="$ROOT/buildhall/output"
mkdir -p "$OUTDIR"

echo "[buildhall] ensuring UEFI image in build/uefi-boot"
"$ROOT/scripts/build_os.sh" --iso

ISO_SRC="$ROOT/build/os.iso"
ISO_DST="$OUTDIR/kingdom-debug.iso"

cp "$ISO_SRC" "$ISO_DST"

echo "[buildhall] ISO ready at $ISO_DST"

