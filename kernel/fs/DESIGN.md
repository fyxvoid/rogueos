# Filesystem Design

## Purpose

The filesystem layer provides a root-only, flat file namespace backed by a block device. The VFS exposes open/close/read/write/seek/fsync/unlink and a descriptor model (fd 0/1/2 TTY, fd >= 3 file handles). The on-disk layout uses a volume header and a file record table. Design and implementation are original.

## Algorithm / Concept Origin

- **Block-backed storage**: Reading and writing in fixed-size blocks is a standard approach; the layout (which block is header, which is file table, which is data) is defined for this system only.
- **Flat file table**: A fixed-size table of file records (name, size, start block) allows O(n) lookup by name and simple allocation. No directory tree; root only.
- **Descriptor table**: Per-process (or global) open-file table mapping fd to file record index and offset is a common abstraction; the specific API and constants are project-defined.

## Design Choices

- **VolumeHeader**: Single block at block 0 holding magic, data start, next free block, file table block, and file count. No replication of external on-disk structure names or layouts.
- **FileRecord**: Fixed-size record (name, size, start_block, reserved). Table lives in a single block; semantics are independent.
- **VFS**: Open returns a handle (fd); read/write/seek operate on handle and use file_index into the file record table. No path resolution beyond root-level names.

## Implementation

- **simple_fs**: VolumeHeader, FileRecord, mount_root, flush_volume_header, find_file_by_name, alloc_file_record, get_file_record_info, read_file_data, write_file_data, free_file_record, list_root. Block I/O via BlockDevice trait only; no separate block layer module.
- **vfs**: FdEntry (file_index, offset), open/close/read_file/write_file/seek/fsync/list_root/unlink. fd 0/1/2 reserved for TTY; first file fd is 3.

No references to external VFS or on-disk format designs. No structural replication of any external inode or superblock layout.
