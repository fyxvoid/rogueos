//! Process lifecycle: spawn, exit, run first. Spawn-only; no fork/clone.

use crate::arch::x86_64::gdt;
use crate::memory::paging;
use crate::process::context;
use crate::process::loader;
use crate::process::pid;
use crate::process::process::{
    self, ProcessDescriptor, ProcessState, TrapFrame, USER_STACK_PAGES, USER_STACK_TOP,
};
use crate::process::scheduler;

const PAGE_SIZE: usize = 4096;

fn log_scheduler_tick(pid_opt: Option<process::Pid>, runqueue_len: usize) {
    crate::arch::serial::write_str("[sched] pid=");
    crate::arch::serial::write_hex(pid_opt.unwrap_or(0) as u64);
    crate::arch::serial::write_str(" rq_len=");
    crate::arch::serial::write_hex(runqueue_len as u64);
    crate::arch::serial::write_str("\r\n");
}

/// Simple checksum for ELF verification: XOR of all bytes (pre-load vs post-load).
fn elf_checksum(data: &[u8]) -> u64 {
    data.iter().fold(0u64, |acc, &b| acc ^ (b as u64))
}

/// Create the first user process: set up address space, load ELF, fill trap frame.
/// Returns the process table index or None on failure.
pub fn create_user_process(elf_data: &[u8]) -> Option<usize> {
    let pre_checksum = elf_checksum(elf_data);
    crate::arch::serial::write_str("[KRN] user_create: ELF checksum (pre-load)=");
    crate::arch::serial::write_hex(pre_checksum);
    crate::arch::serial::write_str("\r\n");
    crate::arch::serial::write_str("[KRN] user_create: alloc_slot\r\n");
    let (idx, pid_val) = pid::allocate_process_slot()?;
    crate::arch::serial::write_str("[KRN] user_create: use_kernel_cr3\r\n");
    let cr3 = crate::memory::paging::read_cr3();
    crate::arch::serial::write_str("[KRN] user_create: load_elf\r\n");
    let load = match loader::load_elf(elf_data, cr3) {
        Some(r) => r,
        None => {
            crate::arch::serial::write_str("[KRN] user_create: load_elf_failed\r\n");
            pid::release_slot(idx);
            return None;
        }
    };
    let entry = load.entry;
    // Post-load checksum: bytes at entry VA in memory vs elf_data[entry_file_offset..].
    if let Some(entry_off) = load.entry_file_offset {
        let file_rest = elf_data.len().saturating_sub(entry_off);
        let check_len = core::cmp::min(256, file_rest);
        if check_len > 0 {
            let entry_file_checksum = elf_checksum(&elf_data[entry_off..entry_off + check_len]);
            unsafe {
                let ptr = entry as *const u8;
                let mut post_buf = [0u8; 256];
                core::ptr::copy_nonoverlapping(ptr, post_buf.as_mut_ptr(), check_len);
                let post_checksum = elf_checksum(&post_buf[0..check_len]);
                crate::arch::serial::write_str("[KRN] user_create: ELF checksum (post-load, ");
                crate::arch::serial::write_hex(check_len as u64);
                crate::arch::serial::write_str(" bytes at entry)=");
                crate::arch::serial::write_hex(post_checksum);
                crate::arch::serial::write_str("\r\n");
                if entry_file_checksum != post_checksum {
                    crate::arch::serial::write_str("[KRN] user_create: checksum mismatch (stale/wrong binary)\r\n");
                    pid::release_slot(idx);
                    return None;
                }
            }
        }
    }

    // Verify entry (text) PTE: USER|PRESENT|executable; .text not writable. Dump PTE.
    let entry_pte = paging::walk_pte(cr3, entry);
    match entry_pte {
        Some(pte) => {
            let present = (pte & paging::PageFlag::Present as u64) != 0;
            let user = (pte & paging::PageFlag::User as u64) != 0;
            let nx = (pte & paging::PageFlag::NoExec as u64) != 0;
            let writable = (pte & paging::PageFlag::Writable as u64) != 0;
            if !present || !user {
                crate::arch::serial::write_str("[KRN] user_create: entry PTE missing PRESENT or USER\r\n");
                pid::release_slot(idx);
                return None;
            }
            if nx {
                crate::arch::serial::write_str("[KRN] user_create: entry page has NX (not executable)\r\n");
                pid::release_slot(idx);
                return None;
            }
            if writable {
                crate::arch::serial::write_str("[KRN] user_create: entry (.text) must not be writable\r\n");
                pid::release_slot(idx);
                return None;
            }
            crate::arch::serial::write_str("[KRN] user_create: entry PTE ok (P+U+X, not W)\r\n");
            paging::dump_ptes_for_vas_serial(cr3, &[entry & !(PAGE_SIZE as u64 - 1)]);
            unsafe {
                let ptr = entry as *const u8;
                crate::arch::serial::write_str("[KRN] user_create: first 16 bytes at entry: ");
                for i in 0..16 {
                    crate::arch::serial::write_hex(*ptr.add(i) as u64);
                    crate::arch::serial::write_str(" ");
                }
                crate::arch::serial::write_str("\r\n");
                let all_zero = (0..16).all(|i| *ptr.add(i) == 0);
                if all_zero {
                    crate::arch::serial::write_str("[KRN] user_create: WARN entry bytes all zero\r\n");
                }
            }
        }
        None => {
            crate::arch::serial::write_str("[KRN] user_create: entry not mapped\r\n");
            pid::release_slot(idx);
            return None;
        }
    }

    // Verify data segment PTE: USER|PRESENT|writable|NX. Dump PTE.
    if let Some(data_va) = load.data_page_va {
        let data_page = data_va & !(PAGE_SIZE as u64 - 1);
        if let Some(pte) = paging::walk_pte(cr3, data_page) {
            let nx = (pte & paging::PageFlag::NoExec as u64) != 0;
            let writable = (pte & paging::PageFlag::Writable as u64) != 0;
            if !nx {
                crate::arch::serial::write_str("[KRN] user_create: data page must be NX (not executable)\r\n");
                pid::release_slot(idx);
                return None;
            }
            if !writable {
                crate::arch::serial::write_str("[KRN] user_create: data page must be writable\r\n");
                pid::release_slot(idx);
                return None;
            }
            crate::arch::serial::write_str("[KRN] user_create: data PTE ok (P+U+W+NX)\r\n");
            paging::dump_ptes_for_vas_serial(cr3, &[data_page]);
        }
    }

    crate::arch::serial::write_str("[KRN] user_create: setup_user_stack\r\n");
    let Some(user_rsp) = process::setup_user_stack(cr3) else {
        crate::arch::serial::write_str("[KRN] user_create: setup_user_stack_failed\r\n");
        pid::release_slot(idx);
        return None;
    };
    crate::arch::serial::write_str("[KRN] user_create: alloc_kernel_stack\r\n");
    let kernel_stack = pid::alloc_kernel_stack(idx)?;
    // Ring 3: selectors must have RPL=3 so iretq sets CPL=3; otherwise CPL stays 0 and user pages (USER bit) are not accessible.
    let trap_frame = TrapFrame {
        rip: entry,
        cs: (gdt::USER_CS | 3) as u64,
        rflags: 0x202, // IF
        rsp: user_rsp,
        ss: (gdt::USER_SS | 3) as u64,
    };
    let pcb = ProcessDescriptor::new(
        pid_val,
        ProcessState::Runnable,
        cr3,
        kernel_stack,
        trap_frame,
    );
    pid::put_descriptor(idx, pcb);

    let entry_page = entry & !(PAGE_SIZE as u64 - 1);
    let stack_end = USER_STACK_TOP.saturating_sub(USER_STACK_PAGES as u64 * PAGE_SIZE as u64);
    let vas: [u64; 4] = [
        entry_page,
        entry_page + PAGE_SIZE as u64,
        USER_STACK_TOP - PAGE_SIZE as u64,
        stack_end,
    ];
    paging::dump_ptes_for_vas_serial(cr3, &vas);

    scheduler::enqueue_runqueue(idx);
    Some(idx)
}

