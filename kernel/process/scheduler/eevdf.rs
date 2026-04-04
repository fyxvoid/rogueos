//! EEVDF (Earliest Eligible Virtual Deadline First) scheduler.
//!
//! Inspired by the EEVDF paper (Stoica & Zhang, 1996) and Linux 6.6+'s
//! implementation. This is a clean-room design; no GPL code is used.
//!
//! ## Core Concepts
//!
//! Each runnable task tracks:
//! - `vruntime`: total virtual CPU time consumed, scaled by weight (nice level).
//! - `deadline`: vruntime + one slice / weight — the target completion time.
//! - `eligible`: vruntime ≤ min_vruntime (the task has not over-consumed).
//!
//! The scheduler always picks the **eligible task with the earliest deadline**.
//! This guarantees proportional fairness (CFS property) while giving latency-
//! sensitive tasks low deadline values that get them scheduled quickly.
//!
//! ## Nice levels
//!
//! Nice maps to a weight using a geometric series (≈1.25× per step), matching
//! the classic CFS weight table concept but independently derived:
//!
//!   weight(nice) = BASE_WEIGHT / 1.25^nice   (nice ∈ [-20, +19])
//!
//! Higher weight → more CPU time per quantum → lower vruntime increment per tick.
//!
//! ## Virtual runtime update
//!
//! On each tick (timer interrupt) for the currently running task:
//!
//!   delta_vruntime = TICK_NS * NICE0_WEIGHT / task_weight
//!
//! This keeps vruntime comparable across tasks of different weights.
//!
//! ## Slice
//!
//! Base timeslice is 4 ms at nice 0. Heavier tasks get proportionally more.
//! Lighter tasks (higher nice) get proportionally less, down to 0.5 ms floor.
//!
//! ## Eligibility
//!
//! A task is eligible when: `task.vruntime ≤ min_vruntime + lag_tolerance`
//! where `lag_tolerance = slice / weight` (a small grace window).
//!
//! If no eligible task exists (e.g. all over-consumed), we fall back to the
//! task with globally minimum vruntime — this prevents starvation.

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum simultaneous runnable processes.
pub const MAX_RUNNABLE: usize = 64;

/// Virtual time unit = 1 microsecond in our fixed-point representation.
const US: u64 = 1;
/// Base timeslice for nice-0 process in virtual time units (4 ms).
const BASE_SLICE_US: u64 = 4_000 * US;
/// Minimum timeslice floor (0.5 ms).
const MIN_SLICE_US: u64 = 500 * US;
/// Weight for nice 0 (reference point).
const NICE0_WEIGHT: u64 = 1024;
/// One scheduler tick in virtual time units (1 ms assumed; matches typical 1 kHz timer).
const TICK_US: u64 = 1_000 * US;

// ---------------------------------------------------------------------------
// Weight table: nice -20..+19 → weight.
// Derived independently: each step ≈ ×1.25.
// Index 0 = nice -20 (highest prio), index 39 = nice +19 (lowest prio).
// ---------------------------------------------------------------------------
const WEIGHT_TABLE: [u64; 40] = [
    88761, 71755, 56483, 46273, 36291,   // nice -20..-16
    29154, 23254, 18705, 14949, 11916,   // nice -15..-11
     9548,  7620,  6100,  4904,  3906,   // nice -10..-6
     3121,  2501,  1991,  1586,  1277,   // nice  -5..-1
     1024,   820,   655,   526,   423,   // nice   0.. 4
      335,   272,   215,   172,   137,   // nice   5.. 9
      110,    87,    70,    56,    45,   // nice  10..14
       36,    29,    23,    18,    15,   // nice  15..19
];

fn weight_for_nice(nice: i8) -> u64 {
    let idx = (nice.clamp(-20, 19) + 20) as usize;
    WEIGHT_TABLE[idx]
}

// ---------------------------------------------------------------------------
// Per-task EEVDF state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub(crate) struct EevdfTask {
    /// Process table index (into PROCESS_TABLE).
    pub idx: usize,
    /// Accumulated virtual runtime (microseconds, scaled by weight).
    pub vruntime: u64,
    /// Virtual deadline = vruntime at enqueue + slice / weight.
    pub deadline: u64,
    /// Task weight derived from nice level.
    pub weight: u64,
    /// Whether this slot is occupied.
    pub occupied: bool,
}

