//! Process subsystem: descriptor, pid, scheduler, context, loader, lifecycle, debug, ipc.

mod context;
mod debug;
pub mod ipc;
mod lifecycle;
mod loader;
pub(crate) mod pid;
mod process;
pub(crate) mod scheduler;

pub use lifecycle::{
    create_user_process, exit_current_and_schedule, get_proc_info_snapshot, run_first_process,
    spawn_by_program_id,
};
pub use ipc::{ipc_clear, ipc_dequeue, ipc_enqueue};
pub use pid::{
    current_descriptor, current_index, current_pid, current_pid_for_fault, get_descriptor,
    get_descriptor_mut, index_of_pid, reap_dead, wake_waiters_for,
};
pub use process::{
    dump_state_serial, ProcessDescriptor, ProcessState, TrapFrame, Pid, INVALID_PID,
    MAX_PROCESSES, PROCESS_CANARY, USER_LOAD_BASE, USER_STACK_TOP, Pcb,
};
pub use process::{alloc_address_space, map_page_in_space, setup_user_stack};
pub(crate) use scheduler::{tick_current, requeue_current};

/// Convenience wrapper: update nice level for a process in the EEVDF scheduler.
pub fn set_nice_for_current(proc_idx: usize, nice: i8) {
    scheduler::set_nice(proc_idx, nice);
}

pub use debug::check_current_kernel_stack_canary;
