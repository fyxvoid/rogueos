//! Driver traits so kernel/syscall layer depends on abstractions, not concrete drivers.
//! Framebuffer, InputSource, BlockDevice (for NVMe later).

use libs::KeyEvent;

/// Framebuffer abstraction: clear, fill_rect, flush. Implemented by GOP framebuffer.
pub trait Framebuffer {
    fn clear(&self, color: u32);
    fn fill_rect(&self, x: u32, y: u32, w: u32, h: u32, color: u32);
    fn flush(&self);
}

/// Input event source (keyboard + mouse). Implemented by USB HID; stub until Section 11.
pub trait InputSource {
    fn pop_event(&self) -> Option<KeyEvent>;
}

/// Block device for storage. Implemented by NVMe (Section 6). Not used by syscall yet.
pub trait BlockDevice {
    fn read_blocks(&self, block_offset: u64, buf: &mut [u8]) -> bool;
    fn write_blocks(&self, block_offset: u64, buf: &[u8]) -> bool;
}
