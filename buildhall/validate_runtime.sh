#!/bin/sh
# Runtime validation: rebuild, update ESP, verify kernel hash, then boot RogueOS in QEMU.
# Guarantees the kernel executed in QEMU matches the freshly built binary exactly.
# Set SKIP_REBUILD=1 to skip rebuild/ESP/hash and only run QEMU (for quick re-runs).
#
# Profiles:
#   Default: UEFI only (ESP + OVMF).
#   VALIDATE_GRUB=1: GRUB (Multiboot2) boot only; no OVMF.
#   VALIDATE_BOTH=1: Run UEFI then GRUB; pass only if both pass.
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [ "$VALIDATE_BOTH" = "1" ]; then
  echo "[validate] VALIDATE_BOTH=1: running UEFI profile then GRUB profile..."
  "$ROOT/buildhall/validate_runtime.sh" || exit 1
  VALIDATE_GRUB=1 SKIP_REBUILD=1 "$ROOT/buildhall/validate_runtime.sh" || exit 1
  echo "[validate] Both profiles PASS."
  exit 0
fi

if [ "$VALIDATE_GRUB" = "1" ]; then
  # ---------- GRUB profile ----------
  BUILD_DIR="${BUILD_DIR:-$ROOT/build}"
  LOG_DIR="$BUILD_DIR/validate_logs"
  mkdir -p "$LOG_DIR"
  SERIAL_LOG="${SERIAL_LOG:-$BUILD_DIR/runtime_serial_grub.log}"
  QEMU_DEBUG_LOG="${QEMU_DEBUG_LOG:-$BUILD_DIR/runtime_qemu_debug_grub.log}"
  ISO="${BUILD_DIR}/grub.iso"

  if [ "${SKIP_REBUILD:-0}" != "1" ]; then
    echo "[validate] GRUB: Building ISO..."
    "$ROOT/scripts/build_grub_iso.sh" || { echo "[validate] ERROR: build_grub_iso.sh failed."; exit 1; }
  fi
  if [ ! -f "$ISO" ]; then
    echo "[validate] ERROR: $ISO not found. Run ./scripts/build_grub_iso.sh"
    exit 1
  fi

  echo "[validate] GRUB: Booting from CD (65s observation)..."
  rm -f "$SERIAL_LOG" "$QEMU_DEBUG_LOG"
  [ -n "$DISPLAY" ] && QEMU_DISPLAY="-display gtk" || QEMU_DISPLAY="-display none"
  (
    qemu-system-x86_64 -m 512 -cdrom "$ISO" \
      -serial "file:$SERIAL_LOG" \
      $QEMU_DISPLAY -no-reboot -no-shutdown \
      -d int,guest_errors -D "$QEMU_DEBUG_LOG"
  ) &
  QEMU_PID=$!
  sleep 65
  kill $QEMU_PID 2>/dev/null || true
  wait $QEMU_PID 2>/dev/null || true

  BOOT_FAIL=0
  if ! grep -q "\[KRN\] kernel_main_entry" "$SERIAL_LOG" 2>/dev/null; then
    echo "[validate] GRUB FAIL: No [KRN] kernel_main_entry in serial log."
    BOOT_FAIL=1
  fi
  for tag in "step2 heap_ready" "step6 register_programs" "step7" "user_init_spawn" "[INIT] steward start" "[WM] tick"; do
    if ! grep -q "$tag" "$SERIAL_LOG" 2>/dev/null; then
      echo "[validate] GRUB FAIL: Boot phase missing: $tag"
      BOOT_FAIL=1
    fi
  done

  EXCEPTION_COUNT=0
  [ -f "$QEMU_DEBUG_LOG" ] && EXCEPTION_COUNT=$(grep -c -E "check_exception.*new 0x(6|8|d)|Triple fault|invalid opcode" "$QEMU_DEBUG_LOG" 2>/dev/null || true)
  EXCEPTION_COUNT=$((EXCEPTION_COUNT + 0))
  [ "$EXCEPTION_COUNT" -gt 0 ] && echo "[validate] GRUB FAIL: CPU exception(s) in debug log" && BOOT_FAIL=1

  INVARIANT_VIOLATIONS=0
  for pattern in "canary" "buddy_oob" "double_free" "page_fault_unhandled" "diagnostic_halt" "KERNEL PANIC"; do
    grep -qi "$pattern" "$SERIAL_LOG" 2>/dev/null && INVARIANT_VIOLATIONS=$((INVARIANT_VIOLATIONS + 1)) && BOOT_FAIL=1
  done

  WM_TICKS=$(grep -c "\[WM\] tick" "$SERIAL_LOG" 2>/dev/null || echo 0)
  WM_TICKS=$((WM_TICKS + 0))
  [ "$WM_TICKS" -lt 3 ] && echo "[validate] GRUB FAIL: WM ticks < 3" && BOOT_FAIL=1

  {
    echo "----------------------------------------"
    echo "ROGUEOS — GRUB RUNTIME VERDICT"
    echo "----------------------------------------"
    echo "Boot: $([ $BOOT_FAIL -eq 0 ] && echo PASS || echo FAIL)"
    echo "Exceptions: $EXCEPTION_COUNT"
    echo "Invariant violations: $INVARIANT_VIOLATIONS"
    echo "WM ticks: $WM_TICKS"
    echo "Overall: $([ $BOOT_FAIL -eq 0 ] && echo PASS || echo FAIL)"
  } | tee "$LOG_DIR/verdict_grub.txt"
  [ $BOOT_FAIL -eq 0 ] && exit 0 || exit 1
