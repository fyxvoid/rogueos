# Process Subsystem Design

## Purpose

The process subsystem manages process metadata, identity (PID), scheduling, context switching, and loading of user programs. It is spawn-only: no fork or clone. Design and implementation are original.

## Algorithm / Concept Origin

- **Priority-bucket scheduler**: Two queues (high and normal priority); round-robin within each queue. A standard approach for O(1) selection; the specific layout and naming are independent.
- **Spawn-only lifecycle**: Process creation is by loading an executable (ELF) and enqueueing; no duplication of address space or descriptor from another process.
- **Trap frame**: Saved user-mode register state (rip, cs, rflags, rsp, ss) for iretq is a standard mechanism; the structure and layout are defined for this system only.
- **ELF loading**: PT_LOAD segment parsing and mapping follow the ELF specification; the loader code is an independent implementation.

## Design Choices

- **Separation of concerns**: Process descriptor and state live in `process`; table and PID allocation in `pid`; runqueue in `scheduler`; switch logic in `context`; ELF load in `loader`; lifecycle (spawn, exit, run) in `lifecycle`; canary/stack checks in `debug`.
- **No fork/clone**: Simplifies audit and avoids POSIX process model. All processes are created via spawn-by-program-id.
- **Fixed process table**: Bounded table and runqueue size; no dynamic allocation of PCB structures at runtime for determinism.

## Implementation

- **process.rs**: ProcessDescriptor, ProcessState, TrapFrame, constants (PRIORITY_*, USER_LOAD_BASE, etc.), address-space helpers (alloc_address_space, setup_user_stack).
- **pid.rs**: Process table, NEXT_PID, current index/pid, slot allocation/release, kernel stack allocation per slot, canary check.
- **scheduler/**: Two priority buckets, enqueue/dequeue/remove by table index; priority taken from descriptor.
- **context/**: enter_user(frame, cr3, kernel_stack) — set CR3, kernel stack, push iretq frame, iretq.
- **loader/**: load_elf(elf_data, cr3) — parse ELF64 PT_LOAD, map pages, copy segments.
- **lifecycle**: create_user_process (allocate slot, load ELF, setup stack, enqueue), exit_current_and_schedule, run_first_process, spawn_by_program_id, get_proc_info_snapshot.
- **debug**: check_current_kernel_stack_canary (uses pid’s kernel stack canary).

No references to external process or scheduler implementations. No structural replication of any external PCB or scheduler layout.

## Address-space validation (user process)

- **STEP 1 — ELF segment permissions**: PT_LOAD text → USER|PRESENT|EXEC|READ (not writable); data → USER|PRESENT|WRITE|READ (NX=1). After load, PTE flags are dumped for entry and first data page; assertions: .text not writable, .data not executable.
- **STEP 2 — BSS zeroing**: For each segment with memsz > filesz, region [filesz, memsz] is zeroed. First BSS byte is asserted zero after load.
- **STEP 3 — User stack guard page**: One page immediately below the user stack is left unmapped; overflow probes cause #PF.
- **STEP 4 — Heap**: Palace userland is no_std and does not use malloc; no heap region. If userland gains a heap, define USER_HEAP_START and map on demand.
- **STEP 5 — Stress SYS_EXIT**: Optional (STRESS_EXIT_FIRST): spawn 10 sequential short-lived processes (exit binary); ensures no CR3/stack/PID/frame reuse corruption. Requires MAX_PROCESSES ≥ 10.
- **STEP 6 — Long-run test**: Run palace (init) under QEMU for 60 seconds. Pass criteria: no memory leak, no #GP, kernel stack canary not triggered, frame allocator not corrupted. Manual/CI; no kernel code beyond above.

## Desktop pipeline validation (init → Director → Painter → Throne)

- **STEP 1 — Init (steward)**: Startup log; spawn Director (wm); wait (delay); log painter start; spawn Throne (shell). Logs: `[INIT] director start`, `[INIT] painter start`, `[INIT] throne start`. No silent spawn failure.
- **STEP 2 — Framebuffer ownership**: First process to call fb syscall is owner; any other pid writing logs `[FB] warning`. Only Director should write; Painter writes through Director; Throne never touches framebuffer.
- **STEP 3 — Event loop**: Director (wm) must not block forever and not spin at 100% undetected; iteration counter log every 5000 cycles (`[WM] tick 5000`). System must not freeze after 30 seconds.
- **STEP 4 — Input dispatch**: USB key → event → Director → active surface → Throne. Debug log `[INPUT] keycode received` when key event is delivered.
- **STEP 5 — Window lifecycle**: Tiling layout splits evenly (compositor test), no overlap, no null on commit/destroy. Closing window reflows when supported; no memory leak on close.
- **STEP 6 — Memory integration**: Run desktop 60 seconds. Verify frame allocator free count unchanged, heap roughly stable, no stack canary violation, no page faults.
- **STEP 7 — Failure hardening**: On exit, log `[KRN] pid X exited status Y`. Director/Painter/Throne crash must not triple-fault. Painter crash: halt cleanly with diagnostic. Throne crash: return to minimal shell (e.g. runqueue continues with init/wm).
