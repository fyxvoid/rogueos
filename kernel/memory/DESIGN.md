# Memory Subsystem Design

## Purpose

The memory subsystem manages physical frame allocation, page tables and paging, virtual address spaces and regions, and kernel heap allocation. Design and implementation are original; no structural replication of external memory management layouts.

## Algorithm / Concept Origin

- **Buddy allocator**: Power-of-two block splitting and coalescing is a well-known algorithm; the implementation (bitmaps, order lists, layout) is independent.
- **Page tables**: Hierarchical page tables (PML4/PDPT/PD/PT on x86-64) follow the architecture; the code that builds and walks them is written for this system.
- **Slab-style heap**: Object-sized caches and per-cache free lists are a standard approach for kernel allocators; the specific structures and naming are project-defined.
- **Address space abstraction**: A single root (e.g. CR3), regions, and mappings are common concepts; the types (AddressSpace, Region, Mapping) and APIs are defined here.

## Design Choices

- **Physical**: Layout and regions describe usable memory; buddy allocator provides frames; FrameAllocator trait abstracts allocation. No replication of external physical memory manager layout.
- **Paging**: Flags, levels, page table representation, mapper, TLB invalidation, and fault handling are implemented for this kernel. No struct-by-struct correspondence to any external paging code.
- **Virtual**: AddressSpace (e.g. CR3), Region (va range, flags), and mapping helpers. No mm_struct or vm_area_struct equivalents; naming and layout are original.
- **Heap**: Slab, cache, and allocator provide kmalloc/kfree-style API. Implementation is independent.
- **Debug**: Page walk and stats for diagnostics. No dependency on external mm debugging layout.

## Implementation

- **physical/**: layout, region, buddy, frame_allocator.
- **paging/**: tlb, flags, levels, page_table, mapper, fault.
- **virtual/**: address_space, region, mapping.
- **heap/**: slab, cache, allocator, kmalloc.
- **debug/**: page_walk, stats.

No references to external mm/ source paths or structure names. No one-to-one mapping to any external memory subsystem.