fi

# ---------- UEFI profile (default) ----------
BUILD_DIR="${BUILD_DIR:-$ROOT/build}"
UEFI_BOOT="${BUILD_DIR}/uefi-boot"
KERNEL_ELF="${UEFI_BOOT}/kernel.elf"
ESP_DISK="${ESP_DISK:-$BUILD_DIR/esp_disk.img}"
OVMF_CODE="${OVMF_CODE:-}"
OVMF_VARS="${OVMF_VARS:-$HOME/.ovmf/OVMF_VARS.fd}"
for p in /usr/share/edk2/x64/OVMF_CODE.4m.fd /usr/share/edk2/ovmf/x64/OVMF_CODE.fd /usr/share/OVMF/OVMF_CODE.fd; do
  [ -z "$OVMF_CODE" ] && [ -f "$p" ] && OVMF_CODE="$p" && break
done

LOG_DIR="$BUILD_DIR/validate_logs"
mkdir -p "$LOG_DIR"
SERIAL_LOG="${SERIAL_LOG:-$BUILD_DIR/runtime_serial.log}"
QEMU_DEBUG_LOG="${QEMU_DEBUG_LOG:-$BUILD_DIR/runtime_qemu_debug.log}"
SUMMARY="$LOG_DIR/summary.txt"
HASH_LOG="$BUILD_DIR/kernel_hashes.txt"

# ---------- Rebuild ----------
if [ "${SKIP_REBUILD:-0}" != "1" ]; then
  echo "[validate] Rebuild: running scripts/build_os.sh..."
  "$ROOT/scripts/build_os.sh" || { echo "[validate] ERROR: build failed."; exit 1; }
  if [ ! -f "$KERNEL_ELF" ]; then
    echo "[validate] ERROR: kernel not found at $KERNEL_ELF"
    exit 1
  fi
  BUILD_HASH="$(sha256sum "$KERNEL_ELF" | awk '{print $1}')"
  echo "[validate] build hash: $BUILD_HASH"
else
  if [ ! -f "$KERNEL_ELF" ]; then
    echo "[validate] ERROR: SKIP_REBUILD=1 but $KERNEL_ELF not found. Run full validation without SKIP_REBUILD."
    exit 1
  fi
  BUILD_HASH="$(sha256sum "$KERNEL_ELF" | awk '{print $1}')"
  echo "[validate] (SKIP_REBUILD) build hash: $BUILD_HASH"
fi

# ---------- Update ESP (delete existing, recreate from scratch) ----------
if [ "${SKIP_REBUILD:-0}" != "1" ]; then
  echo "[validate] Deleting existing ESP disk (enforce artifact integrity)..."
  rm -f "$ESP_DISK"
  echo "[validate] Recreate ESP: running ./buildhall/esp_disk.sh (no sudo required)..."
  "$ROOT/buildhall/esp_disk.sh" || { echo "[validate] ERROR: esp_disk.sh failed."; exit 1; }
fi

if [ ! -f "$ESP_DISK" ]; then
  echo "[validate] ERROR: $ESP_DISK not found. Run: sudo ./buildhall/esp_disk.sh"
  exit 1
