# Kingdom OS C SDK — Makefile
#
# Targets:
#   make               — build bare-metal libk.a + crt0.o (needs x86_64-elf-gcc)
#   make linux         — build Linux test shim library (uses host gcc)
#   make example-linux — build + run the hello example on Linux
#   make clean         — remove build artefacts

CC_BARE   ?= x86_64-elf-gcc
CC_LINUX  ?= gcc
AR_BARE   ?= x86_64-elf-ar

BARE_CFLAGS  = -std=c11 -ffreestanding -nostdlib -nostdinc \
               -I include -O2 -Wall -Wextra \
               -mno-red-zone -mno-mmx -mno-sse -mno-sse2
LINUX_CFLAGS = -std=c11 -DKINGDOM_LINUX -I include -O2 -Wall -Wextra

BUILD_BARE  = build/bare
BUILD_LINUX = build/linux

# ── Bare-metal targets ────────────────────────────────────────────────────

.PHONY: all
all: $(BUILD_BARE)/libk.a $(BUILD_BARE)/crt0.o

$(BUILD_BARE):
	mkdir -p $@

$(BUILD_BARE)/crt0.o: src/crt0.S | $(BUILD_BARE)
	$(CC_BARE) $(BARE_CFLAGS) -c $< -o $@

$(BUILD_BARE)/libk.o: src/libk.c | $(BUILD_BARE)
	$(CC_BARE) $(BARE_CFLAGS) -c $< -o $@

$(BUILD_BARE)/libk.a: $(BUILD_BARE)/libk.o
	$(AR_BARE) rcs $@ $^

# ── Linux shim ────────────────────────────────────────────────────────────

.PHONY: linux
linux: $(BUILD_LINUX)/kwm_linux.o $(BUILD_LINUX)/libk.o

$(BUILD_LINUX):
	mkdir -p $@

$(BUILD_LINUX)/kwm_linux.o: linux/kwm_linux.c | $(BUILD_LINUX)
	$(CC_LINUX) $(LINUX_CFLAGS) -c $< -o $@

$(BUILD_LINUX)/libk.o: src/libk.c | $(BUILD_LINUX)
	$(CC_LINUX) $(LINUX_CFLAGS) -c $< -o $@

# ── Example: hello (Linux host) ───────────────────────────────────────────

.PHONY: example-linux
example-linux: $(BUILD_LINUX)/hello
	./$<

$(BUILD_LINUX)/hello: examples/hello.c $(BUILD_LINUX)/kwm_linux.o $(BUILD_LINUX)/libk.o | $(BUILD_LINUX)
	$(CC_LINUX) $(LINUX_CFLAGS) -o $@ $^

# ── Clean ─────────────────────────────────────────────────────────────────

.PHONY: clean
clean:
	rm -rf build
