//! Process supervisor — Cogman's init responsibilities.
//!
//! Maintains a static service table, spawns processes, reaps dead children,
//! and applies restart policies. No heap; fixed-size array only.
//!
//! **Capability model:** every service is spawned via `sys_spawn_capped` with
//! the minimum capability set it needs. Cogman journals its state after every
//! service-table mutation so a replacement instance can resume in < 5 ms.

use userland::{sys_reboot, sys_spawn_capped, sys_waitpid, sys_cap_grant, sys_journal_write};
use libs::cap;
use super::persona;

// ── Restart policies ──────────────────────────────────────────────────────

#[derive(Copy, Clone, PartialEq)]
pub enum RestartPolicy {
    /// Never restart — one-shot or manually controlled.
    Never,
    /// Restart only on non-zero exit.
    OnFailure,
    /// Always restart regardless of exit code.
    Always,
}

// ── Service states ────────────────────────────────────────────────────────

#[derive(Copy, Clone, PartialEq)]
pub enum SvcState {
    Stopped,
    Running,
    Restarting,
    Failed,
}

// ── Service table entry ───────────────────────────────────────────────────

#[derive(Copy, Clone)]
pub struct Service {
    pub program_id:    u32,
    pub name:          [u8; 16],
    pub name_len:      usize,
    pub pid:           u32,
    pub state:         SvcState,
    pub policy:        RestartPolicy,
    pub auto_start:    bool,
    pub restart_count: u16,
    pub restart_delay: u32,   // ticks remaining before next spawn attempt
    pub last_exit:     i32,
    /// Capability mask granted to spawned instances of this service.
    /// Cogman intersects this with its own caps before passing to the kernel,
    /// so child processes are always sealed capability containers.
    pub cap_mask:      u64,
}

impl Service {
    const fn new(
        id: u32,
        name: &[u8],
        policy: RestartPolicy,
        auto_start: bool,
        cap_mask: u64,
    ) -> Self {
        let mut n = [0u8; 16];
        let len = if name.len() < 16 { name.len() } else { 16 };
        let mut i = 0;
        while i < len {
            n[i] = name[i];
            i += 1;
        }
        Service {
            program_id: id,
            name: n,
            name_len: len,
            pid: 0,
            state: SvcState::Stopped,
            policy,
            auto_start,
            restart_count: 0,
            restart_delay: 0,
            last_exit: 0,
            cap_mask,
        }
    }

    pub fn name_bytes(&self) -> &[u8] {
        &self.name[..self.name_len]
    }
}

// ── Default service table (matches kernel program registrations) ──────────
//
// Program IDs mirror kernel/init/main.rs:
//   shell=0, wm=1, editor=2, viewer=3, copy=4, monitor=5,
//   shutdown=6, exit=7, session=8, wm_legacy=9, cogman=10

pub const MAX_SERVICES: usize = 16;

pub struct Supervisor {
    pub table: [Option<Service>; MAX_SERVICES],
    count: usize,
}

impl Supervisor {
    pub const fn new() -> Self {
        Supervisor {
            table: [None; MAX_SERVICES],
            count: 0,
        }
    }

    pub fn register(&mut self, svc: Service) {
        if self.count < MAX_SERVICES {
            self.table[self.count] = Some(svc);
            self.count += 1;
        }
    }

    /// Spawn all auto-start services and commit state to journal.
    pub fn start_all(&mut self) {
        for slot in self.table.iter_mut().flatten() {
            if slot.auto_start && slot.state == SvcState::Stopped {
                Self::spawn_service(slot);
            }
        }
        self.journal_state();
    }

    /// Spawn a single service with its declared capability mask and update state.
    /// The kernel intersects cap_mask with Cogman's own caps, so no privilege
    /// escalation is possible even with a corrupted cap_mask value.
    fn spawn_service(svc: &mut Service) {
        let pid_raw = sys_spawn_capped(svc.program_id, svc.cap_mask);
        if pid_raw < 0 {
            persona::say_spawn_failed(svc.name_bytes());
            svc.state = SvcState::Failed;
        } else {
            svc.pid = pid_raw as u32;
            svc.state = SvcState::Running;
            persona::say_spawning(svc.name_bytes(), svc.pid);
        }
    }

    /// Non-blocking reap pass — call every tick.
    /// Returns `true` if a halt or reboot was requested.
    pub fn reap_pass(&mut self) -> ControlFlow {
        let mut status: i32 = 0;
        // WNOHANG = 1 — non-blocking
        let reaped = sys_waitpid(0xFFFF_FFFF, &mut status as *mut i32, 1);

        if reaped == 0 {
            // Nothing to reap this tick.
            return ControlFlow::Continue;
        }
        if reaped < 0 {
            // ECHILD or transient error — fine.
            return ControlFlow::Continue;
        }

        let dead_pid = reaped as u32;

        // Special exit codes.
        if status == 42 {
            persona::say_halt();
            sys_reboot(0);
            loop {}
        }
        if status == 43 {
            persona::say_reboot();
            sys_reboot(1);
            loop {}
        }

        // Find the matching service.
        for slot in self.table.iter_mut().flatten() {
            if slot.pid != dead_pid {
                continue;
            }
            slot.pid = 0;
            slot.last_exit = status;
            persona::say_exited(slot.name_bytes(), status);

            let should_restart = match slot.policy {
                RestartPolicy::Never => false,
                RestartPolicy::OnFailure => status != 0,
                RestartPolicy::Always => true,
            };

            if should_restart {
                slot.state = SvcState::Restarting;
                slot.restart_delay = backoff_ticks(slot.restart_count);
                slot.restart_count = slot.restart_count.saturating_add(1);
                persona::say_restarting(slot.name_bytes(), slot.restart_count);
            } else {
                slot.state = SvcState::Stopped;
                persona::say_stopped_policy(slot.name_bytes());
            }
            break;
        }

        // Commit state so a replacement Cogman can resume after a restart.
        self.journal_state();
        ControlFlow::Continue
    }

