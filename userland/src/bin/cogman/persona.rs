//! Cogman's British persona — all human-facing speech lives here.
//!
//! Every string Cogman utters passes through `speak()`. Keeps the rest of
//! the codebase free of raw byte literals and makes the persona easy to
//! adjust without hunting through supervisor logic.

use userland::sys_write;

// ── Low-level output ──────────────────────────────────────────────────────

pub fn speak(msg: &[u8]) {
    let _ = sys_write(1, msg.as_ptr(), msg.len());
}

pub fn speak_u32(n: u32) {
    if n == 0 {
        let _ = sys_write(1, b"0".as_ptr(), 1);
        return;
    }
    let mut buf = [b'0'; 10];
    let mut i = 10usize;
    let mut v = n;
    while v > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    let _ = sys_write(1, buf[i..].as_ptr(), 10 - i);
}

// ── Boot greeting ─────────────────────────────────────────────────────────

pub fn greet() {
    speak(b"\r\n");
    speak(b"  ____                  ___  ____\r\n");
    speak(b" |  _ \\ ___   __ _ _  _|__ \\/ ___|\r\n");
    speak(b" | |_) / _ \\ / _` | | | |/ /\\___ \\\r\n");
    speak(b" |  _ < (_) | (_| | |_| / /_ ___) |\r\n");
    speak(b" |_| \\_\\___/ \\__, |\\__,_/____|____/\r\n");
    speak(b"              |___/\r\n");
    speak(b"\r\n");
    speak(b"  Cogman  -  Personal AI Assistant & System Butler\r\n");
    speak(b"  RogueOS  -  Built for developers, pentesters, and power users\r\n");
    speak(b"\r\n");
    speak(b"[Cogman] Good day. I'm Cogman, your system butler and personal adviser.\r\n");
    speak(b"[Cogman] I shall manage your processes and remain at your disposal.\r\n");
    speak(b"[Cogman] Privacy-first: your data stays local unless you decide otherwise.\r\n");
    speak(b"\r\n");
}

// ── Supervisor events ─────────────────────────────────────────────────────

pub fn say_spawning(name: &[u8], pid: u32) {
    speak(b"[Cogman] Bringing up ");
    speak(name);
    speak(b"  (pid=");
    speak_u32(pid);
    speak(b")\r\n");
}

pub fn say_spawn_failed(name: &[u8]) {
    speak(b"[Cogman] I'm afraid I couldn't start ");
    speak(name);
    speak(b". I shall try again presently.\r\n");
}

pub fn say_exited(name: &[u8], status: i32) {
    speak(b"[Cogman] ");
    speak(name);
    speak(b" has stepped out (exit=");
    speak_u32(status as u32);
    speak(b")\r\n");
}

pub fn say_restarting(name: &[u8], count: u16) {
    speak(b"[Cogman] Restarting ");
    speak(name);
    speak(b". This is restart number ");
    speak_u32(count as u32);
    speak(b".\r\n");
}

pub fn say_stopped_policy(name: &[u8]) {
    speak(b"[Cogman] ");
    speak(name);
    speak(b" has exited cleanly and I shan't restart it per policy.\r\n");
}

pub fn say_halt() {
    speak(b"\r\n[Cogman] Halt requested. Powering down now. Goodbye.\r\n\r\n");
}

pub fn say_reboot() {
    speak(b"\r\n[Cogman] Reboot requested. Back in a moment.\r\n\r\n");
}

pub fn say_waitpid_error() {
    speak(b"[Cogman] Beg pardon — waitpid returned an error. Carrying on.\r\n");
}

// ── Advisor events ────────────────────────────────────────────────────────

pub fn say_advisor_ready(mode: &[u8]) {
    speak(b"[Cogman] Adviser online (");
    speak(mode);
    speak(b"). Ask away whenever you're ready.\r\n");
}

pub fn say_advisor_local_only() {
    speak(b"[Cogman] Running in local-only mode. Your queries never leave this machine.\r\n");
}

pub fn say_advisor_cloud_available() {
    speak(b"[Cogman] Cloud inference is available. I'll ask locally first, \
             cloud as a fallback — with your permission.\r\n");
}

pub fn say_advisor_privacy_strip() {
    speak(b"[Cogman] I've scrubbed any sensitive tokens before that query left the box.\r\n");
}

pub fn say_no_network() {
    speak(b"[Cogman] No network stack yet, I'm afraid. Adviser running in local-model mode only.\r\n");
}

// ── IPC / control ─────────────────────────────────────────────────────────

pub fn say_ipc_unknown(ty: u8) {
    speak(b"[Cogman] Received an unfamiliar IPC message (type=0x");
    // hex nibbles
    let hi = ty >> 4;
    let lo = ty & 0x0f;
    let nibble = |n: u8| if n < 10 { b'0' + n } else { b'a' + n - 10 };
    let _ = sys_write(1, &[nibble(hi), nibble(lo)], 2);
    speak(b"). Ignoring it.\r\n");
}
