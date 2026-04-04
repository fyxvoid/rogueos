//! Process supervisor — Cogman's init responsibilities.
//!
//! Maintains a static service table, spawns processes, reaps dead children,
//! and applies restart policies. No heap; fixed-size array only.

use userland::{sys_reboot, sys_spawn, sys_waitpid};
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
}

impl Service {
    const fn new(
        id: u32,
        name: &[u8],
        policy: RestartPolicy,
        auto_start: bool,
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

    /// Spawn all auto-start services.
    pub fn start_all(&mut self) {
        for slot in self.table.iter_mut().flatten() {
            if slot.auto_start && slot.state == SvcState::Stopped {
                Self::spawn_service(slot);
            }
        }
    }

    /// Spawn a single service and update its state.
    fn spawn_service(svc: &mut Service) {
        let pid_raw = sys_spawn(svc.program_id);
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

        ControlFlow::Continue
    }

    /// Tick-down restart delays and re-spawn anything that's ready.
    pub fn restart_pass(&mut self) {
        for slot in self.table.iter_mut().flatten() {
            if slot.state != SvcState::Restarting {
                continue;
            }
            if slot.restart_delay > 0 {
                slot.restart_delay -= 1;
                continue;
            }
            Self::spawn_service(slot);
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

// ── Default service definitions ───────────────────────────────────────────

pub fn default_supervisor() -> Supervisor {
    let mut sv = Supervisor::new();

    // Session manager — always keep alive.
    sv.register(Service::new(8, b"session", RestartPolicy::Always, true));

    // Shell — restart on failure but respect clean exit.
    sv.register(Service::new(0, b"shell", RestartPolicy::OnFailure, false));

    // Desktop WM — restart on failure.
    sv.register(Service::new(1, b"wm", RestartPolicy::OnFailure, false));

    // Monitor — optional, never auto-start.
    sv.register(Service::new(5, b"monitor", RestartPolicy::Never, false));

    sv
}
