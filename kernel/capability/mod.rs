//! Capability-based security model.
//!
//! Every process carries a [`CapSet`] bitmask. Each syscall that touches
//! a sensitive resource checks the relevant bit before proceeding; if the
//! bit is absent the call returns `SYSERR_PERM`.
//!
//! **Bootstrapping:** Cogman (the init supervisor, always pid 1) is born
//! with `cap::ALL`. It is the sole holder of `cap::GRANT` at boot and the
//! only entity that may grant or revoke capabilities on other processes via
//! `SYS_CAP_GRANT` / `SYS_CAP_REVOKE`.
//!
//! **Spawn inheritance:** `SYS_SPAWN(program_id, cap_mask)` creates a child
//! whose capability set is `parent_caps & cap_mask`. A process can never
//! grant a child more authority than it has itself.
//!
//! **Journal:** Cogman persists its supervisor state to a 4 KiB kernel
//! region via `SYS_JOURNAL_WRITE`. On restart the new instance calls
//! `SYS_JOURNAL_READ` to recover state, achieving < 5 ms restart with
//! zero data loss.

pub mod journal;

use libs::cap;

// ── CapSet ─────────────────────────────────────────────────────────────────

/// Per-process capability bitmask.
///
/// All 64 bits are named in [`libs::cap`]. The kernel checks individual bits
/// before dispatching sensitive syscalls; see `require_cap!` below.
#[derive(Clone, Copy)]
pub struct CapSet {
    pub bits: u64,
}

impl CapSet {
    /// Cogman's full authority — all bits set.
    pub const fn all() -> Self {
        CapSet { bits: cap::ALL }
    }

    /// No authority — default for every freshly spawned process.
    pub const fn none() -> Self {
        CapSet { bits: cap::NONE }
    }

    pub const fn from_bits(bits: u64) -> Self {
        CapSet { bits }
    }

    /// Returns true if ALL bits in `mask` are present.
    #[inline]
    pub fn has(&self, mask: u64) -> bool {
        (self.bits & mask) == mask
    }

    /// Set capability bits (used by SYS_CAP_GRANT).
    #[inline]
    pub fn grant(&mut self, bits: u64) {
        self.bits |= bits;
    }

    /// Clear capability bits (used by SYS_CAP_REVOKE).
    #[inline]
    pub fn revoke(&mut self, bits: u64) {
        self.bits &= !bits;
    }

    /// Intersection: produce the capability set a child may receive from this
    /// parent when the parent requests `requested` bits for the child.
    /// The child can never exceed parent authority.
    #[inline]
    pub fn child_caps(&self, requested: u64) -> CapSet {
        // 0 is backwards-compat sentinel: inherit everything parent has.
        let mask = if requested == 0 { cap::ALL } else { requested };
        CapSet { bits: self.bits & mask }
    }
}

// ── Enforcement helpers ────────────────────────────────────────────────────

/// Return the [`CapSet`] of the currently running process, or `CapSet::none()`
/// if there is no current process (should not happen in a syscall path).
pub fn current_caps() -> CapSet {
    crate::process::current_descriptor()
        .map(|d| d.caps)
        .unwrap_or(CapSet::none())
}

/// Check that the current process holds all bits in `required`.
/// Returns `Ok(())` on success, `Err(SysErr::PERM)` on failure,
/// logging the violation to serial for audit.
pub fn require(required: u64, name: &str) -> Result<(), crate::syscall::user_ptr::SysErr> {
    let caps = current_caps();
    if caps.has(required) {
        return Ok(());
    }
    let pid = crate::process::current_pid().unwrap_or(0);
    crate::arch::serial::write_str("[CAP] DENIED pid=");
    crate::arch::serial::write_hex(pid as u64);
    crate::arch::serial::write_str(" syscall=");
    crate::arch::serial::write_str(name);
    crate::arch::serial::write_str(" missing=");
    crate::arch::serial::write_hex(required & !caps.bits);
    crate::arch::serial::write_str("\r\n");
    Err(crate::syscall::user_ptr::SysErr::PERM)
}
