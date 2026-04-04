#!/bin/sh
# Print SHA256 of kernel.elf as stored on the ESP disk (for integrity check).
# Run as root: uses mount/losetup. Run as user: uses mtools (no sudo).
set -e

SCRIPT_DIR="$(dirname "$0")"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ESP_DISK="${ESP_DISK:-$ROOT/build/esp_disk.img}"

if [ ! -f "$ESP_DISK" ]; then
  echo "[esp_hash] ERROR: $ESP_DISK not found." >&2
  exit 1
fi

if [ "$(id -u)" -eq 0 ]; then
  MNT="${MNT:-/mnt/rogueos-esp}"
  LOOPDEV=""
  cleanup() {
    if [ -n "$LOOPDEV" ]; then
      umount "$MNT" 2>/dev/null || true
      losetup -d "$LOOPDEV" 2>/dev/null || true
    fi
  }
  trap cleanup EXIT
  mkdir -p "$MNT"
  losetup -fP "$ESP_DISK"
  LOOPDEV="$(losetup -j "$ESP_DISK" | cut -d: -f1)"
  PART="${LOOPDEV}p1"
  [ -b "$PART" ] || { echo "[esp_hash] ERROR: partition $PART not found." >&2; exit 1; }
  mount "$PART" "$MNT"
  sha256sum "$MNT/kernel.elf" | awk '{print $1}'
else
  if ! command -v mcopy >/dev/null 2>&1; then
    echo "[esp_hash] ERROR: mtools (mcopy) required when not root." >&2
    exit 1
  fi
  TMP="$(mktemp -t kernel.elf.XXXXXX)"
  trap "rm -f $TMP $MTOOLSRC" EXIT
  MTOOLSRC="$(mktemp -t mtoolsrc.XXXXXX)"
  echo "drive a: file=\"$ESP_DISK\" offset=$((2048 * 512))" > "$MTOOLSRC"
  export MTOOLSRC
  mcopy -n a:/kernel.elf "$TMP" 2>/dev/null || { echo "[esp_hash] ERROR: kernel.elf not found on ESP." >&2; exit 1; }
  sha256sum "$TMP" | awk '{print $1}'
fi
