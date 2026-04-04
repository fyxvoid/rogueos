ENTRY(kernel_main)
SECTIONS
{
    . = 1M;
    _kernel_start = .;
    /* Multiboot header must be in first 8KB of loaded image; put it at start of .text */
    .text : ALIGN(4K) {
        *(.multiboot2)
        *(.text .text.*)
    }
    .rodata : ALIGN(4K) { *(.rodata .rodata.*) }
    .data : ALIGN(4K) { *(.data .data.*) }
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