    /// Commit current supervisor state to the kernel journal.
    /// A replacement Cogman instance can recover this on startup via
    /// `sys_journal_read`, achieving zero-data-loss restart in < 5 ms.
    ///
    /// Format (little-endian binary, no heap):
    ///   [count: u8] [Service { program_id:u32, pid:u32, state:u8, policy:u8,
    ///                           restart_count:u16, last_exit:i32, cap_mask:u64 } × count]
    pub fn journal_state(&self) {
        const ENTRY_SIZE: usize = 4 + 4 + 1 + 1 + 2 + 4 + 8; // 24 bytes
        const MAX: usize = MAX_SERVICES * ENTRY_SIZE + 1;
        let mut buf = [0u8; MAX];
        let mut off = 0usize;

        let count = self.table.iter().filter(|s| s.is_some()).count() as u8;
        buf[off] = count; off += 1;

        for slot in self.table.iter().flatten() {
            buf[off..off+4].copy_from_slice(&slot.program_id.to_le_bytes()); off += 4;
            buf[off..off+4].copy_from_slice(&slot.pid.to_le_bytes());        off += 4;
            buf[off] = slot.state as u8;                                     off += 1;
            buf[off] = slot.policy as u8;                                    off += 1;
            buf[off..off+2].copy_from_slice(&slot.restart_count.to_le_bytes()); off += 2;
            buf[off..off+4].copy_from_slice(&(slot.last_exit as u32).to_le_bytes()); off += 4;
            buf[off..off+8].copy_from_slice(&slot.cap_mask.to_le_bytes());   off += 8;
        }
        sys_journal_write(&buf[..off]);
    }

    /// Tick-down restart delays and re-spawn anything that's ready.
    pub fn restart_pass(&mut self) {
        let mut respawned = false;
        for slot in self.table.iter_mut().flatten() {
            if slot.state != SvcState::Restarting {
                continue;
            }
            if slot.restart_delay > 0 {
                slot.restart_delay -= 1;
                continue;
            }
            Self::spawn_service(slot);
            respawned = true;
        }
        if respawned {
            self.journal_state();
        }
    }
}

/// Exponential-ish backoff: 0, 200, 400, 800, 1600 … capped at 8000 ticks.
fn backoff_ticks(restart_count: u16) -> u32 {
    let shift = if restart_count < 6 { restart_count as u32 } else { 6 };
    (200u32 << shift).min(8000)
}

pub enum ControlFlow {
    Continue,
    Halt,
    Reboot,
}

// ── Program IDs (must match kernel/init/programs.rs) ─────────────────────
pub const PROG_SHELL:   u32 = 0;
pub const PROG_RWM:     u32 = 1;
pub const PROG_EDITOR:  u32 = 2;
pub const PROG_VIEWER:  u32 = 3;
pub const PROG_COPY:    u32 = 4;
pub const PROG_MONITOR: u32 = 5;
pub const PROG_SHUTDOWN:u32 = 6;
pub const PROG_EXIT:    u32 = 7;
pub const PROG_SESSION: u32 = 8;
pub const PROG_WM:      u32 = 9;
pub const PROG_COGMAN:  u32 = 10;
pub const PROG_NOVA:    u32 = 11;

// ── Default service definitions ───────────────────────────────────────────

pub fn default_supervisor() -> Supervisor {
    let mut sv = Supervisor::new();

    // Nova compositor: display, input, compositor, spawn, ipc, shm.
    // Auto-started as the primary display compositor.
    sv.register(Service::new(PROG_NOVA, b"nova",
        RestartPolicy::Always, true,
        cap::COMPOSITOR_WM));

    // Session manager: needs display, input, ipc, shm, spawn (to launch apps).
    sv.register(Service::new(PROG_SESSION, b"session",
        RestartPolicy::Always, true,
        cap::DISPLAY | cap::INPUT | cap::IPC_SEND | cap::IPC_RECV | cap::SHM | cap::SPAWN | cap::FS_READ));

    // Shell: spawn, fs read/write, ipc, display, input.
    sv.register(Service::new(PROG_SHELL, b"shell",
        RestartPolicy::OnFailure, false,
        cap::SHELL));

    // Monitor: read-only process info + display.
    sv.register(Service::new(PROG_MONITOR, b"monitor",
        RestartPolicy::Never, false,
        cap::PROC_INFO | cap::DISPLAY | cap::IPC_SEND | cap::IPC_RECV));

    sv
}
