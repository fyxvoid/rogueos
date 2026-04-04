#!/bin/sh
# Build a UEFI-bootable ISO and run it in VirtualBox.
#
# Requires:
# - VirtualBox (VBoxManage)
# - xorriso (for mkiso)
#
# Notes:
# - Uses a serial log file for debugging: build/vbox-serial.log
# - Creates/updates a VM named by VM_NAME (default: RogueOS)
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VM_NAME="${VM_NAME:-RogueOS}"
ISO="${ISO:-$ROOT/build/os.iso}"
SERIAL_LOG="${SERIAL_LOG:-$ROOT/build/vbox-serial.log}"

if ! command -v VBoxManage >/dev/null 2>&1; then
  echo "VBoxManage not found. Install VirtualBox or add it to PATH."
  exit 1
fi

if [ -z "$SKIP_BUILD" ]; then
  # Delegate to the main build script so all targets and the UEFI tree are in sync.
  "$ROOT/scripts/build_os.sh" --iso
fi

mkdir -p "$(dirname "$SERIAL_LOG")"
: > "$SERIAL_LOG"

if ! VBoxManage list vms | grep -q "\"$VM_NAME\""; then
  VBoxManage createvm --name "$VM_NAME" --ostype "Other_64" --register
fi

# If a previous run is still active or crashed, force it off to avoid lock errors.
if VBoxManage list runningvms | grep -q "\"$VM_NAME\""; then
  VBoxManage controlvm "$VM_NAME" poweroff 2>/dev/null || true
fi

# Basic VM settings: UEFI, serial debug, no GUI by default (override with GUI=1).
VBoxManage modifyvm "$VM_NAME" \
  --firmware efi \
  --memory "${MEMORY_MB:-1024}" \
  --cpus "${CPUS:-2}" \
  --ioapic on \
  --rtcuseutc on \
  --boot1 dvd \
  --boot2 disk \
  --boot3 none \
  --boot4 none \
  --uart1 0x3F8 4 \
  --uartmode1 file "$SERIAL_LOG" \
  --audio none \
  --usb off \
  --usbehci off \
  --usbxhci off

# Ensure a SATA controller exists.
if ! VBoxManage showvminfo "$VM_NAME" --machinereadable | grep -q '^storagecontrollername0='; then
  VBoxManage storagectl "$VM_NAME" --name "SATA" --add sata --controller IntelAhci
fi

# Attach ISO as DVD (port 0 device 0). Detach any previous media first.
VBoxManage storageattach "$VM_NAME" --storagectl "SATA" --port 0 --device 0 --type dvddrive --medium none 2>/dev/null || true
VBoxManage storageattach "$VM_NAME" --storagectl "SATA" --port 0 --device 0 --type dvddrive --medium "$ISO"

if [ "${GUI:-0}" = "1" ]; then
  VBoxManage startvm "$VM_NAME" --type gui
else
  VBoxManage startvm "$VM_NAME" --type headless
fi

echo "VM started: $VM_NAME"
echo "Serial log: $SERIAL_LOG"