fi

# ---------- SHA256 verification (kernel on disk must match build) ----------
echo "[validate] Verifying kernel hash on ESP disk..."
ESP_HASH="$("$ROOT/buildhall/esp_hash_kernel.sh")" || {
  echo "[validate] ERROR: esp_hash_kernel.sh failed (could not read kernel from disk)."
  exit 1
}
RUNTIME_HASH="$ESP_HASH"

echo "[validate] SHA256 build/uefi-boot/kernel.elf:    $BUILD_HASH"
echo "[validate] SHA256 mounted ESP kernel.elf:      $ESP_HASH"
echo "$BUILD_HASH  $KERNEL_ELF" > "$BUILD_DIR/kernel.hash"
echo "$ESP_HASH  /mnt/kernel.elf" > "$BUILD_DIR/esp.hash"
{
  echo "build/uefi-boot/kernel.elf: $BUILD_HASH"
  echo "mounted ESP kernel.elf:     $ESP_HASH"
} > "$HASH_LOG"

if [ "$BUILD_HASH" != "$ESP_HASH" ]; then
  echo ""
  echo "[validate] *** ARTIFACT MISMATCH — ABORT ***"
  echo "[validate] Refusing to run QEMU. Hashes differ."
  echo "[validate] build hash:    $BUILD_HASH"
  echo "[validate] esp_disk hash: $ESP_HASH"
  echo ""
  exit 1
fi
echo "[validate] Hash match: kernel on disk matches build."

# ---------- SECTION 2: ESP structure verification (abort if any check fails) ----------
echo "[validate] Verifying ESP structure (FAT32, partition type, GPT)..."
MTOOLSRC_ESP="$(mktemp -t mtoolsrc.XXXXXX)"
echo "drive e: file=\"$ESP_DISK\" offset=$((2048 * 512))" > "$MTOOLSRC_ESP"
export MTOOLSRC="$MTOOLSRC_ESP"
if ! mdir e: 2>/dev/null | grep -q -E "EFI|kernel"; then
  echo "[validate] FAIL: ESP FAT32 root does not show EFI or kernel (mdir e:)"
  mdir e: 2>/dev/null || true
  rm -f "$MTOOLSRC_ESP"; unset MTOOLSRC
  exit 1
fi
if ! mdir e:/EFI/BOOT 2>/dev/null | grep -qi "BOOTX64"; then
  echo "[validate] FAIL: ESP missing /EFI/BOOT/BOOTX64.EFI"
  rm -f "$MTOOLSRC_ESP"; unset MTOOLSRC
  exit 1
fi
if ! mdir e: 2>/dev/null | grep -qi "kernel"; then
  echo "[validate] FAIL: ESP missing /kernel.elf"
  rm -f "$MTOOLSRC_ESP"; unset MTOOLSRC
  exit 1
fi
PART_TYPE="$(sgdisk -i 1 "$ESP_DISK" 2>/dev/null || true)"
if ! echo "$PART_TYPE" | grep -qi "EFI system partition\|EFI System"; then
  echo "[validate] FAIL: Partition 1 type is not EFI System. Got: $PART_TYPE"
  sgdisk -i 1 "$ESP_DISK" 2>/dev/null || true
  rm -f "$MTOOLSRC_ESP"; unset MTOOLSRC
  exit 1
fi
if ! sgdisk -v "$ESP_DISK" >/dev/null 2>&1; then
  echo "[validate] FAIL: GPT validation failed (sgdisk -v)"
  sgdisk -v "$ESP_DISK" 2>&1 || true
  rm -f "$MTOOLSRC_ESP"; unset MTOOLSRC
  exit 1
fi
rm -f "$MTOOLSRC_ESP"
unset MTOOLSRC
echo "[validate] ESP structure OK: FAT32, EF00, GPT valid."

# ---------- OVMF ----------
if [ ! -f "$OVMF_CODE" ]; then
  echo "[validate] ERROR: OVMF not found. Set OVMF_CODE or install edk2-ovmf."
  exit 1
fi
if [ ! -f "$OVMF_VARS" ]; then
  mkdir -p "$(dirname "$OVMF_VARS")"
  cp /usr/share/edk2/x64/OVMF_VARS.4m.fd "$OVMF_VARS" 2>/dev/null || cp /usr/share/edk2/ovmf/x64/OVMF_VARS.fd "$OVMF_VARS" 2>/dev/null || true
