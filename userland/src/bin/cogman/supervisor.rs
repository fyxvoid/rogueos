//! Process supervisor — Cogman's init responsibilities.
//!
//! Maintains a static service table, spawns processes, reaps dead children,
//! and applies restart policies. No heap; fixed-size array only.
//!
//! **Capability model:** every service is spawned via `sys_spawn_capped` with
//! the minimum capability set it needs. Cogman journals its state after every
//! service-table mutation so a replacement instance can resume in < 5 ms.

use userland::{sys_reboot, sys_spawn_capped, sys_waitpid, sys_journal_write};
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

// ── Static name lookup (names live in .rodata, never on the stack) ────────

pub fn prog_name(id: u32) -> &'static [u8] {
    match id {
        0  => b"shell",
        1  => b"rwm",
        2  => b"editor",
        3  => b"viewer",
        4  => b"copy",
        5  => b"monitor",
        6  => b"shutdown",
        7  => b"exit",
        8  => b"session",
        9  => b"wm",
        10 => b"cogman",
        11 => b"fbtest",
        12 => b"terminal",
        _  => b"unknown",
    }
}

// ── Service table entry ───────────────────────────────────────────────────

#[derive(Copy, Clone)]
pub struct Service {
    pub program_id:    u32,
    pub pid:           u32,
    pub state:         SvcState,
    pub policy:        RestartPolicy,
    pub auto_start:    bool,
    pub restart_count: u16,
    pub restart_delay: u32,
    pub last_exit:     i32,
    /// Capability mask granted to spawned instances of this service.
    pub cap_mask:      u64,
}

impl Service {
    const fn new(
        id: u32,
        _name: &[u8],
        policy: RestartPolicy,
        auto_start: bool,
        cap_mask: u64,
    ) -> Self {
        Service {
            program_id: id,
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

    pub fn name_bytes(&self) -> &'static [u8] {
        prog_name(self.program_id)
    }
}

// ── Default service table (matches kernel program registrations) ──────────
//
// Program IDs mirror kernel/init/main.rs:
//   shell=0, rwm=1, editor=2, viewer=3, copy=4, monitor=5,
//   shutdown=6, exit=7, session=8, wm=9, cogman=10, fbtest=11, terminal=12

const PROG_FBTEST:    u32 = 11;
const PROG_WM:        u32 = 9;
const PROG_TERMINAL:  u32 = 12;

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

    /// Blocking reap pass — blocks until a child exits, yielding CPU to other processes.
    /// Returns `true` if a halt or reboot was requested.
    pub fn reap_pass(&mut self) -> ControlFlow {
        let mut status: i32 = 0;
        // Block until any child exits (options=0). This yields CPU to the WM.
        let reaped = sys_waitpid(0xFFFF_FFFF, &mut status as *mut i32, 0);

        if reaped <= 0 {
            // ECHILD or error — no children, nothing to reap.
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

        // Find the matching service and handle its policy.
        // Also record if fbtest just finished cleanly so we can chain-start wm.
        let mut chain_wm = false;

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
                // fbtest finished → hand off framebuffer to wm.
                if slot.program_id == PROG_FBTEST {
                    chain_wm = true;
                }
            }
            break;
        }

        // Chain: after fbtest exits, start wm (which claims the now-free compositor).
        if chain_wm {
            for slot in self.table.iter_mut().flatten() {
                if slot.program_id == PROG_WM && slot.state == SvcState::Stopped {
                    Self::spawn_service(slot);
                    break;
                }
            }
        }

        // Commit state so a replacement Cogman can resume after a restart.
        self.journal_state();
        ControlFlow::Continue
    }

    /// Commit current supervisor state to the kernel journal.
    pub fn journal_state(&self) {
        const ENTRY_SIZE: usize = 4 + 4 + 1 + 1 + 2 + 4 + 8; // 24 bytes per entry
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

// ── Default service definitions ───────────────────────────────────────────

pub fn default_supervisor() -> Supervisor {
    let mut sv = Supervisor::new();

    // WM / compositor: display, input, compositor, spawn, ipc, shm.
    sv.register(Service::new(PROG_WM, b"wm",
        RestartPolicy::OnFailure, true,
        cap::COMPOSITOR_WM));

    // Terminal: display, input, ipc, shm. Spawned by wm.
    sv.register(Service::new(PROG_TERMINAL, b"terminal",
        RestartPolicy::Never, false,
        cap::DESKTOP_APP));

    // Monitor: read-only process info + display.
    sv.register(Service::new(5, b"monitor",
        RestartPolicy::Never, false,
        cap::PROC_INFO | cap::DISPLAY | cap::IPC_SEND | cap::IPC_RECV));

    sv
}
