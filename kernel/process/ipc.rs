//! Per-process IPC ring buffer.
//!
//! Each process gets a fixed-depth queue of [`RwmMsg`] (64 bytes each).
//! All operations are O(1) and require no heap allocation.
//!
//! Safety: the kernel is single-core and interrupts are disabled during
//! syscall dispatch, so bare static-mut access is safe here.

use libs::RwmMsg;
use super::process::MAX_PROCESSES;

/// Depth of each per-process IPC queue (messages, not bytes).
pub const IPC_QUEUE_DEPTH: usize = 32;

struct IpcRing {
    buf:  [RwmMsg; IPC_QUEUE_DEPTH],
    head: usize, // index of next message to read
    tail: usize, // index of next slot to write
    len:  usize, // number of messages currently held
}

impl IpcRing {
    const fn new() -> Self {
        Self {
            buf:  [RwmMsg::ZERO; IPC_QUEUE_DEPTH],
            head: 0,
            tail: 0,
            len:  0,
        }
    }

    fn enqueue(&mut self, msg: RwmMsg) -> bool {
        if self.len == IPC_QUEUE_DEPTH {
            return false; // queue full
        }
        self.buf[self.tail] = msg;
        self.tail = (self.tail + 1) % IPC_QUEUE_DEPTH;
        self.len += 1;
        true
    }

    fn dequeue(&mut self) -> Option<RwmMsg> {
        if self.len == 0 {
            return None;
        }
        let msg = self.buf[self.head];
        self.head = (self.head + 1) % IPC_QUEUE_DEPTH;
        self.len -= 1;
        Some(msg)
    }

    fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.len  = 0;
    }
}

/// One ring per process slot (indexed by process table index, not PID).
static mut IPC_QUEUES: [IpcRing; MAX_PROCESSES] = [const { IpcRing::new() }; MAX_PROCESSES];

/// Enqueue `msg` into the queue for process at table index `idx`.
/// Returns `true` on success, `false` if the queue is full or `idx` is out of range.
#[inline]
pub fn ipc_enqueue(idx: usize, msg: RwmMsg) -> bool {
    if idx >= MAX_PROCESSES {
        return false;
    }
    unsafe { IPC_QUEUES[idx].enqueue(msg) }
}

/// Dequeue the oldest message for process at table index `idx`.
/// Returns `None` if the queue is empty or `idx` is out of range.
#[inline]
pub fn ipc_dequeue(idx: usize) -> Option<RwmMsg> {
    if idx >= MAX_PROCESSES {
        return None;
    }
    unsafe { IPC_QUEUES[idx].dequeue() }
}

/// Discard all pending messages for the process at table index `idx`.
/// Call this when a process exits so stale messages do not leak to the next
/// process that inherits the same table slot.
#[inline]
pub fn ipc_clear(idx: usize) {
    if idx >= MAX_PROCESSES {
        return;
    }
    unsafe { IPC_QUEUES[idx].clear() }
}
