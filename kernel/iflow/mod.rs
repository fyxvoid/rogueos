//! Information Flow Control (IFC) model.
//!
//! Every process carries an [`IflowLabel`] with two orthogonal axes:
//!
//! - **Secrecy** (`u64` bitmask): tags for sensitive data categories. A bit set
//!   means this process holds data secret in that category. A process with
//!   secrecy tag S can only write to sinks that *also* carry tag S — no write-down.
//!
//! - **Integrity** (`u64` bitmask): tags for trusted data categories. A bit set
//!   means this process produces data that meets that integrity level. A process
//!   can only write to sinks whose integrity *requirements* it satisfies — no
//!   write-up to a higher-integrity sink from a lower-integrity source.
//!
//! ## Flow rule
//!
//! ```text
//! can_flow(src, dst) ⟺
//!     (src.secrecy  & !dst.secrecy)  == 0   // dst is cleared for all of src's secrets
//!  && (dst.integrity & !src.integrity) == 0  // src meets all of dst's integrity demands
//! ```
//!
//! ## Label operations
//!
//! | Operation | Who may call | Effect |
//! |-----------|-------------|--------|
//! | `SYS_IFLOW_GET`         | anyone        | read any process's label            |
//! | `SYS_IFLOW_TAINT`       | self only     | raise secrecy, lower integrity      |
//! | `SYS_IFLOW_DECLASSIFY`  | CAP_DECLASSIFY| lower secrecy (privileged)          |
//! | `SYS_IFLOW_ENDORSE`     | CAP_ENDORSE   | raise integrity of a process        |
//!
//! ## Predefined tags
//!
//! See [`secrecy`] and [`integrity`] sub-modules.
//!
//! ## Enforcement points
//!
//! - `SYS_IPC_SEND`: sender cannot write to a process it cannot flow to.
//! - `SYS_SPAWN`:    child inherits parent label; parent cannot spawn with lower secrecy.
//! - `SYS_WRITE` fd=1/2: kernel writes are always permitted; user writes to TTY are
//!   checked against a public (zero secrecy) sink label.

use libs::cap;

// ── Predefined tag namespaces ─────────────────────────────────────────────

/// Secrecy tag bits. Set a bit to mark data as sensitive in that category.
pub mod secrecy {
    /// Password / key material held by this process.
    pub const CREDENTIAL: u64 = 1 << 0;
    /// Per-session state that should not cross session boundaries.
    pub const SESSION:    u64 = 1 << 1;
    /// Data that arrived from an untrusted network source.
    pub const NETWORK:    u64 = 1 << 2;
    /// Data read from persistent storage (files, nvme).
    pub const FILE:       u64 = 1 << 3;
    /// Kernel-internal trace / debug data (crash logs, PTW dumps).
    pub const DEBUG:      u64 = 1 << 4;
}

/// Integrity tag bits. Set a bit to declare that this process produces trusted data.
pub mod integrity {
    /// Data originated from or was verified by the kernel.
    pub const KERNEL:  u64 = 1 << 0;
    /// Data was verified / endorsed by Cogman (PID 1).
    pub const COGMAN:  u64 = 1 << 1;
    /// Data has been cryptographically signed (future).
    pub const SIGNED:  u64 = 1 << 2;
}

// ── IflowLabel ────────────────────────────────────────────────────────────

/// Per-process information flow label.
#[derive(Clone, Copy)]
pub struct IflowLabel {
    /// Secrecy tags: bits that must also be set in any destination.
    pub secrecy: u64,
    /// Integrity tags: bits that this process provides to any destination.
    pub integrity: u64,
}

impl IflowLabel {
    /// Cogman (PID 1): public secrecy (no secrets), fully trusted integrity.
    pub const fn cogman() -> Self {
        IflowLabel {
            secrecy: 0,
            integrity: u64::MAX,
        }
    }

    /// Default for freshly spawned processes: public, untrusted.
    pub const fn default_user() -> Self {
        IflowLabel {
            secrecy: 0,
            integrity: 0,
        }
    }

    /// A process may taint itself: raise secrecy or lower integrity.
    /// These operations can never increase authority; they are always permitted.
    #[inline]
    pub fn taint(&mut self, add_secrecy: u64, remove_integrity: u64) {
        self.secrecy  |= add_secrecy;
        self.integrity &= !remove_integrity;
    }

    /// Lower secrecy (declassify). Requires `CAP_DECLASSIFY`.
    #[inline]
    pub fn declassify(&mut self, remove_secrecy: u64) {
        self.secrecy &= !remove_secrecy;
    }

    /// Raise integrity of a process (endorse). Requires `CAP_ENDORSE`.
    #[inline]
    pub fn endorse(&mut self, add_integrity: u64) {
        self.integrity |= add_integrity;
    }

    /// Compute the label a child should be born with (inherits parent's full label).
    #[inline]
    pub fn child_label(&self) -> IflowLabel {
        *self
    }
}

// ── Flow check ────────────────────────────────────────────────────────────

/// Returns `true` if information is permitted to flow from `src` to `dst`.
///
/// Two conditions must hold:
/// 1. **No write-down for secrecy** — `dst` must be cleared for every secret `src` holds.
/// 2. **No write-up for integrity** — `src` must satisfy every integrity tag `dst` requires.
#[inline]
pub fn can_flow(src: IflowLabel, dst: IflowLabel) -> bool {
    let secrecy_ok  = (src.secrecy  & !dst.secrecy)  == 0;
    let integrity_ok = (dst.integrity & !src.integrity) == 0;
    secrecy_ok && integrity_ok
}

