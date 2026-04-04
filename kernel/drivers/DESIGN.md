# Drivers Design

## Purpose

Drivers provide hardware abstraction for framebuffer, input, and block storage. Traits define the kernel-facing API; implementations follow hardware specifications. No copying of external driver code or headers.

## Algorithm / Concept Origin

- **Framebuffer**: Linear buffer, clear/fill/flush operations. Standard display model; implementation uses bootloader-provided GOP or equivalent.
- **Block device**: Read/write blocks at offset. NVMe uses the NVMe specification (command set, admin and I/O queues, register layout); register offsets and command structures are from the spec, not from driver source code.
- **Input**: Event source abstraction (keyboard, mouse). USB HID uses the HID specification for report parsing when implemented; stub until then.

## Design Choices

- **Traits**: Framebuffer, InputSource, BlockDevice are original trait definitions. No replication of external driver model hierarchy.
- **Register and command layout**: Where hardware is involved, layout follows the official hardware specification (e.g. NVMe base spec, xHCI, GPU docs). No reuse of GPL driver headers or code.
- **Naming and control flow**: Module and symbol names, and code structure, are project-specific.

## Implementation

- **traits.rs**: Framebuffer (clear, fill_rect, flush), InputSource (pop_event), BlockDevice (read_blocks, write_blocks).
- **framebuffer**: GOP-backed or equivalent; implements Framebuffer.
- **nvme**: Admin and I/O queue setup, identify, read/write via NVMe command structures; register access per NVMe spec.
- **tty**: Serial/console output; used for early and debug output.
- **input**: Aggregates input source; push_event for HID (stub or real).
- **hid_stub / usb**: Placeholder or xHCI/HID implementation per hardware specs.

No references to external driver source paths. No copying of GPL driver logic or header files.
