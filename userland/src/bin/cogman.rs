//! cogman — kingdom OS init, supervisor, and process healer.
//!
//! This is the first userland process spawned by the kernel (replaces the
//! minimal "steward" init). It:
//!   1. Spawns all auto-start services (session, shell, …)
//!   2. Reaps dead children and restarts them per policy
//!   3. Handles CogCtrl IPC messages so any process can query/control services
//!
//! Constraints: no_std, no heap (fixed-size arrays), no signals.
//! IPC uses kingdom KwmMsg / SYS_IPC_SEND / SYS_IPC_RECV.

#![no_std]
#![no_main]

use libs::{
    IPC_NONBLOCK, KwmMsg, KwmPayload, KwmType, PayloadCogCtrl, SYSERR_AGAIN, WNOHANG,
};
use userland::{sys_ipc_recv, sys_ipc_send, sys_poll_input, sys_spawn, sys_waitpid, sys_write};

// ── Tuning ────────────────────────────────────────────────────────────────

/// Maximum number of supervised services.
const MAX_SVCS: usize = 16;

/// How many poll_input ticks to spin between reap/IPC checks (~10 ms per iteration).
const POLL_TICKS: u32 = 1000;

/// How many supervisor loop iterations to wait before a restart attempt.
const RESTART_DELAY_ITERS: u32 = 30; // ~300 ms

// ── Program IDs (must match kernel/audits/main.rs registration) ──────────

const PROG_SHELL:    u32 = 0;
const PROG_RWM:      u32 = 1;
const PROG_EDITOR:   u32 = 2;
const PROG_VIEWER:   u32 = 3;
const PROG_COPY:     u32 = 4;
const PROG_MONITOR:  u32 = 5;
const PROG_SHUTDOWN: u32 = 6;
const PROG_SESSION:  u32 = 8;

// ── Service state ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum SvcState {
    Stopped    = 0,
    Running    = 1,
    Failed     = 2,
    Restarting = 3, // restart_at counter > 0
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum RestartPolicy {
    Never     = 0,
    OnFailure = 1, // restart only on non-zero exit
    Always    = 2,
}

#[derive(Clone, Copy)]
struct ServiceEntry {
    program_id:     u32,
    name:           [u8; 16],
    pid:            u32, // 0 = not running
    state:          SvcState,
    policy:         RestartPolicy,
    auto_start:     bool,
    restart_count:  u16,
    restart_at:     u32, // countdown in loop iterations; 0 = ready now
    last_exit:      i32, // most recent exit status
}

impl ServiceEntry {
    const fn new(program_id: u32, name: &[u8; 16], policy: RestartPolicy, auto_start: bool) -> Self {
        ServiceEntry {
            program_id,
            name:          *name,
            pid:           0,
            state:         SvcState::Stopped,
            policy,
            auto_start,
            restart_count: 0,
            restart_at:    0,
            last_exit:     0,
        }
    }
}

// ── Hardcoded service table ───────────────────────────────────────────────
//
// The session binary (program_id 8 = rwm desktop environment) is the
// primary auto-start service. Shell is spawned on demand or by session.
//
// Extend here as more kingdom services are registered.

static mut TABLE: [Option<ServiceEntry>; MAX_SVCS] = {
    let mut t: [Option<ServiceEntry>; MAX_SVCS] = [None; MAX_SVCS];
    // session — compositor + WM — always restart, auto-start
    t[0] = Some(ServiceEntry::new(
        PROG_SESSION,
        b"session\0\0\0\0\0\0\0\0\0",
        RestartPolicy::Always,
        true,
    ));
    // shell — on-demand; do not auto-start (session spawns it when needed)
    t[1] = Some(ServiceEntry::new(
        PROG_SHELL,
        b"shell\0\0\0\0\0\0\0\0\0\0\0",
        RestartPolicy::OnFailure,
        false,
    ));
    // monitor — optional system monitor; restart on failure
    t[2] = Some(ServiceEntry::new(
        PROG_MONITOR,
        b"monitor\0\0\0\0\0\0\0\0\0",
        RestartPolicy::OnFailure,
        false,
    ));
    t
};

// ── Logging helpers ───────────────────────────────────────────────────────

fn log(msg: &[u8]) {
    let _ = sys_write(1, msg.as_ptr(), msg.len());
}

