//! SYSCALL/SYSRET entry. On SYSCALL the CPU does not switch stack; we load kernel RSP from
//! a global set before entering user. Args: rax=num, rdi,rsi,rdx,r10,r8,r9 (r10 = 4th arg; rcx/r11 overwritten by CPU).

/// Kernel RSP for SYSCALL entry. Set by context::enter_user so we switch to the current process's kernel stack.
#[no_mangle]
static mut __syscall_kernel_rsp: u64 = 0;

/// Set the kernel stack pointer used when entering from SYSCALL. Call before enter_user (e.g. in context::enter_user).
pub fn set_current_kernel_rsp(rsp: u64) {
    unsafe {
        __syscall_kernel_rsp = rsp;
    }
}

/// Saved GPRs for dispatch. Order matches layout on stack: rax, rdi, rsi, rdx, r10, r8, r9 (4th arg is r10, not rcx).
#[repr(C)]
pub struct SyscallRegs {
    pub rax: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub r10: u64,
    pub r8: u64,
    pub r9: u64,
}

/// Called from asm with rdi = pointer to saved regs. Returns value to put in user rax.
#[no_mangle]
pub extern "C" fn syscall_entry_rust(regs: *mut SyscallRegs) -> u64 {
    unsafe {
        let r = &*regs;
        crate::syscall::syscall_dispatch(
            r.rax,
            r.rdi,
            r.rsi,
            r.rdx,
            r.r10, // 4th arg from r10 (rcx is user RIP)
            r.r8,
            r.r9,
        )
    }
}

/// SYSCALL entry point (LSTAR). Switch to kernel stack, save regs, call Rust, restore, sysretq.
#[no_mangle]
pub unsafe extern "C" fn syscall_entry() {
    core::arch::asm!(
        r#"
        mov rsp, [{}]
        sub rsp, 80
        mov [rsp], rax
        mov [rsp + 8], rdi
        mov [rsp + 16], rsi
        mov [rsp + 24], rdx
        mov [rsp + 32], r10
        mov [rsp + 40], r8
        mov [rsp + 48], r9
        mov [rsp + 56], r11
        mov [rsp + 64], rcx
        mov rdi, rsp
        call {}
        mov [rsp], rax
        pop rax
        pop rdi
        pop rsi
        pop rdx
        pop r10
        pop r8
        pop r9
        pop r11
        pop rcx
        sysretq
        "#,
        sym __syscall_kernel_rsp,
        sym syscall_entry_rust,
        options(nostack, noreturn)
    );
}