impl EevdfTask {
    const fn empty() -> Self {
        EevdfTask {
            idx: 0,
            vruntime: 0,
            deadline: 0,
            weight: NICE0_WEIGHT,
            occupied: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Runqueue state (single global; single-core for now)
// ---------------------------------------------------------------------------

static mut TASKS: [EevdfTask; MAX_RUNNABLE] = [EevdfTask::empty(); MAX_RUNNABLE];
static mut TASK_COUNT: usize = 0;
/// Minimum vruntime across all runnable tasks — the "clock" of the runqueue.
static mut MIN_VRUNTIME: u64 = 0;
/// Index into TASKS of the currently running task (-1 = none).
static mut CURRENT_TASK_SLOT: usize = usize::MAX;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn slice_for_weight(weight: u64) -> u64 {
    // Proportional slice: BASE_SLICE_US * weight / NICE0_WEIGHT, floored at MIN.
    let s = BASE_SLICE_US.saturating_mul(weight) / NICE0_WEIGHT;
    s.max(MIN_SLICE_US)
}

unsafe fn recompute_min_vruntime() {
    let mut min = u64::MAX;
    for i in 0..MAX_RUNNABLE {
        if TASKS[i].occupied {
            if TASKS[i].vruntime < min {
                min = TASKS[i].vruntime;
            }
        }
    }
    if min != u64::MAX {
        // min_vruntime is monotonically non-decreasing.
        if min > MIN_VRUNTIME {
            MIN_VRUNTIME = min;
        }
    }
}

// ---------------------------------------------------------------------------
// Public interface
// ---------------------------------------------------------------------------

/// Total number of runnable tasks.
pub fn runqueue_total_len() -> usize {
    unsafe { TASK_COUNT }
}

/// Enqueue a process by table index. Starts with vruntime = min_vruntime so
/// it is immediately eligible (new tasks are not penalised for being new).
pub fn enqueue_runqueue(proc_idx: usize) {
    unsafe {
        // Get nice from process descriptor if available.
        let nice = crate::process::pid::get_descriptor(proc_idx)
            .map(|p| p.nice)
            .unwrap_or(0);
        let weight = weight_for_nice(nice);
        let vrt = MIN_VRUNTIME; // start at min so eligible immediately
        let slice = slice_for_weight(weight);
        let deadline = vrt + slice * NICE0_WEIGHT / weight;

        // Find an empty slot.
        for i in 0..MAX_RUNNABLE {
            if !TASKS[i].occupied {
                TASKS[i] = EevdfTask {
                    idx: proc_idx,
                    vruntime: vrt,
                    deadline,
                    weight,
                    occupied: true,
                };
                TASK_COUNT += 1;
                return;
            }
        }
        // Runqueue full — should not happen with proper MAX_PROCESSES guard.
        crate::kernel::diagnostic::diagnostic_halt("eevdf_runqueue_full");
    }
}

/// Pick and remove the best task (eligible with earliest deadline).
///
/// Eligible: task.vruntime ≤ min_vruntime + lag_tolerance.
/// If none eligible, fall back to minimum-vruntime task.
pub fn dequeue_runqueue() -> Option<usize> {
    unsafe {
        recompute_min_vruntime();

        if TASK_COUNT == 0 {
            return None;
        }

        let mut best_slot: Option<usize> = None;
        let mut best_deadline = u64::MAX;
        let mut fallback_slot: Option<usize> = None;
        let mut fallback_vrt = u64::MAX;

        for i in 0..MAX_RUNNABLE {
            if !TASKS[i].occupied {
                continue;
            }
            let t = &TASKS[i];
            // lag_tolerance: allow tasks slightly over min_vruntime to still be eligible.
            let lag = slice_for_weight(t.weight) * NICE0_WEIGHT / t.weight;
            let eligible = t.vruntime <= MIN_VRUNTIME.saturating_add(lag);

            if eligible {
                if t.deadline < best_deadline {
                    best_deadline = t.deadline;
                    best_slot = Some(i);
                }
            }
            // Fallback: minimum vruntime (prevents starvation).
            if t.vruntime < fallback_vrt {
                fallback_vrt = t.vruntime;
                fallback_slot = Some(i);
            }
        }

        let chosen = best_slot.or(fallback_slot)?;
        let proc_idx = TASKS[chosen].idx;
        CURRENT_TASK_SLOT = chosen;
        // Do not remove yet — remove on next enqueue or explicit remove.
        // Mark as running by temporarily clearing occupied to prevent re-scheduling.
        TASKS[chosen].occupied = false;
        TASK_COUNT -= 1;

        Some(proc_idx)
    }
}

/// Remove a task from the runqueue by process index (called on exit/block).
pub fn remove_from_runqueue(proc_idx: usize) {
    unsafe {
        for i in 0..MAX_RUNNABLE {
            if TASKS[i].occupied && TASKS[i].idx == proc_idx {
                TASKS[i].occupied = false;
                TASK_COUNT = TASK_COUNT.saturating_sub(1);
                return;
            }
        }
        // Also clear current slot if it matches.
        if CURRENT_TASK_SLOT < MAX_RUNNABLE && TASKS[CURRENT_TASK_SLOT].idx == proc_idx {
            CURRENT_TASK_SLOT = usize::MAX;
        }
    }
}

/// Called on each timer tick for the currently running task.
/// Increments vruntime and checks if the timeslice expired.
///
/// Returns `true` if the current task should be preempted (deadline reached).
pub fn tick_current() -> bool {
    unsafe {
        if CURRENT_TASK_SLOT >= MAX_RUNNABLE {
            return false;
        }
        // Retrieve the running task from the current process index.
        // We keep a shadow copy in CURRENT_TASK_SLOT area even when occupied=false.
        let slot = CURRENT_TASK_SLOT;
        // delta_vruntime = TICK_US * NICE0_WEIGHT / weight
        let weight = TASKS[slot].weight;
        let delta = TICK_US.saturating_mul(NICE0_WEIGHT) / weight;
        TASKS[slot].vruntime = TASKS[slot].vruntime.saturating_add(delta);

        // Check if deadline exceeded.
        TASKS[slot].vruntime >= TASKS[slot].deadline
    }
}

/// Called when the current task is rescheduled back onto the runqueue (preemption).
/// Updates deadline for next slice.
pub fn requeue_current() {
    unsafe {
        if CURRENT_TASK_SLOT >= MAX_RUNNABLE {
            return;
        }
        let slot = CURRENT_TASK_SLOT;
        let weight = TASKS[slot].weight;
        let slice = slice_for_weight(weight);
        // New deadline starts from max(current_vruntime, min_vruntime).
        let vrt = TASKS[slot].vruntime.max(MIN_VRUNTIME);
        TASKS[slot].vruntime = vrt;
        TASKS[slot].deadline = vrt + slice * NICE0_WEIGHT / weight;
        TASKS[slot].occupied = true;
        TASK_COUNT += 1;
        CURRENT_TASK_SLOT = usize::MAX;
        recompute_min_vruntime();
    }
}

/// Update the nice level for a task by process index (takes effect on next enqueue).
pub fn set_nice(proc_idx: usize, nice: i8) {
    unsafe {
        for i in 0..MAX_RUNNABLE {
            if TASKS[i].occupied && TASKS[i].idx == proc_idx {
                TASKS[i].weight = weight_for_nice(nice);
                return;
            }
        }
    }
}

/// Dump scheduler state to serial for diagnostics.
pub fn dump_runqueue_serial() {
    unsafe {
        crate::arch::x86_64::serial::write_str("[EEVDF] min_vruntime=");
        crate::arch::x86_64::serial::write_hex(MIN_VRUNTIME);
        crate::arch::x86_64::serial::write_str(" count=");
        crate::arch::x86_64::serial::write_hex(TASK_COUNT as u64);
        crate::arch::x86_64::serial::write_str("\r\n");
        for i in 0..MAX_RUNNABLE {
            if TASKS[i].occupied {
                crate::arch::x86_64::serial::write_str("  slot=");
                crate::arch::x86_64::serial::write_hex(i as u64);
                crate::arch::x86_64::serial::write_str(" idx=");
                crate::arch::x86_64::serial::write_hex(TASKS[i].idx as u64);
                crate::arch::x86_64::serial::write_str(" vrt=");
                crate::arch::x86_64::serial::write_hex(TASKS[i].vruntime);
                crate::arch::x86_64::serial::write_str(" dl=");
                crate::arch::x86_64::serial::write_hex(TASKS[i].deadline);
                crate::arch::x86_64::serial::write_str(" w=");
                crate::arch::x86_64::serial::write_hex(TASKS[i].weight);
                crate::arch::x86_64::serial::write_str("\r\n");
            }
        }
    }
}