fn log_u32(n: u32) {
    let mut buf = [b'0'; 10];
    if n == 0 {
        log(b"0");
        return;
    }
    let mut v = n;
    let mut end = 10usize;
    while v > 0 {
        end -= 1;
        buf[end] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    log(&buf[end..]);
}

// ── Service table helpers ─────────────────────────────────────────────────

fn find_by_pid(pid: u32) -> Option<usize> {
    unsafe {
        for (i, slot) in TABLE.iter().enumerate() {
            if let Some(ref s) = slot {
                if s.pid == pid {
                    return Some(i);
                }
            }
        }
    }
    None
}

fn find_by_program_id(prog_id: u32) -> Option<usize> {
    unsafe {
        for (i, slot) in TABLE.iter().enumerate() {
            if let Some(ref s) = slot {
                if s.program_id == prog_id {
                    return Some(i);
                }
            }
        }
    }
    None
}

// ── Spawn one service ─────────────────────────────────────────────────────

fn spawn_service(idx: usize) {
    let entry = unsafe { TABLE[idx].as_mut().unwrap() };
    let pid = sys_spawn(entry.program_id);
    if pid > 0 {
        entry.pid   = pid as u32;
        entry.state = SvcState::Running;
        log(b"[COGMAN] spawned svc=");
        log(&entry.name[..name_len(&entry.name)]);
        log(b" pid=");
        log_u32(entry.pid);
        log(b"\r\n");
    } else {
        entry.state = SvcState::Failed;
        log(b"[COGMAN] spawn failed svc=");
        log(&entry.name[..name_len(&entry.name)]);
        log(b"\r\n");
    }
}

fn name_len(name: &[u8; 16]) -> usize {
    name.iter().position(|&b| b == 0).unwrap_or(16)
}

// ── Reap dead children ────────────────────────────────────────────────────

fn reap_dead() {
    loop {
        let mut status: i32 = 0;
        let reaped = sys_waitpid(u32::MAX, &mut status as *mut i32, WNOHANG);
        if reaped <= 0 {
            // SYSERR_AGAIN (-11) or SYSERR_INVAL (-1) = nothing to reap
            break;
        }
        let pid = reaped as u32;
        log(b"[COGMAN] reaped pid=");
        log_u32(pid);
        log(b" status=");
        log_u32(status as u32);
        log(b"\r\n");

        if let Some(idx) = find_by_pid(pid) {
            let entry = unsafe { TABLE[idx].as_mut().unwrap() };
            entry.pid       = 0;
            entry.last_exit = status;

            let should_restart = match entry.policy {
                RestartPolicy::Never     => false,
                RestartPolicy::Always    => true,
                RestartPolicy::OnFailure => status != 0,
            };

            if should_restart {
                entry.state      = SvcState::Restarting;
                entry.restart_at = RESTART_DELAY_ITERS;
                log(b"[COGMAN] scheduling restart svc=");
                log(&entry.name[..name_len(&entry.name)]);
                log(b"\r\n");
            } else {
                entry.state = SvcState::Stopped;
            }
        }
    }
}

// ── Restart countdown ─────────────────────────────────────────────────────

fn tick_restarts() {
    unsafe {
        for slot in TABLE.iter_mut() {
            if let Some(ref mut entry) = slot {
                if entry.state == SvcState::Restarting {
                    if entry.restart_at > 0 {
                        entry.restart_at -= 1;
                    }
                }
            }
        }
    }
}

// ── Auto-start and restart-due services ──────────────────────────────────

fn start_pending() {
    for idx in 0..MAX_SVCS {
        let should = unsafe {
            TABLE[idx].as_ref().map_or(false, |s| {
                (s.state == SvcState::Stopped && s.auto_start) ||
                (s.state == SvcState::Restarting && s.restart_at == 0)
            })
        };
        if should {
            // increment restart counter if this is a restart (not first start)
            unsafe {
                if let Some(ref mut entry) = TABLE[idx] {
                    if entry.restart_count < u16::MAX {
                        entry.restart_count += 1;
                    }
                }
            }
            spawn_service(idx);
        }
    }
}

// ── IPC control channel ───────────────────────────────────────────────────

fn build_cog_resp(program_id: u32, idx: Option<usize>) -> KwmMsg {
    let mut msg = KwmMsg::ZERO;
    msg.msg_type = KwmType::CogResp as u8;
    let ctrl = unsafe { &mut msg.payload.cog_ctrl };
    ctrl.program_id = program_id;
    if let Some(i) = idx {
        if let Some(ref entry) = unsafe { &TABLE[i] } {
            ctrl.state         = entry.state as u8;
            ctrl.restart_count = entry.restart_count;
            ctrl.pid           = entry.pid;
            ctrl.name          = entry.name;
        }
    }
    msg
}

fn handle_ipc() {
    let mut msg = KwmMsg::ZERO;
    loop {
        let r = sys_ipc_recv(&mut msg, IPC_NONBLOCK);
        if r < 0 {
            // SYSERR_AGAIN = empty queue
            break;
        }
        let sender = msg.sender_pid;
        let prog_id = unsafe { msg.payload.cog_ctrl.program_id };

        match msg.msg_type {
            t if t == KwmType::CogList as u8 => {
                // Send one CogResp per registered service
                for idx in 0..MAX_SVCS {
                    if let Some(ref entry) = unsafe { &TABLE[idx] } {
                        let resp = build_cog_resp(entry.program_id, Some(idx));
                        let _ = sys_ipc_send(sender, &resp, 0);
                    }
                }
            }
            t if t == KwmType::CogStatus as u8 => {
                let idx = find_by_program_id(prog_id);
                let resp = build_cog_resp(prog_id, idx);
                let _ = sys_ipc_send(sender, &resp, 0);
            }
            t if t == KwmType::CogStop as u8 => {
                if let Some(idx) = find_by_program_id(prog_id) {
                    let entry = unsafe { TABLE[idx].as_mut().unwrap() };
                    // Mark stopped before sending shutdown so the reap path won't restart
                    entry.policy = RestartPolicy::Never;
                    entry.state  = SvcState::Stopped;
                    // Send the shutdown program as a proxy (best-effort)
                    let _ = sys_spawn(PROG_SHUTDOWN);
                    log(b"[COGMAN] stop requested for prog=");
                    log_u32(prog_id);
                    log(b"\r\n");
                }
                let idx = find_by_program_id(prog_id);
                let resp = build_cog_resp(prog_id, idx);
                let _ = sys_ipc_send(sender, &resp, 0);
            }
            t if t == KwmType::CogStart as u8 => {
                if let Some(idx) = find_by_program_id(prog_id) {
                    let entry = unsafe { TABLE[idx].as_mut().unwrap() };
                    if entry.state == SvcState::Stopped || entry.state == SvcState::Failed {
                        entry.state  = SvcState::Stopped; // ensure start_pending picks it up
                        entry.auto_start = true;
                    }
                }
                let idx = find_by_program_id(prog_id);
                let resp = build_cog_resp(prog_id, idx);
                let _ = sys_ipc_send(sender, &resp, 0);
            }
            t if t == KwmType::CogRestart as u8 => {
                if let Some(idx) = find_by_program_id(prog_id) {
                    let entry = unsafe { TABLE[idx].as_mut().unwrap() };
                    // Force a restart at next tick regardless of current state
                    entry.state      = SvcState::Restarting;
                    entry.restart_at = 0;
                    log(b"[COGMAN] restart requested for prog=");
                    log_u32(prog_id);
                    log(b"\r\n");
                }
                let idx = find_by_program_id(prog_id);
                let resp = build_cog_resp(prog_id, idx);
                let _ = sys_ipc_send(sender, &resp, 0);
            }
            t if t == KwmType::Ping as u8 => {
                let mut pong = KwmMsg::ZERO;
                pong.msg_type = KwmType::Ack as u8;
                pong.seq      = msg.seq;
                let _ = sys_ipc_send(sender, &pong, 0);
            }
            _ => {} // ignore unknown message types
        }
    }
}

// ── Spin delay ────────────────────────────────────────────────────────────

fn spin(ticks: u32) {
    let mut ev = libs::KeyEvent { keycode: 0, pressed: false };
    for _ in 0..ticks {
        let _ = sys_poll_input(&mut ev);
    }
}

// ── Entry point ───────────────────────────────────────────────────────────

#[no_mangle]
fn _start() -> ! {
    log(b"[COGMAN] v2 init start\r\n");

    // Mark all auto-start services as "stopped but ready" so start_pending picks them up.
    // (They are already Stopped by default; auto_start=true is what matters.)
    log(b"[COGMAN] starting auto-start services\r\n");
    start_pending();

    log(b"[COGMAN] supervisor loop running\r\n");

    loop {
        // 1. Reap any dead children
        reap_dead();

        // 2. Tick restart counters
        tick_restarts();

        // 3. Spawn anything that is due
        start_pending();

        // 4. Drain IPC control queue
        handle_ipc();

        // 5. Small spin to avoid busy-looping at full CPU
        spin(POLL_TICKS);
    }
}
