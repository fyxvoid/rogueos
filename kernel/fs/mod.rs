//! Minimal filesystem and VFS for daily-driver: single root, flat file list, NVMe-backed.

mod simple_fs;
mod vfs;

pub use simple_fs::{flush_volume_header, mount_root, root_mounted};
pub use vfs::{close, close_fds_for_process, fsync, list_root, open, read_file, seek, unlink, write_file};
pub use vfs::OpenFlags;
