//! Process descriptor (PCB), state, trap frame, and address-space helpers.

use crate::memory::paging;
use crate::memory::r#virtual as virt;
use crate::arch::x86_64::debug_regs::HwBpState;
use crate::capability::CapSet;
use libs::{Uid, DEFAULT_SESSION_UID};

// ---------------------------------------------------------------------------
// Process descriptor (PCB)
// ---------------------------------------------------------------------------

/// Process ID. Unique per process; 0 is reserved (invalid / kernel).
pub type Pid = u32;

pub const INVALID_PID: Pid = 0;

/// Maximum number of processes (process table). Raised to 64 for real workloads.
/// EEVDF runqueue supports MAX_RUNNABLE (64) slots independently.
pub const MAX_PROCESSES: usize = 64;

/// Saved user-mode state for iretq.
/// Layout: rip=0, cs=8, rflags=16, rsp=24, ss=32. enter_user pushes (ss, rsp, rflags, cs, rip) so iretq pops rip, cs, rflags, rsp, ss.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TrapFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// Process state for the scheduler.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProcessState {
    Empty,
    Runnable,
    Running,
    Blocked,
    Dead,
}

/// Canary value for memory corruption detection.
pub const PROCESS_CANARY: u64 = 0xDEAD_BEEF_CAFE_BABE;

/// Legacy priority constants kept for syscall ABI compatibility.
/// EEVDF maps these to nice values: HIGH → nice -5, NORMAL → nice 0.
pub const PRIORITY_HIGH: u8 = 0;
pub const PRIORITY_NORMAL: u8 = 1;

/// Process descriptor (PCB). One per process; holds everything needed to run or resume it.
#[derive(Clone)]
pub struct ProcessDescriptor {
    /// Canary for corruption detection; must equal PROCESS_CANARY.
    pub canary: u64,
    /// Process ID (unique, never 0).
    pub pid: Pid,
    /// User identity. Single-user model: kernel is UID_KERNEL (0); all user processes use DEFAULT_SESSION_UID (1000).
    pub uid: Uid,
    /// Scheduling state.
    pub state: ProcessState,
    /// Legacy scheduling priority (PRIORITY_HIGH or PRIORITY_NORMAL).
    pub priority: u8,
    /// EEVDF nice level (-20 = highest prio, +19 = lowest prio, 0 = default).
    pub nice: i8,
    /// Address space: physical address of PML4 (CR3).
    pub cr3: u64,
    /// Kernel stack top (high address; stack grows down). Used on syscall/interrupt.
    pub kernel_stack_top: u64,
    /// Saved user state: rip, cs, rflags, rsp, ss for iretq.
    pub trap_frame: TrapFrame,
    /// Exit status when state == Dead; set on exit, read by waitpid then slot is reaped.
    pub exit_status: Option<i32>,
    /// Hardware breakpoint state (DR0-DR7) — saved/restored on context switch.
    pub hw_bp: HwBpState,
    /// PID this process is blocked waiting for (u32::MAX = any child). None if not blocked.
    pub waiting_for: Option<Pid>,
    /// Capability bitmask. Controls which syscalls this process may invoke.
    /// Cogman (pid 1) is born with `CapSet::all()`; every other process starts
    /// with the intersection of its parent's caps and the requested spawn mask.
    pub caps: CapSet,
}

impl ProcessDescriptor {
    /// Create a new descriptor (for internal use by lifecycle).
    pub fn new(
        pid: Pid,
        state: ProcessState,
        cr3: u64,
        kernel_stack_top: u64,
        trap_frame: TrapFrame,
    ) -> Self {
        ProcessDescriptor {
            canary: PROCESS_CANARY,
            pid,
            uid: DEFAULT_SESSION_UID,
            state,
            priority: PRIORITY_NORMAL,
            nice: 0,
            cr3,
            kernel_stack_top,
            trap_frame,
            exit_status: None,
            hw_bp: HwBpState::new(),
            waiting_for: None,
            caps: CapSet::none(), // caller must set appropriate caps after construction
        }
    }

    /// Whether this process is runnable or running.
    #[inline]
    pub fn is_alive(&self) -> bool {
        matches!(self.state, ProcessState::Runnable | ProcessState::Running)
    }

    /// Whether the process has exited.
    #[inline]
    pub fn is_dead(&self) -> bool {
        self.state == ProcessState::Dead
    }
}

