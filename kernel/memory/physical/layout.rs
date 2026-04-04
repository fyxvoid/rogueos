//! Physical memory layout constants. Single source for frame region base and size.
//! Can be overridden from boot or config; default is fixed region for early bootstrap.

/// Page size in bytes (4 KiB).
pub const PAGE_SIZE: usize = 4096;

/// Default physical base for the allocatable frame region (2 MiB).
/// Chosen to avoid low memory and bootloader usage.
pub const FRAME_REGION_BASE: u64 = 2 * 1024 * 1024;

/// Default number of pages in the frame region (8 MiB total).
pub const FRAME_REGION_PAGES: usize = 2048;

/// Maximum allocation order: 2^MAX_ORDER pages per block (e.g. 12 => 16 MiB max block).
pub const MAX_ORDER: usize = 12;