/// Run the process at the given index: set current, mark Running, enter user.
/// Does not return until the process traps back (e.g. syscall).
pub fn run_first_process(process_index: usize) -> ! {
    let (cr3, kernel_stack, frame_ptr): (u64, u64, *const TrapFrame) = match pid::get_descriptor(process_index) {
        Some(slot) => {
            unsafe { pid::set_current(Some(process_index)); }
            (
                slot.cr3,
                slot.kernel_stack_top,
                &slot.trap_frame as *const TrapFrame,
            )
        }
        None => {
            crate::arch::serial::write_str("[run] invalid process index\r\n");
            crate::kernel::diagnostic::diagnostic_halt("run_first_invalid_index");
        }
    };
    if let Some(slot) = pid::get_descriptor_mut(process_index) {
        slot.state = ProcessState::Running;
    }
    log_scheduler_tick(pid::current_pid(), scheduler::runqueue_total_len());
    crate::arch::serial::write_str("[run] idx=");
    crate::arch::serial::write_hex(process_index as u64);
    crate::arch::serial::write_str(" pid=");
    crate::arch::serial::write_hex(pid::current_pid().unwrap_or(0) as u64); // log only; 0 if none
    crate::arch::serial::write_str(" cr3=");
    crate::arch::serial::write_hex(cr3);
    // SAFETY: frame_ptr is from process descriptor at process_index; we hold the index and just set current.
    let frame = unsafe { &*frame_ptr };
    crate::arch::serial::write_str(" rip=");
    crate::arch::serial::write_hex(frame.rip);
    crate::arch::serial::write_str(" rsp=");
    crate::arch::serial::write_hex(frame.rsp);
    crate::arch::serial::write_str(" cs=");
    crate::arch::serial::write_hex(frame.cs);
    crate::arch::serial::write_str(" ss=");
    crate::arch::serial::write_hex(frame.ss);
    crate::arch::serial::write_str(" kstack=");
    crate::arch::serial::write_hex(kernel_stack);
    crate::arch::serial::write_str("\r\n");

    // Verify user stack initial state before iretq.
    crate::arch::serial::write_str("[run] USER_STACK_TOP=");
    crate::arch::serial::write_hex(USER_STACK_TOP);
    crate::arch::serial::write_str(" USER_STACK_TOP-PAGE=");
    crate::arch::serial::write_hex(USER_STACK_TOP.saturating_sub(PAGE_SIZE as u64));
    crate::arch::serial::write_str("\r\n");
    // RSP == USER_STACK_TOP is valid: setup_user_stack() returns exactly USER_STACK_TOP
    // as the initial stack pointer (top of the allocated stack, grows downward on first push).
    if frame.rsp > USER_STACK_TOP {
        crate::arch::serial::write_str("[run] rsp > USER_STACK_TOP\r\n");
        crate::kernel::diagnostic::diagnostic_halt("user_rsp_invalid");
    }
    if (frame.rsp % 16) != 0 {
        crate::arch::serial::write_str("[run] rsp not 16-byte aligned\r\n");
        crate::kernel::diagnostic::diagnostic_halt("user_rsp_unaligned");
    }
    let rsp_minus_8 = frame.rsp.saturating_sub(8);
    let pte_rsp = paging::walk_pte(cr3, rsp_minus_8);
    match pte_rsp {
        Some(pte) => {
            if (pte & paging::PageFlag::Present as u64) == 0 {
                crate::kernel::diagnostic::diagnostic_halt("user_stack_not_present");
            }
            if (pte & paging::PageFlag::User as u64) == 0 {
                crate::kernel::diagnostic::diagnostic_halt("user_stack_not_user");
            }
            if (pte & paging::PageFlag::Writable as u64) == 0 {
                crate::kernel::diagnostic::diagnostic_halt("user_stack_not_writable");
            }
        }
        None => {
            crate::arch::serial::write_str("[run] rsp-8 not mapped\r\n");
            crate::kernel::diagnostic::diagnostic_halt("user_stack_unmapped");
        }
    }
    const STACK_TEST_PATTERN: u64 = 0xDEADBEEF_CAFEBABE;
    // SAFETY: rsp is in user stack range; we verified PTE for rsp-8 above; same CR3 active.
    unsafe {
        let ptr = (frame.rsp - 8) as *mut u64;
        core::ptr::write_volatile(ptr, STACK_TEST_PATTERN);
        let read_back = core::ptr::read_volatile(ptr);
        if read_back != STACK_TEST_PATTERN {
            crate::arch::serial::write_str("[run] stack write/read test failed\r\n");
            crate::kernel::diagnostic::diagnostic_halt("user_stack_rw_fail");
        }
        crate::arch::serial::write_str("[run] stack rsp-8 write/read ok\r\n");
    }

    crate::arch::serial::write_str("[run] About to enter user at RIP=");
    crate::arch::serial::write_hex(frame.rip);
    crate::arch::serial::write_str("\r\n");
    #[cfg(not(test))]
    {
        paging::debug_walk_in_space(cr3, frame.rip);
        unsafe {
            let ptr = frame.rip as *const u8;
            crate::arch::serial::write_str("[run] bytes at trap_frame.rip: ");
            for i in 0..16 {
                crate::arch::serial::write_hex(*ptr.add(i) as u64);
                crate::arch::serial::write_str(" ");
            }
            crate::arch::serial::write_str("\r\n");
        }
        // 0xa0002 debug removed: address is below identity map (0x100000) and causes #PF.
    }

    // SAFETY: enter_user expects valid frame, cr3, kernel_stack from the process we are running.
    unsafe {
        context::enter_user(frame_ptr, cr3, kernel_stack);
    }
    loop {
        crate::arch::halt();
    }
}