/// The public output sink (TTY, display, IPC to unprivileged process with no label):
/// zero secrecy required, zero integrity provided — the most permissive destination.
pub const PUBLIC_SINK: IflowLabel = IflowLabel { secrecy: 0, integrity: 0 };

// ── Kernel enforcement helpers ────────────────────────────────────────────

/// Check that information can flow from the current process to a target with label `dst`.
/// Logs the violation to serial and returns `Err(PERM)` on denial.
pub fn check_flow_to(
    dst: IflowLabel,
    ctx: &str,
) -> Result<(), crate::syscall::user_ptr::SysErr> {
    let src = current_label();
    if can_flow(src, dst) {
        return Ok(());
    }
    let pid = crate::process::current_pid().unwrap_or(0);
    crate::arch::serial::write_str("[IFC] DENIED pid=");
    crate::arch::serial::write_hex(pid as u64);
    crate::arch::serial::write_str(" ctx=");
    crate::arch::serial::write_str(ctx);
    crate::arch::serial::write_str(" src.sec=");
    crate::arch::serial::write_hex(src.secrecy);
    crate::arch::serial::write_str(" dst.sec=");
    crate::arch::serial::write_hex(dst.secrecy);
    crate::arch::serial::write_str(" src.int=");
    crate::arch::serial::write_hex(src.integrity);
    crate::arch::serial::write_str(" dst.int=");
    crate::arch::serial::write_hex(dst.integrity);
    crate::arch::serial::write_str("\r\n");
    Err(crate::syscall::user_ptr::SysErr::PERM)
}

/// Return the IflowLabel of the currently running process, or the public default.
pub fn current_label() -> IflowLabel {
    crate::process::current_descriptor()
        .map(|d| d.iflow)
        .unwrap_or(IflowLabel::default_user())
}

/// Return the IflowLabel for a process by index, or `None`.
pub fn label_of_index(idx: usize) -> Option<IflowLabel> {
    crate::process::get_descriptor(idx).map(|d| d.iflow)
}

// ── Syscall implementations ───────────────────────────────────────────────

/// `SYS_IFLOW_GET(pid, out_secrecy *mut u64, out_integrity *mut u64)` — public query.
pub fn sys_iflow_get(
    pid: u32,
    out_sec: *mut u64,
    out_int: *mut u64,
) -> Result<u64, crate::syscall::user_ptr::SysErr> {
    use crate::syscall::user_ptr;
    let cr3 = user_ptr::current_cr3()?;
    user_ptr::validate_user_range(cr3, out_sec as u64, 8, true)?;
    user_ptr::validate_user_range(cr3, out_int as u64, 8, true)?;

    let idx = crate::process::index_of_pid(pid).ok_or(user_ptr::SysErr::NOENT)?;
    let label = crate::process::get_descriptor(idx)
        .map(|d| d.iflow)
        .ok_or(user_ptr::SysErr::NOENT)?;

    unsafe {
        core::ptr::write(out_sec, label.secrecy);
        core::ptr::write(out_int, label.integrity);
    }
    Ok(0)
}

/// `SYS_IFLOW_TAINT(add_secrecy u64, remove_integrity u64)` — raise own secrecy / lower own integrity.
/// No capability required: making yourself more restricted is always safe.
pub fn sys_iflow_taint(
    add_secrecy: u64,
    remove_integrity: u64,
) -> Result<u64, crate::syscall::user_ptr::SysErr> {
    let idx = crate::process::current_index()
        .ok_or(crate::syscall::user_ptr::SysErr::INVAL)?;
    if let Some(pcb) = crate::process::get_descriptor_mut(idx) {
        pcb.iflow.taint(add_secrecy, remove_integrity);
    }
    Ok(0)
}

/// `SYS_IFLOW_DECLASSIFY(remove_secrecy u64)` — lower own secrecy. Requires `CAP_DECLASSIFY`.
pub fn sys_iflow_declassify(
    remove_secrecy: u64,
) -> Result<u64, crate::syscall::user_ptr::SysErr> {
    crate::capability::require(cap::DECLASSIFY, "iflow_declassify")?;
    let idx = crate::process::current_index()
        .ok_or(crate::syscall::user_ptr::SysErr::INVAL)?;
    if let Some(pcb) = crate::process::get_descriptor_mut(idx) {
        pcb.iflow.declassify(remove_secrecy);
    }
    Ok(0)
}

/// `SYS_IFLOW_ENDORSE(pid u32, add_integrity u64)` — raise target's integrity. Requires `CAP_ENDORSE`.
pub fn sys_iflow_endorse(
    target_pid: u32,
    add_integrity: u64,
) -> Result<u64, crate::syscall::user_ptr::SysErr> {
    crate::capability::require(cap::ENDORSE, "iflow_endorse")?;
    let idx = crate::process::index_of_pid(target_pid)
        .ok_or(crate::syscall::user_ptr::SysErr::NOENT)?;
    if let Some(pcb) = crate::process::get_descriptor_mut(idx) {
        pcb.iflow.endorse(add_integrity);
    }
    Ok(0)
}
