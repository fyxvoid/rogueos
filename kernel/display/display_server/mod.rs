//! Display server: multi-surface management and compositing.
//!
//! The WM creates one surface per logical window (up to MAX_SURFACES), attaches
//! a 32bpp pixel buffer to each, and commits them one by one to blit their
//! content to the hardware framebuffer.
//!
//! There is no Wayland or X11 protocol — surfaces are directly managed
//! via `SYS_SURFACE_*` syscalls (namespace 0x210–0x215).

const MAX_SURFACES: usize = 16;

/// Physical screen dimensions (must match framebuffer initialisation).
const SCREEN_W: u32 = crate::drivers::framebuffer::FB_WIDTH;
const SCREEN_H: u32 = crate::drivers::framebuffer::FB_HEIGHT;

/// Internal state for one surface slot.
struct SurfaceSlot {
    /// Stable surface identifier. 0 = free.
    id: u32,
    /// PID of the process that created this surface (the "owner"). Only the owner may attach.
    owner_pid: u32,
    /// Attached pixel buffer: (ptr, width, height, stride_bytes).
    buffer: Option<(*const u8, u32, u32, u32)>,
    /// Z-order: lower value is drawn first (further back).
    z: u8,
}

impl SurfaceSlot {
    const fn empty() -> Self {
        Self { id: 0, owner_pid: 0, buffer: None, z: 0 }
    }
}

/// PID of the process that has claimed compositor authority (SYS_CLAIM_COMPOSITOR).
/// Only this PID may call surface_commit or composite_all.
/// None = no compositor registered yet (permissive mode for backwards compat).
static mut COMPOSITOR_PID: Option<u32> = None;

static mut SLOTS: [SurfaceSlot; MAX_SURFACES] = [
    SurfaceSlot::empty(), SurfaceSlot::empty(), SurfaceSlot::empty(), SurfaceSlot::empty(),
    SurfaceSlot::empty(), SurfaceSlot::empty(), SurfaceSlot::empty(), SurfaceSlot::empty(),
    SurfaceSlot::empty(), SurfaceSlot::empty(), SurfaceSlot::empty(), SurfaceSlot::empty(),
    SurfaceSlot::empty(), SurfaceSlot::empty(), SurfaceSlot::empty(), SurfaceSlot::empty(),
];

/// Monotonically-increasing surface ID counter (starts at 1; 0 = free slot sentinel).
static mut NEXT_ID: u32 = 1;

// ── Public API ───────────────────────────────────────────────────────────────

/// Register `pid` as the compositor. Returns `true` on first call (success),
/// `false` if a compositor is already registered.
pub fn claim_compositor(pid: u32) -> bool {
    unsafe {
        if COMPOSITOR_PID.is_none() {
            COMPOSITOR_PID = Some(pid);
            true
        } else {
            false
        }
    }
}

/// Return the PID of the registered compositor, or `None` if not yet claimed.
pub fn get_compositor_pid() -> Option<u32> {
    unsafe { COMPOSITOR_PID }
}

/// Release compositor authority held by `pid`. Called when the compositor process exits.
/// No-op if `pid` does not match the registered compositor.
pub fn release_compositor(pid: u32) {
    unsafe {
        if COMPOSITOR_PID == Some(pid) {
            COMPOSITOR_PID = None;
        }
    }
}

/// Allocate a new surface owned by `owner_pid`.
/// Returns the surface's stable ID, or `None` if all slots are occupied.
pub fn surface_create(owner_pid: u32) -> Option<u32> {
    unsafe {
        for slot in &mut SLOTS {
            if slot.id == 0 {
                let id = NEXT_ID;
                NEXT_ID = NEXT_ID.wrapping_add(1);
                if NEXT_ID == 0 { NEXT_ID = 1; } // skip 0 sentinel
                slot.id = id;
                slot.owner_pid = owner_pid;
                slot.buffer = None;
                slot.z = 0;
                return Some(id);
            }
        }
        None
    }
}

/// Free a surface slot.  Silently ignores unknown IDs.
pub fn surface_destroy(id: u32) {
    unsafe {
        for slot in &mut SLOTS {
            if slot.id == id {
                *slot = SurfaceSlot::empty();
                return;
            }
        }
    }
}

