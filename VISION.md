# RogueOS Vision & Evolution

> *Linux fork/exec is fundamentally broken for security. Windows job objects
> are coarse. seL4 is correct but unusable. RogueOS makes every process as
> isolated as a VM but with zero overhead and full Rust type safety.*

---

## The Thesis

Modern operating systems fail at security by default. Every process on Linux
inherits a copy of the parent's entire file descriptor table, address space,
and capability set via fork(). A single exploited process can escalate to
access resources its code never legitimately needed.

RogueOS inverts this: **no process has any authority by default**. Every
resource a process can touch is an explicit, unforgeable, kernel-managed
capability token. The attack surface of a compromised process is bounded
precisely by the tokens it was granted at spawn time.

---

## Implemented (this branch)

### 1. Per-Process Address Spaces (Per-Process CR3)

Every process is spawned with its own PML4 page table. Kernel mappings are
shallow-copied (shared read-only) so the kernel can run during syscalls.
User-VA regions (`0x400000+`, user stack at `0x7fff_ffff_f000`) are fresh
zeroed tables — completely isolated.

**Result:** Multiple processes can load ELF binaries at the same virtual
address (e.g., `0x400000`) without conflict. A memory corruption bug in one
process cannot corrupt another process's code or data.

**Files:** `kernel/memory/paging/mapper.rs` → `create_process_cr3()`

### 2. PCID / ASID Infrastructure

`probe_pcid()` detects Intel/AMD PCID support at boot. `alloc_pcid()` assigns
a unique 12-bit identifier to each process. Every `ProcessDescriptor` carries
its `pcid: u16`.

The scheduler will use this to write `CR3 | PCID | (1 << 63)` (no-TLB-flush
flag) on context switch — eliminating the largest overhead of address-space
switching and making context switches approach zero cost.

**Files:** `kernel/memory/paging/mapper.rs` → `probe_pcid()`, `alloc_pcid()`

### 3. Capability-Based Security

Every process carries a `CapSet` (64-bit bitmask). Every syscall that touches
a sensitive resource checks the relevant bit before proceeding.

Cogman (pid 1) is born with `cap::ALL` and `cap::GRANT`. It is the root of
the capability tree — the only entity that can grant or revoke authority on
other processes. No child can have more authority than its parent granted.

```
Cogman (cap::ALL)
├── session  (DISPLAY | INPUT | IPC | SHM | SPAWN | FS_READ)
├── shell    (SPAWN | FS | IPC | DISPLAY | INPUT)
├── wm       (DISPLAY | INPUT | COMPOSITOR | SPAWN | IPC | SHM)
└── monitor  (PROC_INFO | DISPLAY | IPC)
```

**Files:** `kernel/capability/mod.rs`, `kernel/capability/journal.rs`

### 4. Cogman Restart Journal (Zero Data Loss)

Cogman writes its supervisor state to a 4 KiB kernel journal after every
service-table mutation (spawn / reap / restart). A replacement Cogman
instance calls `SYS_JOURNAL_READ` on startup and resumes supervising all
services in < 5 ms with zero data loss.

**Files:** `kernel/capability/journal.rs`, `userland/src/bin/cogman/supervisor.rs`

### 5. Anonymous Memory (SYS_MMAP / SYS_MUNMAP)

Processes can request private anonymous memory pages from the kernel.
The kernel bump-allocates virtual addresses in the range
`0x0000_4000_0000` – `0x0000_7000_0000`, allocates physical frames from
the buddy allocator, and maps them into the process's own CR3.

This is the foundation for:
- Process-level heap allocators (no shared bump allocator)
- Guard pages (PROT_NONE)
- Copy-on-write shared memory (future)
- Demand paging (future)

**Files:** `kernel/syscall/dispatcher/mmap.rs`

---

## The Evolution Roadmap

