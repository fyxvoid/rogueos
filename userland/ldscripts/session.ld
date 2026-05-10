OUTPUT_FORMAT("elf64-x86-64")
ENTRY(_start)
SECTIONS {
    . = 0xC00000;
    .text : { *(.text .text.*) }
    .rodata : { *(.rodata .rodata.* .rodata1) }
    . = ALIGN(0x1000);
    .data : { *(.data .data.*) *(.sdata .sdata.*) *(.got .got.*) *(.got.plt) *(.init_array .init_array.*) *(.fini_array .fini_array.*) }
    . = ALIGN(0x1000);
    .bss (NOLOAD) : { *(.bss .bss.*) *(COMMON) }
    /DISCARD/ : { *(.comment) *(.note .note.*) *(.eh_frame .eh_frame_hdr) }
}
