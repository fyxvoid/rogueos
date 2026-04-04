### Paging Architecture

This document describes the current paging and physical memory model in the kernel.

#### Address space layout (high level)

- **Kernel image & stack**
  - Linked at **1 MiB** (`KERNEL_IDENTITY_START = 0x0010_0000`) with an 8 MiB identity-mapped window:
    - `[0x0010_0000, 0x0090_0000)` — kernel `.text`/`.rodata`/`.data`/`.bss`, page table pool, kernel stack.
  - Paging is initialized in `[kernel/memory/paging/mapper.rs]::`init`:
    - Allocates a fresh PML4 (`new_address_space()`).
    - Identity-maps the 8 MiB kernel window with 4 KiB pages into the new CR3.
    - Switches CR3 to this new PML4 via `tlb::write_cr3`.

- **Physical frame region (buddy allocator)**
  - Discovered from UEFI BootInfo by `frame_allocator::init_from_bootinfo` using `memmap::choose_conventional_region`.
  - Region start and page count are passed into `buddy::init_with_region`, with a cap at `MAX_REGION_PAGES` (256 MiB).
  - `paging::init()` then **identity-maps this entire frame region** in the new CR3 via `identity_map_range(start, len)` and flushes TLBs.
  - A final explicit 4 KiB mapping of the first frame (`start`) is installed to guarantee that buddy metadata writes are safe.

- **User address space**
  - For now, there is a **single CR3** (kernel’s) shared by all processes:
    - `create_user_process` uses `paging::read_cr3()` and loads the ELF directly into that CR3.
  - User VA layout:
    - `USER_LOAD_BASE = 0x0000_0000_0040_0000` for user `.text`/`.data`.
    - `USER_STACK_TOP = 0x0000_7fff_ffff_f000`, with `USER_STACK_PAGES` below it.

#### Page table levels and helpers

- 4-level x86_64 paging:
  - Defined in `[kernel/memory/paging/levels.rs]`.
  - Page sizes:
    - 4 KiB (`PAGE_SIZE_4K`)
    - 2 MiB (`PAGE_SIZE_2MB`)
    - 1 GiB (`PAGE_SIZE_1GB`)
  - Index helpers:
    - `pml4_index(va)`, `pdpt_index(va)`, `pd_index(va)`, `pt_index(va)`.
  - Alignment helpers:
    - `page_align_down/_up`, `page_align_down_2mb/_1gb`, etc.

#### Mapping operations

- Module: `[kernel/memory/paging/mapper.rs]`

- **Table allocation**
  - `alloc_table_page()` draws from a static pool `PT_POOL` (128 pages), providing PA for new paging structures.
  - `page_table::phys_to_virt_table(pa)` assumes the kernel has an identity mapping for the PT pool region.

- **Map into arbitrary CR3**
  - `map_page_into_space(cr3, va, pa, flags)`:
    - Walks/allocates PML4→PDPT→PD→PT for `va` using `get_or_alloc_child`, `get_or_alloc_pd`, `get_or_alloc_pt`.
    - Installs a 4 KiB PTE `(pa & FRAME_MASK) | (flags & 0xFFF)`.
  - `map_page_2mb_into_space` and `map_page_1gb_into_space`:
    - Enforce alignment.
    - Split larger mappings when necessary (e.g. 1 GiB PDPTE → PD of 2 MiB entries).

- **Identity mapping a physical range**
  - `identity_map_range(pa_start, len)`:
    - Iterates [pa_start, pa_start+len) and maps **PA==VA** using:
      - 1 GiB pages where possible,
      - then 2 MiB pages,
      - finally 4 KiB pages for the remainder.
    - Uses the **current CR3** (which, during `paging::init()`, is the new kernel CR3).

- **Translation & debug**
  - `walk_pte(cr3, va)` returns the final PTE (if present) in the targeted address space.
  - `translate_in_space(cr3, va) -> Option<u64>` computes the PA for a given VA, handling 4 KiB, 2 MiB, and 1 GiB mappings.
  - `debug_walk_in_space(cr3, va)` and `debug_walk(va)` (current CR3) log the full L4→L1 walk plus resolved PA; they are `#[cfg(not(test))]` and only re-exported from `paging` in non-test builds.

#### Memory invariants (enforced by code)

- **Buddy/physical region invariants**
  - Frame region (start, pages) is chosen from UEFI “conventional memory” and does not overlap reserved ranges (kernel, framebuffer, NVMe, ACPI).
  - `buddy::init_with_region(start, pages)`:
    - Enforces page alignment and a maximum page count (`MAX_REGION_PAGES`).
  - `frame_allocator::region()` returns this region, and `paging::init()`:
    - Identity-maps it fully in the kernel CR3.
    - Ensures the first frame page at `start` is mapped Present+Writable with PA==VA.
  - `buddy::build_initial_freelist()` checks:
    - RSP inside `[ _stack_bottom, _stack_top ]` (kernel stack bounds).
    - `debug_walk(start)` shows a valid PTE before any writes.
    - Canary write/read at `start` (0xCAFEBABECAFED00D) to validate aliasing.
    - `push_free` logs and bounds-checks the `next` pointer against `[region_start, region_end)`.

- **User mapping invariants**
  - `load_elf`:
    - Validates ELF header (magic, class, machine, type).
    - Logs and remembers:
      - `entry` address,
      - first writable PT_LOAD (`data_page_va`),
      - first BSS location.
    - For executable PT_LOAD segments:
      - Logs `vaddr`, `file_off`, `filesz`, `memsz`.
      - After mapping the first page, compares the first 16 bytes at the mapped VA with the ELF file bytes at `p_offset`; halts on mismatch.
    - Verifies that `entry` lies inside some PT_LOAD, or halts with `user_entry_outside_load_segment`.
    - Checks that the first BSS word is zero in memory.
  - `create_user_process`:
    - Validates the entry PTE:
      - Present, User, executable (NX clear), and not writable.
    - Validates one data page PTE as Present, User, Writable, NX.
    - Dumps PTEs and the first 16 bytes at `entry`.
    - Computes an ELF checksum pre- and post-load (over bytes at `entry`), halting on mismatch to catch stale/wrong images.

#### Comparison vs “modern” expectations

Current model:

- **Single address space**
  - All user processes share the kernel CR3; per-process CR3s are not yet used, though some scaffolding (e.g. `AddressSpace` in the virtual layer) exists.
  - Pros: simpler implementation.
  - Cons: no per-process isolation; user code can (theoretically) reach kernel mappings if page protections are misconfigured.

- **Protection bits**
  - NX (`PageFlag::NoExec`) and User/Writable bits are actively used in both loader and process creation.
  - Entry (.text) is non-writable and executable; data is writable and NX.

- **Large pages**
  - 1 GiB and 2 MiB mappings are used opportunistically in `identity_map_range` to cover large identity-mapped regions (kernel, frame region).
  - User code is currently mapped with 4 KiB pages only.

Gaps vs modern OS designs:

- No **per-process address spaces**:
  - Lacks process isolation and the ability to reclaim pages on process exit by tearing down its CR3.
- No support yet for:
  - ASLR for user/kernel images.
  - Copy-on-write or advanced VM tricks.

These gaps are addressed in more detail, with a roadmap, in `design-paging-evolution.md`.