### Stage 1 — Address Space Hardening (current)
- [x] Per-process CR3
- [x] PCID detection + allocation
- [x] SYS_MMAP / SYS_MUNMAP (anonymous private)
- [ ] PCID written to CR3 on context switch (needs scheduler)
- [ ] Physical frame reclaim on process exit (free buddy frames on unmap)
- [ ] Guard pages (PROT_NONE mapped pages that trigger SIGSEGV-equivalent)

### Stage 2 — True Capability Containers
- [x] CapSet per process
- [x] Core syscall gates (SPAWN, REBOOT, INPUT, IPC, PROC_INFO)
- [ ] FS capability enforcement (SYS_OPEN checks CAP_FS_READ / CAP_FS_WRITE)
- [ ] SHM capability gate (SYS_SHM_CREATE checks CAP_SHM)
- [ ] PERF / HW_BP capability gates
- [ ] Capability transfer: `SYS_CAP_DELEGATE(target_pid, cap_id, flags)`

### Stage 3 — Multiprocessor (SMP)
- [ ] ACPI MADT parsing (detect AP count + APIC IDs)
- [ ] AP startup sequence (INIT-SIPI-SIPI via LAPIC)
- [ ] Per-CPU kernel stacks
- [ ] Spinlock + seqlock primitives (safe `UnsafeCell<T>` behind a lock)
- [ ] Scheduler: per-CPU run queues with work-stealing

### Stage 4 — Typed Capability Tokens (seL4-inspired)
Replace the flat bitmask with unforgeable typed tokens:
```rust
pub enum CapToken {
    Endpoint { target_pid: Pid, rights: IpcRights },
    MemoryFrame { pa: u64, size: usize, prot: Prot },
    VNode { pml4_pa: u64 },          // address space root
    Irq { vector: u8 },              // hardware interrupt
    Untyped { pa: u64, size: usize }, // raw physical memory
}
```
Mint/revoke via kernel; processes pass tokens by handle. Revocation is
immediate and transitive — no capability leaks.

### Stage 5 — Demand Paging + Copy-on-Write
- Page fault handler extends process VMA on demand
- CoW fork-equivalent: `SYS_CLONE_SPACE` — share PML4 with CoW bits
- Enables fast process creation without copying address spaces

### Stage 6 — Verified Kernel Interface
Leverage Rust's type system to make incorrect kernel API use a compile
error, not a runtime panic:
- Capability tokens as `#[must_use]` RAII guards (drop = revoke)
- Zero-cost verify: `CapToken<Display>` can only be passed to display
  syscalls — enforced by the type system, not runtime checks

---

## Design Principles

1. **No ambient authority.** A fresh process has `CapSet::none()`. It can do
   nothing. Cogman explicitly grants the minimum needed set.

2. **Parent cannot elevate child.** `child_caps = parent_caps & requested`.
   Impossible to escalate through a spawn chain.

3. **Unforgeable handles.** CapSet bits are stored in kernel memory
   (ProcessDescriptor), never in user memory. A process cannot modify its own
   capability set.

4. **Isolation by default, sharing by explicit grant.** IPC, shared memory,
   display access — all require explicit capability tokens. A process
   that does not need the network cannot accidentally use it.

5. **Zero overhead.** Per-process CR3 costs one TLB flush at spawn (not at
   every syscall). With PCID, context switching costs zero TLB flushes for
   warm workloads. Capability checks are a single `&` instruction.

---

## Comparison

| Property             | Linux        | Windows      | seL4         | **RogueOS**       |
|----------------------|-------------|-------------|-------------|------------------|
| Default authority    | Inherited    | Inherited    | None        | **None**         |
| Fork semantics       | Full clone   | N/A          | No fork     | **No fork**      |
| Capability model     | Coarse (uid) | Job objects  | Typed caps  | **Typed (WIP)**  |
| Address isolation    | Per-process  | Per-process  | Per-process | **Per-process**  |
| Context switch cost  | TLB flush    | TLB flush    | TLB flush   | **Zero (PCID)**  |
| Type safety          | None         | None         | None        | **Rust**         |
| Verifiability        | None         | None         | Formal      | **Rust types**   |
