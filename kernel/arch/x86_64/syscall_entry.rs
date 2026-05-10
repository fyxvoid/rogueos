//! SYSCALL/SYSRET entry. On SYSCALL the CPU does not switch stack; we load kernel RSP from
//! a global set before entering user. Args: rax=num, rdi,rsi,rdx,r10,r8,r9 (r10 = 4th arg; rcx/r11 overwritten by CPU).

/// Kernel RSP for SYSCALL entry. Set by context::enter_user so we switch to the current process's kernel stack.
#[no_mangle]
static mut __syscall_kernel_rsp: u64 = 0;

/// User RSP saved on every SYSCALL entry (before stack switch). Used for blocking syscalls that
/// need to save and restore the full user context via IRETQ.
#[no_mangle]
static mut __syscall_user_rsp: u64 = 0;

/// Pointer to the saved SyscallRegs on the kernel stack (set in syscall_entry_rust before dispatch).
/// Layout: [rax+0][rdi+8][rsi+16][rdx+24][r10+32][r8+40][r9+48][r11+56][rcx+64]
static mut CURRENT_SYSCALL_REGS_PTR: usize = 0;

/// Set the kernel stack pointer used when entering from SYSCALL. Call before enter_user (e.g. in context::enter_user).
pub fn set_current_kernel_rsp(rsp: u64) {
    unsafe {
        __syscall_kernel_rsp = rsp;
    }
}

/// Return the user RSP saved on the most recent SYSCALL entry.
#[inline]
pub fn get_user_rsp() -> u64 {
    unsafe { __syscall_user_rsp }
}

/// Return user RIP (value of RCX at SYSCALL = return address in user code).
#[inline]
pub fn get_user_rip() -> u64 {
    let ptr = unsafe { CURRENT_SYSCALL_REGS_PTR };
    if ptr == 0 { return 0; }
    unsafe { *((ptr + 64) as *const u64) }
}

/// Return user RFLAGS (value of R11 at SYSCALL).
#[inline]
pub fn get_user_rflags() -> u64 {
    let ptr = unsafe { CURRENT_SYSCALL_REGS_PTR };
    if ptr == 0 { return 0x202; }
    unsafe { *((ptr + 56) as *const u64) }
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
        CURRENT_SYSCALL_REGS_PTR = regs as usize;
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
        mov [{user_rsp}], rsp
        mov rsp, [{kern_rsp}]
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
        call {entry}
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
        mov rsp, [{user_rsp}]
        sysretq
        "#,
        user_rsp = sym __syscall_user_rsp,
        kern_rsp = sym __syscall_kernel_rsp,
        entry = sym syscall_entry_rust,
        options(nostack, noreturn)
    );
}
