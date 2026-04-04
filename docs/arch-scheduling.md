### Scheduling, Processes, and (Current) Single-Core Model

This document summarizes the process model and scheduler in the kernel today.

#### Process model

- Defined primarily in `[kernel/process/process.rs]` and `[kernel/process/pid.rs]`.
- **ProcessDescriptor** holds:
  - PID, state (`Empty`, `Runnable`, `Running`, `Dead`),
  - CR3 (currently always the kernel CR3),
  - kernel stack top,
  - saved `TrapFrame` (user RIP, CS, RFLAGS, RSP, SS).
- Maximum processes:
  - `MAX_PROCESSES = 10`, enforced consistently across PID table and runqueue.
- Kernel stacks:
  - `KERNEL_STACKS[MAX_PROCESSES]`, fixed-size per-process stacks tracked by `KERNEL_STACK_USED`.

#### Runqueue and scheduler

- Module: `[kernel/process/scheduler/runqueue.rs]`.
- **Runqueue structure**
  - Two priority buckets: `NUM_PRIORITIES = 2` (high, normal).
  - Each priority has:
    - `RQ_HEAD[prio]`, `RQ_TAIL[prio]`, `RQ_LEN[prio]`.
    - Circular buffer `RUNQUEUE[prio][0..MAX_PROCESSES]` of Optional indices.
- **Enqueue**
  - `enqueue_runqueue(idx)`:
    - Looks up the process descriptor via `pid::get_descriptor(idx)` and extracts its `priority`.
    - Falls back to `PRIORITY_NORMAL` if descriptor missing.
    - Appends `idx` at the tail of the selected priority queue if there is capacity.
- **Dequeue**
  - `dequeue_runqueue()`:
    - Scans priorities from highest to lowest.
    - If `RQ_LEN[prio] > 0`, removes and returns the head index, updating head and length.
    - If a slot is unexpectedly `None` where length indicates it should be populated, halts with `runqueue_slot_empty`.
- **Removal**
  - `remove_from_runqueue(idx)`:
    - Searches through each priority queue for `idx`.
    - Compacts the queue by shifting subsequent entries left, then clears the last slot and updates `RQ_TAIL`/`RQ_LEN`.

#### CPU/core model

- Current design is effectively **single-core**:
  - Only the BSP (CPU 0) is brought up; there is no AP startup path that runs kernel code concurrently on additional cores.
  - Scheduler and runqueue are global, static, and not protected by locks (safe under single-core, non-preemptive assumptions).
- Time slicing / preemption:
  - A programmable timer interrupt (hardware INT 0x20) drives periodic scheduling decisions.
  - `log_scheduler_tick` records the current PID and total runqueue length on each tick.
  - There is no explicit thread abstraction yet; scheduling is per-process.

#### Entry to user mode

- Modules: `[kernel/process/context/mod.rs]`, `[kernel/process/lifecycle.rs]`.
- `create_user_process`:
  - Allocates a process slot and kernel stack.
  - Loads an ELF into the **current CR3**.
  - Constructs a `TrapFrame` with:
    - `rip = entry` (ELF e_entry),
    - `cs = USER_CS|3`, `ss = USER_SS|3`,
    - `rflags = IF` (interrupts enabled),
    - `rsp = USER_STACK_TOP - initial offset`.
  - Enqueues the process into the runqueue.
- `run_first_process(process_index)`:
  - Looks up the descriptor; halts with `run_first_invalid_index` if missing.
  - Sets PID “current”, marks state `Running`, logs key register state.
  - Validates the user stack mapping and performs a small write/read probe.
  - Calls `context::enter_user(frame, cr3, kernel_stack)` which:
    - Sets TSS RSP0 to `kernel_stack_top`.
    - Loads CR3 (currently the shared kernel/user CR3).
    - Sets kernel RSP.
    - Pushes an iretq frame and executes `iretq` to enter CPL=3.

#### Comparison vs modern SMP/threading expectations

Current state:

- **Single-core, single global runqueue**
  - No per-CPU data or per-CPU runqueues.
  - No thread abstraction separate from processes (one kernel stack per process, one trap frame).
- **Preemption is timer-based but simple**
  - Scheduling is cooperative with timer interrupt assistance; no multi-core contention.

Gaps vs a modern SMP/threaded OS:

- No AP bring-up:
  - Additional cores are not started; all work is on the BSP.
- No per-CPU structures:
  - No per-CPU current process, runqueue, or local interrupts accounted for.
- No threads:
  - Processes and kernel stacks are 1:1; no lightweight threads or kernel threads.

These gaps and a staged plan to address them (AP startup, per-CPU data, thread abstraction) are detailed in `design-smp-and-threads.md`.

