//! Priority-bucket runqueue: two queues (high, normal), round-robin within each.

use crate::process::pid;
use crate::process::process::{PRIORITY_NORMAL, MAX_PROCESSES};

const NUM_PRIORITIES: usize = 2;

static mut RQ_HEAD: [usize; NUM_PRIORITIES] = [0; NUM_PRIORITIES];
static mut RQ_TAIL: [usize; NUM_PRIORITIES] = [0; NUM_PRIORITIES];
static mut RQ_LEN: [usize; NUM_PRIORITIES] = [0; NUM_PRIORITIES];
static mut RUNQUEUE: [[Option<usize>; MAX_PROCESSES]; NUM_PRIORITIES] =
    [[None; MAX_PROCESSES]; NUM_PRIORITIES];

pub(crate) fn runqueue_total_len() -> usize {
    unsafe { RQ_LEN[0] + RQ_LEN[1] }
}

pub(crate) fn enqueue_runqueue(idx: usize) {
    unsafe {
        let prio = pid::get_descriptor(idx)
            .map(|p| p.priority as usize)
            .unwrap_or(PRIORITY_NORMAL as usize)
            .min(NUM_PRIORITIES - 1);
        if RQ_LEN[prio] < MAX_PROCESSES {
            let tail = RQ_TAIL[prio];
            RUNQUEUE[prio][tail] = Some(idx);
            RQ_TAIL[prio] = (tail + 1) % MAX_PROCESSES;
            RQ_LEN[prio] += 1;
        }
    }
}

pub(crate) fn dequeue_runqueue() -> Option<usize> {
    unsafe {
        for prio in 0..NUM_PRIORITIES {
            if RQ_LEN[prio] > 0 {
                let head = RQ_HEAD[prio];
                let idx = match RUNQUEUE[prio][head].take() {
                    Some(i) => i,
                    None => crate::kernel::diagnostic::diagnostic_halt("runqueue_slot_empty"),
                };
                RQ_HEAD[prio] = (head + 1) % MAX_PROCESSES;
                RQ_LEN[prio] -= 1;
                return Some(idx);
            }
        }
        None
    }
}

pub(crate) fn remove_from_runqueue(idx: usize) {
    unsafe {
        for prio in 0..NUM_PRIORITIES {
            let len = RQ_LEN[prio];
            let head = RQ_HEAD[prio];
            let mut found_at: Option<usize> = None;
            for i in 0..len {
                let pos = (head + i) % MAX_PROCESSES;
                if RUNQUEUE[prio][pos] == Some(idx) {
                    found_at = Some(i);
                    break;
                }
            }
            if let Some(i) = found_at {
                for j in i..len - 1 {
                    let from = (head + j + 1) % MAX_PROCESSES;
                    let to = (head + j) % MAX_PROCESSES;
                    RUNQUEUE[prio][to] = RUNQUEUE[prio][from];
                    RUNQUEUE[prio][from] = None;
                }
                RUNQUEUE[prio][(head + len - 1) % MAX_PROCESSES] = None;
                RQ_LEN[prio] -= 1;
                RQ_TAIL[prio] = if RQ_LEN[prio] == 0 {
                    RQ_HEAD[prio]
                } else {
                    (head + RQ_LEN[prio] - 1) % MAX_PROCESSES
                };
                return;
            }
        }
    }
}

pub(crate) fn dump_runqueue_serial() {
    unsafe {
        crate::arch::x86_64::serial::write_str("[DIAG][PROC] runqueues:\r\n");
        for prio in 0..NUM_PRIORITIES {
            crate::arch::x86_64::serial::write_fmt(format_args!(
                "  prio={} head={} tail={} len={} [",
                prio, RQ_HEAD[prio], RQ_TAIL[prio], RQ_LEN[prio]
            ));
            for j in 0..MAX_PROCESSES {
                match RUNQUEUE[prio][j] {
                    Some(idx) => crate::arch::x86_64::serial::write_fmt(format_args!("{} ", idx)),
                    None => crate::arch::x86_64::serial::write_str(". "),
                }
            }
            crate::arch::x86_64::serial::write_str("]\r\n");
        }
    }
}
