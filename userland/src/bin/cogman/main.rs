//! Cogman — RogueOS System Butler & Personal AI Adviser (PID 1).
//!
//! Cogman is the first userland process started by the kernel. It wears
//! two hats simultaneously:
//!
//!   1. **Init / supervisor** — spawns and guards every other process on the
//!      system. If session dies, it comes back. If the WM crashes, Cogman
//!      tidies up and restarts it. Halt and reboot flow through Cogman.
//!
//!   2. **British AI assistant** — responds to queries via IPC from any
//!      process. Speaks to the user in a composed, precise British manner.
//!      Routes queries to a local GGUF model (privacy-first) or to the
//!      Claude API (opt-in, PII-stripped) when network is available.
//!
//! Constraints: `#![no_std]`, `#![no_main]`, no heap. All state is in
//! statics or stack locals. The IPC loop is non-blocking; the reap loop
//! uses WNOHANG. Cogman never blocks indefinitely so it stays responsive.

#![no_std]
#![no_main]

mod advisor;
mod persona;
mod supervisor;

use userland::sys_ipc_recv;
use libs::{IPC_NONBLOCK, RwmMsg};
use supervisor::{default_supervisor, Supervisor};

// ── IPC message types for Cogman control ─────────────────────────────────
//
// Any process can IPC-send to Cogman (PID 1) using these types.
// Responses come back as RwmType::CogResp.

const COG_CTRL_STOP:   u8 = 0x41;
const COG_CTRL_STATUS: u8 = 0x42;
const COG_CTRL_LIST:   u8 = 0x43;
const COG_CTRL_ASK:    u8 = 0x45; // Ask the adviser a question
const COG_RESP:        u8 = 0x44;

// ── Poll delay ────────────────────────────────────────────────────────────

/// Spin iterations between IPC poll passes. Keeps CPU usage low while
/// keeping Cogman responsive. Tune based on scheduler tick rate.
const POLL_DELAY: u64 = 500_000;

fn pause_spin() {
    for _ in 0..POLL_DELAY {
        unsafe { core::arch::asm!("pause", options(nomem, nostack)); }
    }
}

// ── IPC handling ──────────────────────────────────────────────────────────

fn handle_ipc(sv: &mut Supervisor, msg: &RwmMsg) {
    let ty = msg.msg_type;
    match ty {
        COG_CTRL_LIST => {
            persona::speak(b"[Cogman] Service table:\r\n");
            for slot in sv.table.iter().flatten() {
                let state = match slot.state {
                    supervisor::SvcState::Running    => b"running   " as &[u8],
                    supervisor::SvcState::Stopped    => b"stopped   ",
                    supervisor::SvcState::Restarting => b"restarting",
                    supervisor::SvcState::Failed     => b"failed    ",
                };
                persona::speak(b"  [");
                persona::speak(state);
                persona::speak(b"] ");
                persona::speak(slot.name_bytes());
                if slot.pid > 0 {
                    persona::speak(b" pid=");
                    persona::speak_u32(slot.pid);
                }
                persona::speak(b"\r\n");
            }
        }

        COG_CTRL_STATUS => {
            persona::speak(b"[Cogman] Running. All services nominal.\r\n");
        }

        COG_CTRL_ASK => {
            // Extract the query text from the IPC payload.
            // Payload bytes 0..55 are the query string (NUL-terminated).
            let payload = unsafe { &msg.payload.raw };
            let query_len = payload.data.iter().position(|&b| b == 0).unwrap_or(55);
            let query_text = &payload.data[..query_len];

            let ctx = advisor::QueryContext {
                pentest_mode: false,
                cloud_opt_in: false, // user must explicitly opt in
            };
            let query = advisor::Query { text: query_text, context: ctx };
            let mut resp_buf = [0u8; advisor::MAX_RESPONSE];
            let resp_len = advisor::ask(&query, &mut resp_buf);

            persona::speak(b"[Cogman] ");
            if resp_len > 0 {
                let _ = userland::sys_write(1, resp_buf.as_ptr(), resp_len);
            }
            persona::speak(b"\r\n");
        }

        COG_CTRL_STOP => {
            // Payload[0..3] = target program_id (LE u32)
            let payload = unsafe { &msg.payload.raw };
            let prog_id = u32::from_le_bytes([
                payload.data[0], payload.data[1], payload.data[2], payload.data[3],
            ]);
            persona::speak(b"[Cogman] Stopping service with program_id=");
            persona::speak_u32(prog_id);
            persona::speak(b" (not yet implemented - coming presently).\r\n");
        }

        _ => {
            persona::say_ipc_unknown(ty);
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────

#[no_mangle]
fn _start() -> ! {
    // ── 1. Greet ──────────────────────────────────────────────────────────
    persona::greet();

    // ── 2. Announce adviser status ────────────────────────────────────────
    advisor::announce_ready();

    // ── 3. Build service table and start auto-start services ──────────────
    let mut sv = default_supervisor();
    sv.start_all();

    persona::speak(b"[Cogman] All systems go. I'm here if you need me.\r\n\r\n");

    // ── 4. Main event loop ────────────────────────────────────────────────
    loop {
        // Reap any dead children (non-blocking).
        sv.reap_pass();

        // Tick down restart delays and re-spawn if ready.
        sv.restart_pass();

        // Drain the IPC inbox (non-blocking).
        loop {
            let mut msg = RwmMsg::zeroed();
            let ret = sys_ipc_recv(&mut msg, IPC_NONBLOCK);
            if ret < 0 {
                break; // EAGAIN — inbox empty
            }
            handle_ipc(&mut sv, &msg);
        }

        // Brief pause before next poll cycle.
        pause_spin();
    }
}
