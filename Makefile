# RogueOS: single build entry point.
# Build order: userland -> kernel -> boot.
# Usage: make all   (or make userland kernel boot)

ROOT := $(CURDIR)
CARGO_BUILD_JOBS ?= 1
export CARGO_BUILD_JOBS

RUSTFLAGS_USERLAND := -C relocation-model=static -C link-arg=-no-pie
RUSTFLAGS_KERNEL := -C relocation-model=static -C link-arg=-no-pie
TARGET_NONE := x86_64-unknown-none
TARGET_UEFI := x86_64-unknown-uefi
USERLAND_LD := $(ROOT)/userland/ldscripts

.PHONY: userland kernel boot all image run debug force-rebuild

# Build one userland binary with its specific linker script.
# Each binary gets a unique virtual load address so processes can coexist
# in the shared CR3 address space without overlapping page mappings.
define BUILD_BIN
	RUSTFLAGS="$(RUSTFLAGS_USERLAND) -C link-arg=-T$(2)" cargo build -p userland --release --target $(TARGET_NONE) --bin $(1)
endef

userland:
	$(call BUILD_BIN,cogman,$(USERLAND_LD)/cogman.ld)
	$(call BUILD_BIN,init,$(USERLAND_LD)/init.ld)
	$(call BUILD_BIN,shell,$(USERLAND_LD)/shell.ld)
	$(call BUILD_BIN,fbtest,$(USERLAND_LD)/fbtest.ld)
	$(call BUILD_BIN,session,$(USERLAND_LD)/session.ld)
	$(call BUILD_BIN,wm,$(USERLAND_LD)/wm.ld)
	$(call BUILD_BIN,editor,$(USERLAND_LD)/editor.ld)
	$(call BUILD_BIN,viewer,$(USERLAND_LD)/viewer.ld)
	$(call BUILD_BIN,copy,$(USERLAND_LD)/copy.ld)
	$(call BUILD_BIN,monitor,$(USERLAND_LD)/monitor.ld)
	$(call BUILD_BIN,shutdown,$(USERLAND_LD)/shutdown.ld)
	$(call BUILD_BIN,exit,$(USERLAND_LD)/exit.ld)
	$(call BUILD_BIN,rwm,$(USERLAND_LD)/rwm.ld)
	$(call BUILD_BIN,terminal,$(USERLAND_LD)/terminal.ld)

kernel: userland
	@touch $(ROOT)/kernel/build.rs
	RUSTFLAGS="$(RUSTFLAGS_KERNEL)" cargo build -p kernel --release --target $(TARGET_NONE) --bin kernel

boot:
	cargo build -p boot --release --target $(TARGET_UEFI)

all: userland kernel boot
	@echo "Build complete: userland, kernel, boot."

image: all
	@mkdir -p $(ROOT)/build/uefi-boot/EFI/boot
	@cp $(ROOT)/target/$(TARGET_UEFI)/release/boot.efi $(ROOT)/build/uefi-boot/EFI/boot/bootx64.efi 2>/dev/null || cp $(ROOT)/target/$(TARGET_UEFI)/debug/boot.efi $(ROOT)/build/uefi-boot/EFI/boot/bootx64.efi
	@cp $(ROOT)/target/$(TARGET_NONE)/release/kernel $(ROOT)/build/uefi-boot/kernel.elf
	@printf '@echo -off\r\nfs0:\r\n\\EFI\\boot\\bootx64.efi\r\n' > $(ROOT)/build/uefi-boot/startup.nsh
	@echo "UEFI image ready in build/uefi-boot/"

run: image
	@SKIP_BUILD=1 ./scripts/run_qemu.sh

force-rebuild:
	@touch $(ROOT)/kernel/build.rs
	@$(MAKE) image

debug: image
	@echo "Starting QEMU with GDB server (port 1234). In another terminal run:"
	@echo "  rust-gdb -ex 'target remote :1234' -ex 'symbol-file $(ROOT)/target/$(TARGET_NONE)/release/kernel' $(ROOT)/target/$(TARGET_NONE)/release/kernel"
	@echo "  (or: gdb -ex 'target remote :1234' -ex 'symbol-file .../kernel' .../kernel)"
	@QEMU_GDB=1 SKIP_BUILD=1 ./scripts/run_qemu.sh
