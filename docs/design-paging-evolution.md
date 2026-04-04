### Design: Paging Evolution

This document outlines a staged evolution from the current single-CR3, identity-mapped kernel model toward per-process address spaces and stronger isolation.

#### Stage 1 — Make buddy independent of identity mapping

- **Goal:** Remove the hard dependency that “frame region is identity-mapped” in the kernel.
- **Steps:**
  - Introduce a small translation helper in memory:
    - `fn phys_to_kernel_virt(pa: u64) -> *mut u8` which:
      - Either performs `pa as *mut u8` (today’s identity assumption), or
      - Uses a fixed kernel VA window mapping (in a future layout).
  - Replace all direct `pa as *mut u8` uses in buddy and frame allocator with this helper.
  - Keep the actual mapping policy (identity vs offset) centralized in one place.

#### Stage 2 — Per-process address spaces

- **Goal:** Give each process its own CR3, while keeping the userland ABI unchanged.
- **Steps:**
  - Use the existing (currently unused) virtual-address-space abstraction:
    - Implement `AddressSpace::new()` backed by `alloc_pml4` and kernel mappings.
    - Provide methods to:
      - Map/unmap user pages for code, data, stack.
      - Clone or share kernel mappings (higher-half or shared region).
  - Extend `ProcessDescriptor` to hold an `AddressSpace` handle (or its CR3).
  - Change `create_user_process` to:
    - Allocate a fresh address space.
    - Call `load_elf` with that address space’s CR3, not the kernel CR3.
    - Map the user stack and any per-process data.
  - Change `run_first_process` / context switch:
    - Load the process’s CR3 instead of the global kernel/user CR3 before entering user mode.

#### Stage 3 — Strengthen kernel/user separation

- **Goal:** Enforce that user processes cannot reach kernel memory, even if buggy or malicious.
- **Steps:**
  - Define a clear virtual split:
    - For example: user below `0x0000_8000_0000_0000`, kernel above.
  - Ensure that:
    - The kernel half is only mapped with `User=0` entries.
    - All user mappings (code, data, heap, stack) live in the lower half with `User=1`.
  - Audit all uses of `map_page_in_space` and `map_page_into_space` to confirm flags and ranges are correct.

#### Stage 4 — Optional: ASLR and advanced VM

- **Future options:**
  - **ASLR**:
    - Randomize user code base (instead of fixed `USER_LOAD_BASE`).
    - Randomize stack base within a window.
  - **Copy-on-write**:
    - For now, processes are spawned from static ELF images; future work could introduce CoW for fork-like semantics or shared libraries.
  - **Guard pages**:
    - Place unmapped guard pages around stacks and critical regions to catch overflows earlier.