fi

# ---------- SECTION 1: AHCI (SATA) for firmware-stable boot; no virtio ----------
echo "[validate] Booting from ESP disk (AHCI): $ESP_DISK"
echo "[validate] Starting QEMU in maximum debug mode (60s observation)..."
echo "[validate] Serial -> $SERIAL_LOG"
echo "[validate] QEMU debug -> $QEMU_DEBUG_LOG"
rm -f "$SERIAL_LOG" "$QEMU_DEBUG_LOG"

# Use gtk display if DISPLAY set; otherwise none (e.g. headless validation).
[ -n "$DISPLAY" ] && QEMU_DISPLAY="-display gtk" || QEMU_DISPLAY="-display none"
# Run QEMU: AHCI + -boot order=c for deterministic auto-boot. Monitor 65s.
(
  qemu-system-x86_64 \
    -machine q35 \
    -cpu qemu64,+sse4.2,+avx \
    -smp 2 \
    -m 4096 \
    -device ahci,id=ahci0 \
    -drive id=espdisk,file="$ESP_DISK",format=raw,if=none \
    -device ide-hd,drive=espdisk,bus=ahci0.0 \
    -boot order=c \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
    -drive if=pflash,format=raw,file="${OVMF_VARS}" \
    -serial "file:$SERIAL_LOG" \
    $QEMU_DISPLAY \
    -no-reboot \
    -no-shutdown \
    -d int,guest_errors \
    -D "$QEMU_DEBUG_LOG"
) &
QEMU_PID=$!

# Also tee serial to stdout for live view (QEMU writes to file; we tail it)
sleep 2
tail -f "$SERIAL_LOG" 2>/dev/null &
TAIL_PID=$!

# Wait 65 seconds (60s monitoring + buffer)
sleep 65

kill $TAIL_PID 2>/dev/null || true
kill $QEMU_PID 2>/dev/null || true
wait $QEMU_PID 2>/dev/null || true

echo "[validate] QEMU stopped. Analyzing..."

# Abort immediately if firmware did not boot (no [BOOT] or no kernel entry)
if ! grep -q "\[BOOT\]" "$SERIAL_LOG" 2>/dev/null; then
  echo "[validate] FAIL: Firmware boot did not reach bootloader. No [BOOT] in serial log."
  echo "[validate] First 50 lines of serial:"
  head -50 "$SERIAL_LOG" 2>/dev/null || true
  exit 1
fi
if ! grep -q "\[KRN\] kernel_main_entry" "$SERIAL_LOG" 2>/dev/null; then
  echo "[validate] FAIL: Kernel did not start. No [KRN] kernel_main_entry in serial log."
  exit 1
fi

# Kernel artifact check: correct kernel must log use_kernel_cr3 (not alloc_addr_space)
if grep -q "user_create: use_kernel_cr3" "$SERIAL_LOG" 2>/dev/null; then
  echo "[validate] Correct kernel on disk: user_create: use_kernel_cr3"
else
  if grep -q "user_create: alloc_addr_space" "$SERIAL_LOG" 2>/dev/null; then
    echo "[validate] WRONG KERNEL ON DISK"
    exit 1
  fi
fi

# ---------- SECTION 5: Boot phase validation (in order) ----------
BOOT_FAIL=0
for tag in "[BOOT]" "kernel_main_entry" "step2 heap_ready" "step6 register_programs" "step7" "user_init_spawn" "[INIT] steward start" "[INIT] director start" "[INIT] throne start" "[WM] tick"; do
  if ! grep -q "$tag" "$SERIAL_LOG" 2>/dev/null; then
    echo "FAIL: Boot phase missing: $tag"
    BOOT_FAIL=1
  fi
done
[ $BOOT_FAIL -eq 0 ] && echo "PASS: Boot phase sequence present"