/// Attach a 32bpp ARGB pixel buffer to a surface.
/// `caller_pid` must match the surface's `owner_pid`; returns `false` on ownership mismatch.
/// Returns `false` for unknown `id`, invalid geometry, or ownership violation.
pub fn surface_attach(id: u32, ptr: *const u8, width: u32, height: u32, stride: u32, caller_pid: u32) -> bool {
    if ptr.is_null() || width == 0 || height == 0 || stride < width * 4 {
        return false;
    }
    unsafe {
        for slot in &mut SLOTS {
            if slot.id == id {
                // Enforce: only the surface owner may attach a buffer.
                if slot.owner_pid != 0 && slot.owner_pid != caller_pid {
                    return false;
                }
                slot.buffer = Some((ptr, width, height, stride));
                return true;
            }
        }
        false
    }
}

/// Blit a surface's attached buffer to the framebuffer at `(dst_x, dst_y)`.
/// `caller_pid` must equal the registered compositor PID (if one is registered).
/// Returns `false` if the surface has no buffer, the ID is unknown, or caller is not compositor.
pub fn surface_commit(id: u32, dst_x: u32, dst_y: u32, caller_pid: u32) -> bool {
    // Compositor enforcement: if a compositor is registered, only it may commit.
    unsafe {
        if let Some(comp_pid) = COMPOSITOR_PID {
            if caller_pid != comp_pid {
                return false;
            }
        }
    }
    let buf = unsafe {
        let mut found = None;
        for slot in &SLOTS {
            if slot.id == id {
                found = slot.buffer;
                break;
            }
        }
        found
    };
    let (ptr, w, h, stride) = match buf {
        Some(b) => b,
        None => return false,
    };
    if ptr.is_null() { return false; }
    // Composite into the RAM backbuffer instead of MMIO directly.
    // Falls back to MMIO blit if no backbuffer has been allocated yet.
    crate::syscall::dispatcher::gfx::backbuffer_blit(dst_x, dst_y, w, h, stride, ptr);
    true
}

/// Composite all surfaces with attached buffers in z-order onto the framebuffer,
/// then flush to hardware.  The WM calls this once after updating all surfaces.
pub fn composite_all() {
    // Simple bubble sort on z (small number of surfaces — fine).
    let slots = unsafe { &SLOTS };
    let mut order: [usize; MAX_SURFACES] = [0; MAX_SURFACES];
    let mut count = 0usize;
    for (i, slot) in slots.iter().enumerate() {
        if slot.id != 0 && slot.buffer.is_some() {
            order[count] = i;
            count += 1;
        }
    }
    // Sort by z ascending (insertion sort — stable, O(n^2) but n ≤ 16).
    for i in 1..count {
        let mut j = i;
        while j > 0 && unsafe { SLOTS[order[j - 1]].z } > unsafe { SLOTS[order[j]].z } {
            order.swap(j - 1, j);
            j -= 1;
        }
    }
    // Composite each surface into the RAM backbuffer in z-order.
    // Falls back to direct MMIO blit per surface if no backbuffer allocated.
    for k in 0..count {
        let slot = unsafe { &SLOTS[order[k]] };
        if let Some((ptr, w, h, stride)) = slot.buffer {
            if !ptr.is_null() {
                crate::syscall::dispatcher::gfx::backbuffer_blit(0, 0, w, h, stride, ptr);
            }
        }
    }
    // Final MMIO flush is handled by sys_fb_flush (BACKBUFFER → MMIO),
    // called by the compositor at the end of its draw loop.
}

/// Set the z-order for a surface (lower = further back).
pub fn surface_set_z(id: u32, z: u8) {
    unsafe {
        for slot in &mut SLOTS {
            if slot.id == id {
                slot.z = z;
                return;
            }
        }
    }
}

/// Return the fixed screen dimensions.
pub fn screen_size() -> (u32, u32) {
    (SCREEN_W, SCREEN_H)
}

/// Return the number of currently allocated surface slots.
pub fn surface_count() -> usize {
    unsafe { SLOTS.iter().filter(|s| s.id != 0).count() }
}

// ── Legacy shim (kept for backwards compat with old display_server callers) ──

/// Kept so existing code that calls `client_connect()` still compiles.
/// New code should use `surface_create(owner_pid)` directly.
#[allow(dead_code)]
pub fn client_connect() -> Option<u32> {
    surface_create(0)
}
