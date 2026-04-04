/* Kernel linked at 1M. Multiboot2 header must be in first 32KB of image.
   Section order: .multiboot2, .text (stub then rest), .rodata, .data, .bss.
   Entry is 32-bit multiboot2_start; it switches to long mode and calls kernel_main. */
ENTRY(multiboot2_start)

SECTIONS
{
    . = 1M;
    _kernel_start = .;

    /* Multiboot2 header first (first 32KB); then 32/64-bit bootstrap */
    .text : ALIGN(4K) {
        *(.multiboot2)
        *(.text.boot)
        *(.text .text.*)
    }

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4K) {
        *(.data .data.*)
    }

    .bss : ALIGN(4K) {
        _bss_start = .;
        *(COMMON)
        *(.bss .bss.*)
        _bss_end = .;
        _stack_bottom = .;
        . += 64K;
        _stack_top = .;
    }

    _kernel_end = .;
}
