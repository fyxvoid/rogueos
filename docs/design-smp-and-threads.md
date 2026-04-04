### Design: SMP and Threads Roadmap

This document proposes a staged evolution from the current single-core scheduler to a basic SMP and threading model.

#### Stage 1 — Per-CPU bootstrap and data

- **AP startup**
  - Extend `arch/x86_64` boot code to:
    - Enumerate APIC IDs and send INIT+SIPI to bring up APs.
    - Enter a common `ap_entry` function for secondary cores.
  - In `ap_entry`, each AP:
    - Switches to the kernel’s PML4 (shared for now).
    - Initializes a per-CPU stack and per-CPU data structure.

- **Per-CPU struct**
  - Define `PerCpu` with fields such as:
    - `current_pid: Option<Pid>`
    - `current_process_index: Option<usize>`
    - `local_runqueue: RunQueue` (or references into a global runqueue for the first stage).
  - Provide helper functions:
    - `percpu::current()` → `&'static mut PerCpu`.

#### Stage 2 — Thread abstraction

- **Thread vs process**
  - Extend `ProcessDescriptor` or introduce a `Thread` struct to represent:
    - A kernel stack and trap frame.
    - An owning process (for address space and resources).
  - Initially, keep a 1:1 mapping (one thread per process), but design the data model so multiple threads per process can be added later.

- **Scheduling unit = thread**
  - Change runqueue entries from process indices to thread IDs (or `(pid, tid)`).
  - Keep the existing priority buckets but store threads instead of processes.

#### Stage 3 — SMP-aware scheduler

- **Per-CPU runqueues**
  - Give each `PerCpu` its own runqueue.
  - Implement simple load balancing:
    - On thread creation, assign to the least-loaded CPU.
    - On timer interrupt, if a CPU is idle, it may steal a thread from another CPU’s runqueue.

- **Timer interrupts**
  - Ensure the local APIC timers are programmed on each CPU.
  - Use per-CPU timer interrupts to drive time slicing on each core independently.

- **Locking strategy**
  - Introduce basic synchronization around shared structures:
    - PID table, file descriptors, global runqueue (if kept), and display/display state.
  - Use coarse-grained spinlocks initially, refining later if contention is observed.

#### Stage 4 — Address spaces and per-process CR3

- Once the threading model is in place:
  - Move away from a single CR3 for all processes.
  - Use the existing (currently unused) `AddressSpace` abstraction to:
    - Allocate a PML4 per process.
    - Map user segments and stacks into that address space.
  - On context switch:
    - Switch CR3 when changing the running process (not merely threads within a process).
  - This stage ties into the paging evolution described in `design-paging-evolution.md`.

