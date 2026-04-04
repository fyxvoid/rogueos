/* Multiboot2 bootstrap: SeaBIOS -> GRUB (32-bit) -> this stub -> long mode -> kernel_main.
   Entry is 32-bit protected mode. We set stack, GDT, PAE, 4-level paging, then far jump to 64-bit. */

#define MB2_MAGIC      0xE85250D6
#define MB2_ARCH       0
#define MB2_HEADER_LEN 24
#define MB2_CHECKSUM   (0 - (MB2_MAGIC + MB2_ARCH + MB2_HEADER_LEN))

/* Multiboot2 header: in first 32KB, 8-byte aligned. Required for GRUB. */
.section .multiboot2, "a", @progbits
.align 8
.Lmultiboot2_header:
    .long MB2_MAGIC
    .long MB2_ARCH
    .long MB2_HEADER_LEN
    .long MB2_CHECKSUM
    /* End tag: type 0, size 8 (u32, u32) */
    .long 0
    .long 8

.section .text.boot, "ax", @progbits
.code32
.globl multiboot2_start
.type multiboot2_start, @function
multiboot2_start:
    /* 1. Disable interrupts before anything else */
    cli
    /* 2. Set up stack (must be valid before any call/push; identity-mapped later) */
    movl $boot_stack_end, %esp
    xorl %ebp, %ebp
    /* 3. Save multiboot2 info (EBX = physical addr of multiboot2 info) */
    movl %ebx, multiboot2_mbi
    /* 4. Load GDT *before* enabling paging (CPU must have valid GDT for far jump) */
    lgdt boot_gdt_desc
    /* 5. Enable PAE (CR4.PAE = bit 5) */
    movl %cr4, %eax
    orl $0x20, %eax
    movl %eax, %cr4
    /* 6. Load CR3 with PML4 physical address (identity-mapped in .data) */
    movl $boot_pml4, %eax
    movl %eax, %cr3
    /* 7. Set EFER.LME (long mode enable) via MSR 0xC0000080 */
    movl $0xC0000080, %ecx
    rdmsr
    orl $0x100, %eax
    wrmsr
    /* 8. Enable paging (CR0.PG = bit 31) */
    movl %cr0, %eax
    orl $0x80000000, %eax
    movl %eax, %cr0
    /* 9. Far jump to 64-bit code segment (selector 0x18 = GDT entry 3, 64-bit code); enters long mode */
    ljmp $0x18, $long_mode_entry

.Lhalt32:
    hlt
    jmp .Lhalt32

.code64
.align 8
long_mode_entry:
    /* 10. Reload segment regs for 64-bit (selector 0x20 = GDT entry 4, 64-bit data) */
    movw $0x20, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %fs
    movw %ax, %gs
    movw %ax, %ss
    /* 11. Set up kernel stack (linker-provided; identity-mapped) */
    movabs $_stack_top, %rsp
    xorl %ebp, %ebp
    /* 12. Zero .bss (kernel expects it) */
    movabs $_bss_start, %rdi
    movabs $_bss_end, %rcx
    subq %rdi, %rcx
    xorl %eax, %eax
    shrq $3, %rcx
    rep stosq
    /* 13. RDI = multiboot2 info physical address (saved in 32-bit) */
    movabs $multiboot2_mbi, %rsi
    movl (%rsi), %edi
    /* 14. Call Rust; never returns */
    call multiboot2_entry
    hlt
    jmp .

/* Stack and page tables in dedicated sections so layout is explicit */
.section .bss.boot_stack, "aw", @nobits
.align 16
boot_stack:
    .space 0x4000
boot_stack_end:

/* Page tables: identity map first 2MB (4KB pages). Must be in writable section. */
.section .data.boot_pt, "aw", @progbits
.align 4096
boot_pml4:
    .quad boot_pdpt + 3
    .rept 511
    .quad 0
    .endr
boot_pdpt:
    .quad boot_pd + 3
    .rept 511
    .quad 0
    .endr
boot_pd:
    .quad boot_pt + 3
    .rept 511
    .quad 0
    .endr
boot_pt:
    .set i, 0
    .rept 512
    .quad (i * 4096) + 3
    .set i, i + 1
    .endr

/* GDT: null, 32-bit code, 32-bit data, 64-bit code, 64-bit data. Loaded before paging. */
.section .data.boot_gdt, "aw", @progbits
.align 8
boot_gdt:
    .quad 0
    .quad 0x00CF9A000000FFFF   /* 32-bit code */
    .quad 0x00CF92000000FFFF   /* 32-bit data */
    .quad 0x00AF9A000000FFFF   /* 64-bit code */
    .quad 0x00AF92000000FFFF   /* 64-bit data */
boot_gdt_desc:
    .short . - boot_gdt - 1
    .long boot_gdt

.section .data.boot_mbi, "aw", @progbits
.align 4
multiboot2_mbi:
    .long 0
