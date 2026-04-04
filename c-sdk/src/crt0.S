/* crt0.S — minimal C runtime start for Kingdom OS (x86-64 bare metal).
 *
 * The kernel jumps to _start with a clean stack.  We zero BSS, then call
 * kmain().  kmain() is the user entry point (replaces main()).
 *
 * Link last: x86_64-elf-gcc ... crt0.o libk.o app.o -o app.elf
 */

    .section .text
    .global _start
    .type   _start, @function
_start:
    /* Clear the stack frame pointer for clean backtraces. */
    xor %rbp, %rbp

    /* Zero BSS.  __bss_start and __bss_end are provided by the linker script. */
    lea __bss_start(%rip), %rdi
    lea __bss_end(%rip),   %rcx
    sub %rdi, %rcx          /* rcx = byte count */
    jz  .Lbss_done
    xor %eax, %eax
    rep stosb
.Lbss_done:

    /* Call the application entry point. */
    call kmain

    /* If kmain() returns, call SYS_EXIT(0). */
    mov $0x109, %rax        /* SYS_EXIT */
    xor %rdi, %rdi
    syscall
    /* Never reached. */
    hlt
    .size _start, . - _start
