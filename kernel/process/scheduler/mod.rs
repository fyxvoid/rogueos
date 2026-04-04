//! Scheduler: EEVDF (Earliest Eligible Virtual Deadline First).
//! Replaces the original priority-bucket round-robin with a proper
//! virtual-runtime / deadline scheduler. See eevdf.rs for design notes.

mod runqueue;
pub(crate) mod eevdf;

pub(crate) use eevdf::{
    dequeue_runqueue,
    enqueue_runqueue,
    remove_from_runqueue,
    runqueue_total_len,
    dump_runqueue_serial,
    tick_current,
    requeue_current,
    set_nice,
    MAX_RUNNABLE,
};
