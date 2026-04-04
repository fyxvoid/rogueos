//! Input event queue exposed to syscalls. Fed by USB HID only (keyboard + mouse).
//! No PS/2. See drivers::hid_stub for the USB HID placeholder.

use core::sync::atomic::{AtomicUsize, Ordering};

use libs::{KeyEvent, MouseEvent};

const QUEUE_SIZE: usize = 64;
static KEY_DROPS: AtomicUsize = AtomicUsize::new(0);
static MOUSE_DROPS: AtomicUsize = AtomicUsize::new(0);

static mut QUEUE: [KeyEvent; QUEUE_SIZE] = [KeyEvent {
    keycode: 0,
    pressed: false,
}; QUEUE_SIZE];
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);

static mut MOUSE_QUEUE: [MouseEvent; QUEUE_SIZE] = [MouseEvent {
    dx: 0,
    dy: 0,
    buttons: 0,
}; QUEUE_SIZE];
static MOUSE_HEAD: AtomicUsize = AtomicUsize::new(0);
static MOUSE_TAIL: AtomicUsize = AtomicUsize::new(0);

/// Push a key event into the ring buffer. Drops event if full.
pub fn push_event(ev: KeyEvent) {
    let head = HEAD.load(Ordering::Relaxed);
    let tail = TAIL.load(Ordering::Acquire);
    let next = (head + 1) % QUEUE_SIZE;
    if next == tail {
        // Queue full, drop event.
        let d = KEY_DROPS.fetch_add(1, Ordering::Relaxed) + 1;
        if (d & 0x3f) == 0 {
            crate::arch::serial::write_str("[input] key drops=");
            crate::arch::serial::write_hex(d as u64);
            crate::arch::serial::write_str("\r\n");
        }
        return;
    }
    unsafe {
        QUEUE[head] = ev;
    }
    HEAD.store(next, Ordering::Release);
}

/// Pop a key event from the ring buffer, if any.
pub fn pop_event() -> Option<KeyEvent> {
    let tail = TAIL.load(Ordering::Relaxed);
    let head = HEAD.load(Ordering::Acquire);
    if tail == head {
        return None;
    }
    let ev = unsafe { QUEUE[tail] };
    let next = (tail + 1) % QUEUE_SIZE;
    TAIL.store(next, Ordering::Release);
    Some(ev)
}

/// Push a mouse event. Drops if full.
pub fn push_mouse_event(ev: MouseEvent) {
    let head = MOUSE_HEAD.load(Ordering::Relaxed);
    let tail = MOUSE_TAIL.load(Ordering::Acquire);
    let next = (head + 1) % QUEUE_SIZE;
    if next == tail {
        let d = MOUSE_DROPS.fetch_add(1, Ordering::Relaxed) + 1;
        if (d & 0x3f) == 0 {
            crate::arch::serial::write_str("[input] mouse drops=");
            crate::arch::serial::write_hex(d as u64);
            crate::arch::serial::write_str("\r\n");
        }
        return;
    }
    unsafe {
        MOUSE_QUEUE[head] = ev;
    }
    MOUSE_HEAD.store(next, Ordering::Release);
}

/// Pop a mouse event, if any.
pub fn pop_mouse_event() -> Option<MouseEvent> {
    let tail = MOUSE_TAIL.load(Ordering::Relaxed);
    let head = MOUSE_HEAD.load(Ordering::Acquire);
    if tail == head {
        return None;
    }
    let ev = unsafe { MOUSE_QUEUE[tail] };
    let next = (tail + 1) % QUEUE_SIZE;
    MOUSE_TAIL.store(next, Ordering::Release);
    Some(ev)
}

// --- Driver trait implementation ---

/// Input queue as the concrete InputSource implementation (fed by USB HID).
pub struct InputQueue;

impl crate::drivers::traits::InputSource for InputQueue {
    fn pop_event(&self) -> Option<KeyEvent> {
        pop_event()
    }
}

static INPUT_QUEUE: InputQueue = InputQueue;

/// Return the kernel's input source (trait object). Syscall layer uses this.
pub fn get_input_source() -> &'static dyn crate::drivers::traits::InputSource {
    &INPUT_QUEUE
}