/// Called from sys_exit (or fault path): mark current Dead, remove from runqueue, release slot, switch to next or halt.
/// Log exit reason so Director/Painter/Throne crash is visible; system must not triple fault.
pub fn exit_current_and_schedule(exit_status: Option<i32>) -> ! {
    let current_idx = match pid::current_index() {
        Some(i) => i,
        None => loop {
            crate::arch::halt();
        },
    };
    let exiting_pid = pid::get_descriptor(current_idx).map(|p| p.pid).unwrap_or(0);
    crate::fs::close_fds_for_process(exiting_pid);
    // Release any perf counters this process held.
    crate::arch::x86_64::perf::perf_close_for_process(current_idx);
    // Clear hardware breakpoints so they don't fire in the next process.
    crate::arch::x86_64::debug_regs::clear_dr_hardware();
    // Discard any queued IPC messages so the slot is clean for the next process.
    crate::process::ipc_clear(current_idx);
    crate::arch::serial::write_str("[KRN] pid ");
    crate::arch::serial::write_hex(exiting_pid as u64);
    crate::arch::serial::write_str(" exited");
    if let Some(s) = exit_status {
        crate::arch::serial::write_str(" status ");
        crate::arch::serial::write_hex(s as u32 as u64);
    }
    crate::arch::serial::write_str("\r\n");

    if let Some(ref mut pcb) = pid::get_descriptor_mut(current_idx) {
        pcb.state = ProcessState::Dead;
        pcb.exit_status = exit_status;
    }
    scheduler::remove_from_runqueue(current_idx);
    // Do not release_slot here; process stays as zombie until waitpid reaps it.
    // Wake any process blocked in waitpid waiting for this pid.
    pid::wake_waiters_for(exiting_pid);
    pid::set_current(None);

    log_scheduler_tick(Some(exiting_pid), scheduler::runqueue_total_len());
    if let Some(next_idx) = scheduler::dequeue_runqueue() {
        run_first_process(next_idx);
    }
    loop {
        crate::arch::halt();
    }
}

/// Spawn a process by program id (0=shell, 1=wm, ...). Returns new pid or None on failure.
pub fn spawn_by_program_id(program_id: u32) -> Option<process::Pid> {
    let elf = crate::kernel::programs::get_elf(program_id)?;
    if elf.len() < 4 || elf[0..4] != [0x7f, b'E', b'L', b'F'] {
        return None;
    }
    let idx = create_user_process(elf)?;
    pid::get_descriptor(idx).map(|p| p.pid)
}

fn state_to_u8(s: ProcessState) -> u8 {
    match s {
        ProcessState::Empty => 0,
        ProcessState::Runnable => 1,
        ProcessState::Running => 2,
        ProcessState::Blocked => 3,
        ProcessState::Dead => 4,
    }
}

/// Fill up to buf.len() ProcInfo entries; returns number filled.
pub fn get_proc_info_snapshot(buf: &mut [libs::ProcInfo]) -> usize {
    let mut n = 0;
    for i in 0..process::MAX_PROCESSES {
        if n >= buf.len() {
            break;
        }
        if let Some(p) = pid::get_descriptor(i) {
            buf[n] = libs::ProcInfo {
                pid: p.pid,
                state: state_to_u8(p.state),
            };
            n += 1;
        }
    }
    n
}