/// Alias for ProcessDescriptor.
pub type Pcb = ProcessDescriptor;

// ---------------------------------------------------------------------------
// Address space helpers
// ---------------------------------------------------------------------------

/// User virtual address for code load (e.g. ELF load base).
pub const USER_LOAD_BASE: u64 = 0x400_000;
/// User stack top (below this).
pub const USER_STACK_TOP: u64 = 0x7fff_ffff_f000;
pub(crate) const USER_STACK_PAGES: usize = 8;
/// Palace userland is no_std and does not use malloc; no USER_HEAP_START or heap region.
/// If userland gains a heap later: define USER_HEAP_START, ensure no overlap with stack, map on demand.
const PAGE_SIZE: usize = 4096;

/// Allocate a new empty address space (new PML4). Returns CR3 value or 0.
pub fn alloc_address_space() -> u64 {
    virt::alloc_address_space()
}

/// Map a page in a given address space. cr3 = PML4 physical address.
pub fn map_page_in_space(cr3: u64, va: u64, pa: u64, flags: u64) -> bool {
    virt::map_page_in_space(cr3, va, pa, flags)
}

/// Set up user stack mapping in the given address space. Returns Some(user RSP) or None on failure.
/// Guard page below stack (unmapped); overflow causes #PF.
pub fn setup_user_stack(cr3: u64) -> Option<u64> {
    let flags = paging::EntryFlags::user_rw().as_u64();
    for i in 0..USER_STACK_PAGES {
        let Some(pa) = virt::alloc_table_page() else {
            return None;
        };
        // Map pages BELOW RSP (i=0 → one page below stack top, i=1 → two pages below, etc.)
        let va = USER_STACK_TOP - ((i as u64 + 1) * PAGE_SIZE as u64);
        if !virt::map_page_in_space(cr3, va, pa, flags) {
            return None;
        }
    }
    // Guard page: leave unmapped. VA = USER_STACK_TOP - USER_STACK_PAGES*PAGE_SIZE - PAGE_SIZE.
    // Deliberate access below stack (e.g. overflow) triggers clean page fault.
    Some(USER_STACK_TOP)
}

/// Dump process table and runqueue state to serial (for diagnostics).
pub fn dump_state_serial() {
    super::pid::dump_table_serial();
    super::scheduler::dump_runqueue_serial();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trap_frame(rip: u64) -> TrapFrame {
        TrapFrame {
            rip,
            cs: 0x23,
            rflags: 0x202,
            rsp: USER_STACK_TOP,
            ss: 0x2b,
        }
    }

    #[test]
    fn test_process_descriptor_new() {
        let tf = make_trap_frame(0x400_000);
        let p = ProcessDescriptor::new(1, ProcessState::Runnable, 0x1000, 0x8000, tf);
        assert_eq!(p.pid, 1);
        assert_eq!(p.uid, DEFAULT_SESSION_UID);
        assert_eq!(p.state, ProcessState::Runnable);
        assert_eq!(p.cr3, 0x1000);
        assert_eq!(p.kernel_stack_top, 0x8000);
        assert_eq!(p.trap_frame.rip, 0x400_000);
    }

    #[test]
    fn test_is_alive() {
        let tf = make_trap_frame(0);
        assert!(ProcessDescriptor::new(1, ProcessState::Runnable, 0, 0, tf.clone()).is_alive());
        assert!(ProcessDescriptor::new(1, ProcessState::Running, 0, 0, tf.clone()).is_alive());
        assert!(!ProcessDescriptor::new(1, ProcessState::Empty, 0, 0, tf.clone()).is_alive());
        assert!(!ProcessDescriptor::new(1, ProcessState::Dead, 0, 0, tf).is_alive());
    }

    #[test]
    fn test_is_dead() {
        let tf = make_trap_frame(0);
        assert!(ProcessDescriptor::new(1, ProcessState::Dead, 0, 0, tf.clone()).is_dead());
        assert!(!ProcessDescriptor::new(1, ProcessState::Runnable, 0, 0, tf).is_dead());
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_PROCESSES, 64);
        assert_eq!(USER_LOAD_BASE, 0x400_000);
        assert_eq!(USER_STACK_TOP, 0x7fff_ffff_f000);
    }
}
