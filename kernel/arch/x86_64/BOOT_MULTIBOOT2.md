# Multiboot2 boot path (GRUB + SeaBIOS)

## Why "failed to boot both default and fallback entries" was happening

GRUB reports that when the **machine resets** (triple fault) before returning to the bootloader. The CPU triple-faults, the platform resets, and GRUB treats that as a failed boot. So the failure was in our 32→64 transition, not in finding the kernel.

Likely causes that were fixed:

1. **GDT loaded after paging** — The GDT was loaded with `lgdt` *after* enabling CR0.PG. The far jump into the 64-bit code segment must see a valid GDT that was installed while the CPU was still in a consistent state. **Fix:** Load GDT before enabling paging (after stack, before PAE/CR3/EFER/CR0.PG).

2. **No interrupts disabled** — With paging/GDT in flux, an interrupt could fire and use a bad IDT/GDT or unmapped stack. **Fix:** `cli` at the very first instruction.

3. **64-bit path not setting stack / .bss** — The kernel expects a valid stack and zeroed .bss. The stub was still on the small bootstrap stack and never zeroed .bss. **Fix:** In long mode, set RSP to `_stack_top`, zero `[_bss_start, _bss_end)` (by quadwords), then call `multiboot2_entry`.

4. **Multiboot2 end tag** — End tag must be `u32 type=0, u32 size=8`. Using `.short` for type was wrong. **Fix:** `.long 0` and `.long 8`.

5. **Section order** — Linker now places `.multiboot2` then `.text.boot` then rest of `.text`, so the Multiboot2 header is in the first 32 KB and the 32-bit entry is immediately after it.

## Boot flow (unchanged)

- SeaBIOS → GRUB (Multiboot2) → 32-bit `multiboot2_start` → set stack, GDT, PAE, paging, EFER.LME → far jump to 64-bit `long_mode_entry` → reload segments, set RSP, zero .bss → `multiboot2_entry(mbi)` → `kernel_main(bootinfo)`.

## Files

- **boot_multiboot2.S** — 32-bit entry, GDT, identity paging (first 2 MB), far jump; 64-bit segment reload, stack, .bss zeroing, call to Rust.
- **linker_multiboot2.ld** — ENTRY(multiboot2_start); section order .multiboot2, .text.boot, .text, .rodata, .data, .bss.
- **multiboot2_parser.rs** — Parses multiboot2 tags, fills BootInfo at 0x8000, calls `kernel_main`.

## QEMU debug

Use `-no-reboot -no-shutdown -d int` so that on triple fault you get interrupt/exception logging instead of an immediate reset.