# ---------- SECTION 6: Exception scan ----------
EXCEPTION_COUNT=0
if [ -f "$QEMU_DEBUG_LOG" ]; then
  EXCEPTION_COUNT=$(grep -E "check_exception.*new 0x(6|8|d)|Triple fault|invalid opcode" "$QEMU_DEBUG_LOG" 2>/dev/null | wc -l)
  EXCEPTION_COUNT=$((EXCEPTION_COUNT + 0))
  if [ "$EXCEPTION_COUNT" -gt 0 ]; then
    echo "FAIL: CPU exception(s) in qemu-debug.log (count: $EXCEPTION_COUNT)"
    grep -n -E "check_exception.*new 0x(6|8|d)|Triple fault|invalid opcode" "$QEMU_DEBUG_LOG" 2>/dev/null | head -5
    FIRST_LINE=$(grep -n -E "check_exception.*new 0x(6|8|d)|Triple fault|invalid opcode" "$QEMU_DEBUG_LOG" 2>/dev/null | head -1 | cut -d: -f1)
    [ -n "$FIRST_LINE" ] && sed -n "${FIRST_LINE},$((FIRST_LINE+3))p" "$QEMU_DEBUG_LOG" 2>/dev/null | grep -E "pc=|RIP=" | head -1
    BOOT_FAIL=1
  fi
fi

# ---------- SECTION 7: WM liveness (tick counter increases) ----------
WM_TICKS=$(grep -c "\[WM\] tick" "$SERIAL_LOG" 2>/dev/null | tr -d ' \n' || echo 0)
WM_TICKS=$((WM_TICKS + 0))
if [ "$WM_TICKS" -lt 3 ]; then
  echo "FAIL: WM not running or stalled (fewer than 3 [WM] tick lines)"
  BOOT_FAIL=1
else
  echo "PASS: WM ticks seen ($WM_TICKS)"
fi

# ---------- SECTION 8: Memory / invariant scan ----------
INVARIANT_VIOLATIONS=0
for pattern in "canary" "buddy_oob" "double_free" "page_fault_unhandled" "diagnostic_halt" "KERNEL PANIC"; do
  if grep -qi "$pattern" "$SERIAL_LOG" 2>/dev/null; then
    echo "FAIL: Invariant violation: $pattern found in serial log"
    grep -n -i "$pattern" "$SERIAL_LOG" 2>/dev/null | head -3
    INVARIANT_VIOLATIONS=$((INVARIANT_VIOLATIONS + 1))
    BOOT_FAIL=1
  fi
done
[ $INVARIANT_VIOLATIONS -eq 0 ] && [ $BOOT_FAIL -eq 0 ] && echo "PASS: No invariant violations in serial"

# Freeze: no [WM] tick implies possible freeze (already covered above)
FREEZE_DETECTED="NO"
if [ "$WM_TICKS" -lt 3 ] && grep -q "\[INIT\] throne start" "$SERIAL_LOG" 2>/dev/null; then
  FREEZE_DETECTED="YES"
fi

LINES=$(wc -l < "$SERIAL_LOG" 2>/dev/null || echo 0)

# ---------- SECTION 9: Verdict output ----------
{
  echo "----------------------------------------"
  echo "ROGUEOS — FIRMWARE + RUNTIME VERDICT"
  echo "----------------------------------------"
  echo ""
  echo "Boot: $([ $BOOT_FAIL -eq 0 ] && echo PASS || echo FAIL)"
  echo "Kernel entry: PASS"
  echo "Userland started: $(grep -q '\[INIT\] steward start' "$SERIAL_LOG" 2>/dev/null && echo PASS || echo FAIL)"
  echo "WM running: $([ "$WM_TICKS" -ge 3 ] 2>/dev/null && echo PASS || echo FAIL)"
  echo "Exceptions detected: $EXCEPTION_COUNT"
  echo "Invariant violations: $INVARIANT_VIOLATIONS"
  echo "Freeze detected: $FREEZE_DETECTED"
  if [ $BOOT_FAIL -eq 0 ] && [ $EXCEPTION_COUNT -eq 0 ] && [ $INVARIANT_VIOLATIONS -eq 0 ] && [ "$WM_TICKS" -ge 3 ]; then
    echo "Overall status: PASS"
  else
    echo "Overall status: FAIL"
  fi
  echo ""
  echo "----------------------------------------"
  echo "Serial log lines: $LINES"
  echo "build hash: $BUILD_HASH"
  echo "esp hash: $ESP_HASH"
} | tee "$LOG_DIR/verdict.txt"

# Exit with failure if any check failed
if [ $BOOT_FAIL -ne 0 ]; then
  exit 1
fi
if [ $EXCEPTION_COUNT -gt 0 ] || [ $INVARIANT_VIOLATIONS -gt 0 ]; then
  exit 1
fi
exit 0
