# Kingdom OS: single build entry point.
# Build order: lib -> userland -> kernel -> boot.
# Usage: make all   (or make lib userland kernel boot)

ROOT := $(CURDIR)
CARGO_BUILD_JOBS ?= 1
export CARGO_BUILD_JOBS

RUSTFLAGS_USERLAND := -C relocation-model=static -C link-arg=-no-pie
RUSTFLAGS_KERNEL := -C relocation-model=static -C link-arg=-no-pie
TARGET_NONE := x86_64-unknown-none
TARGET_UEFI := x86_64-unknown-uefi

.PHONY: lib userland kernel boot all image run debug

lib:
	cargo build -p libs

userland: lib
	RUSTFLAGS="$(RUSTFLAGS_USERLAND)" cargo build -p userland --release --target $(TARGET_NONE)

kernel: userland
	RUSTFLAGS="$(RUSTFLAGS_KERNEL)" cargo build -p kernel --release --target $(TARGET_NONE) --bin kernel

boot: lib
	cargo build -p boot --release --target $(TARGET_UEFI)

all: lib userland kernel boot
	@echo "Build complete: lib, userland, kernel, boot."

image: all
	@mkdir -p $(ROOT)/build/uefi-boot/EFI/boot
	@cp $(ROOT)/target/$(TARGET_UEFI)/release/boot.efi $(ROOT)/build/uefi-boot/EFI/boot/bootx64.efi 2>/dev/null || cp $(ROOT)/target/$(TARGET_UEFI)/debug/boot.efi $(ROOT)/build/uefi-boot/EFI/boot/bootx64.efi
	@cp $(ROOT)/target/$(TARGET_NONE)/release/kernel $(ROOT)/build/uefi-boot/kernel.elf
	@echo "UEFI image ready in build/uefi-boot/"

run: image
	@SKIP_BUILD=1 ./scripts/run_qemu.sh

debug: image
	@echo "Starting QEMU with GDB server (port 1234). In another terminal run:"
	@echo "  rust-gdb -ex 'target remote :1234' -ex 'symbol-file $(ROOT)/target/$(TARGET_NONE)/release/kernel' $(ROOT)/target/$(TARGET_NONE)/release/kernel"
	@echo "  (or: gdb -ex 'target remote :1234' -ex 'symbol-file .../kernel' .../kernel)"
	@QEMU_GDB=1 SKIP_BUILD=1 ./scripts/run_qemu.sh
