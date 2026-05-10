//! terminal — Stage 10: RDP terminal emulator.
//!
//! Registers with the WM via the Rogue Display Protocol, renders a scrollable
//! 64x24 character grid into a kernel surface, and processes key events
//! forwarded by the WM. Implements a built-in command line.
//!
//! Protocol flow:
//!   1. SYS_SURFACE_CREATE -> surface_id
//!   2. Send RDP_CONNECT (with surface_id) to the compositor PID
//!   3. Wait for RDP_GRANT -> learn content dimensions
//!   4. Render text into PIXEL_BUF, sys_surface_attach, send RDP_COMMIT
//!   5. Wait for RDP_PRESENT_DONE; handle RDP_KEY for input
//!   6. On RDP_CLOSE -> clean up and exit

#![no_std]
#![no_main]

extern crate alloc;

use libs::{
    IPC_NONBLOCK, KeyEvent, RwmMsg,
    keycodes::*,
};
use userland::{
    sys_exit, sys_get_compositor_pid, sys_ipc_recv, sys_ipc_send,
    sys_poll_input, sys_surface_attach, sys_surface_create, sys_surface_destroy,
    sys_write,
};

// ── RDP message type bytes ────────────────────────────────────────────────────
const RDP_CONNECT:      u8 = 0x50;
const RDP_GRANT:        u8 = 0x51;
const RDP_COMMIT:       u8 = 0x52;
const RDP_KEY:          u8 = 0x54;
const RDP_CLOSE:        u8 = 0x56;
const RDP_PRESENT_DONE: u8 = 0x58;

// ── Terminal grid dimensions ──────────────────────────────────────────────────
const TERM_COLS: usize = 64;
const TERM_ROWS: usize = 24;

// ── Pixel font (4×6, identical to wm.rs) ─────────────────────────────────────
const FONT_W: u32 = 4;
const FONT_H: u32 = 6;
const CHAR_W: u32 = FONT_W + 1; // 5px per column (4 + 1 kerning)
const CHAR_H: u32 = FONT_H + 2; // 8px per row   (6 + 2 leading)
const PAD:    u32 = 4;          // padding around the grid

const PX_W: usize = (TERM_COLS as u32 * CHAR_W + PAD * 2) as usize;
const PX_H: usize = (TERM_ROWS as u32 * CHAR_H + PAD * 2) as usize;
const STRIDE: u32 = PX_W as u32 * 4;

// Colors (BGRA / 0xAARRGGBB).
const C_BG:     u32 = 0xFF_1A_1B_26; // Tokyo Night background
const C_FG:     u32 = 0xFF_C0_CA_F5; // foreground text
const C_CURSOR: u32 = 0xFF_7A_A2_F7; // cursor bar

// ── Static pixel and character buffers ───────────────────────────────────────
// Placed in BSS (zero-initialised) so they do not inflate the ELF.
static mut PIXEL_BUF: [u32; PX_W * PX_H] = [0; PX_W * PX_H];
static mut CHAR_BUF:  [[u8; TERM_COLS]; TERM_ROWS] = [[b' '; TERM_COLS]; TERM_ROWS];

// ── Input line buffer ─────────────────────────────────────────────────────────
const MAX_LINE: usize = TERM_COLS - 2;
static mut LINE_BUF: [u8; MAX_LINE] = [0; MAX_LINE];
static mut LINE_LEN: usize = 0;

// ── Scroll state ──────────────────────────────────────────────────────────────
/// Row in CHAR_BUF where the next output line will be written.
static mut CUR_ROW: usize = 0;

// ── Font data (same encoding as wm.rs) ───────────────────────────────────────
#[rustfmt::skip]
const FONT: [u32; 95] = [
    0x000000, 0x020222, 0x00000A, 0x0AFAFA, 0x07861E, 0x094B26, 0x0D6664, 0x000002,
    0x042112, 0x021224, 0x000A5A, 0x002720, 0x012000, 0x000600, 0x020000, 0x001248,
    0b_0110_1001_1011_1101_1001_0110,
    0b_0100_1100_0100_0100_0100_1110,
    0b_0110_1001_0001_0010_0100_1111,
    0b_1110_0001_0110_0001_0001_1110,
    0b_0010_0110_1010_1111_0010_0010,
    0b_1111_1000_1110_0001_0001_1110,
    0b_0110_1000_1110_1001_1001_0110,
    0b_1111_0001_0010_0010_0100_0100,
    0b_0110_1001_0110_1001_1001_0110,
    0b_0110_1001_0111_0001_0001_0110,
    0x006060, 0x012060, 0x042124, 0x00F0F0, 0x042412, 0x020210,
    0b_0110_1001_1011_1011_1000_0110,
    0x099F96, 0x079797, 0x0E111E, 0x079997, 0x0F171F, 0x01171F, 0x0E9D1E,
    0x099F99, 0x072227, 0x06544E, 0x095359, 0x0F1111, 0x0999F9, 0x099DB9, 0x069996,
    0x011797, 0x0ED996, 0x095797, 0x07861E, 0x02222F, 0x069999, 0x066999, 0x06F999,
    0x096669, 0x022269, 0x0F124F,
    0b_0110_0100_0100_0100_0100_0110,
    0b_1000_1000_0100_0010_0001_0001,
    0b_0110_0010_0010_0010_0010_0110,
    0x000096, 0x0F0000,
    0x000004,
    0x099F96, 0x079797, 0x0E111E, 0x079997, 0x0F171F, 0x01171F, 0x0E9D1E,
    0x099F99, 0x072227, 0x06544E, 0x095359, 0x0F1111, 0x0999F9, 0x099DB9, 0x069996,
    0x011797, 0x0ED996, 0x095797, 0x07861E, 0x02222F, 0x069999, 0x066999, 0x06F999,
    0x096669, 0x022269, 0x0F124F,
    0x026226,
    0b_0100_0100_0100_0100_0100_0100,
    0x064426, 0x00050A,
];

fn glyph_bits(ch: u8) -> u32 {
    if ch < 0x20 || ch > 0x7E { return 0; }
    FONT[(ch - 0x20) as usize]
}

// ── Pixel-buffer drawing ──────────────────────────────────────────────────────

fn fill_rect_px(x: u32, y: u32, w: u32, h: u32, color: u32) {
    if w == 0 || h == 0 { return; }
    unsafe {
        let buf = PIXEL_BUF.as_mut_ptr();
        let stride_px = PX_W;
        for row in 0..h as usize {
            let row_base = (y as usize + row) * stride_px + x as usize;
            for col in 0..w as usize {
                core::ptr::write_volatile(buf.add(row_base + col), color);
            }
        }
    }
}

fn draw_char_px(px: u32, py: u32, ch: u8, color: u32) {
    let bits = glyph_bits(ch);
    if bits == 0 { return; }
    for row in 0..FONT_H {
        let row_bits = (bits >> (row * FONT_W)) & 0xF;
        if row_bits == 0 { continue; }
        let mut col = 0u32;
        while col < FONT_W {
            if (row_bits >> col) & 1 == 1 {
                let start = col;
                while col < FONT_W && (row_bits >> col) & 1 == 1 { col += 1; }
                fill_rect_px(px + start, py + row, col - start, 1, color);
            } else {
                col += 1;
            }
        }
    }
}

// ── Render full terminal ──────────────────────────────────────────────────────

fn render(cursor_col: usize) {
    // Background.
    fill_rect_px(0, 0, PX_W as u32, PX_H as u32, C_BG);

    unsafe {
        for row in 0..TERM_ROWS {
            for col in 0..TERM_COLS {
                let ch = CHAR_BUF[row][col];
                let px = PAD + col as u32 * CHAR_W;
                let py = PAD + row as u32 * CHAR_H;
                draw_char_px(px, py, ch, C_FG);
            }
        }
        // Cursor: filled bar in last row at cursor position.
        let last = TERM_ROWS - 1;
        let cx = PAD + cursor_col as u32 * CHAR_W;
        let cy = PAD + last as u32 * CHAR_H + FONT_H + 1;
        fill_rect_px(cx, cy, CHAR_W, 1, C_CURSOR);
    }
}

// ── Terminal output helpers ───────────────────────────────────────────────────

fn scroll_up() {
    unsafe {
        for r in 0..TERM_ROWS - 1 {
            CHAR_BUF[r] = CHAR_BUF[r + 1];
        }
        CHAR_BUF[TERM_ROWS - 1] = [b' '; TERM_COLS];
    }
}

fn print_line(s: &[u8]) {
    unsafe {
        if CUR_ROW >= TERM_ROWS - 1 {
            scroll_up();
            CUR_ROW = TERM_ROWS - 2;
        }
        let row = &mut CHAR_BUF[CUR_ROW];
        let n = s.len().min(TERM_COLS);
        for (i, &b) in s[..n].iter().enumerate() {
            row[i] = b;
        }
        // Clear rest of row.
        for i in n..TERM_COLS {
            row[i] = b' ';
        }
        CUR_ROW += 1;
        // Clear the input row.
        CHAR_BUF[TERM_ROWS - 1] = [b' '; TERM_COLS];
    }
}

fn show_prompt() {
    unsafe {
        let row = &mut CHAR_BUF[TERM_ROWS - 1];
        row[0] = b'>';
        row[1] = b' ';
        let n = LINE_LEN.min(TERM_COLS - 2);
        for i in 0..n {
            row[2 + i] = LINE_BUF[i];
        }
        for i in (2 + n)..TERM_COLS {
            row[i] = b' ';
        }
    }
}

// ── Built-in commands ─────────────────────────────────────────────────────────

fn run_command(cmd: &[u8]) {
    match cmd {
        b"help" | b"?" => {
            print_line(b"RogueOS Terminal v1.0");
            print_line(b"  help     show this message");
            print_line(b"  clear    clear screen");
            print_line(b"  ps       list processes");
            print_line(b"  ls       list root fs");
            print_line(b"  exit     close terminal");
        }
        b"clear" => {
            unsafe {
                for r in 0..TERM_ROWS - 1 {
                    CHAR_BUF[r] = [b' '; TERM_COLS];
                }
                CUR_ROW = 0;
            }
        }
        b"ls" => {
            let mut buf = [0u8; 512];
            let n = userland::sys_list_root(buf.as_mut_ptr(), buf.len());
            if n > 0 {
                // Split on newlines and print each entry.
                let mut start = 0usize;
                for i in 0..n as usize {
                    if buf[i] == b'\n' || buf[i] == b'\r' {
                        if i > start {
                            print_line(&buf[start..i]);
                        }
                        start = i + 1;
                    }
                }
                if (n as usize) > start {
                    print_line(&buf[start..n as usize]);
                }
            } else {
                print_line(b"(empty or error)");
            }
        }
        b"ps" => {
            let mut procs = [libs::ProcInfo { pid: 0, state: 0 }; 16];
            let n = userland::sys_get_proc_info(procs.as_mut_ptr(), 16);
            if n > 0 {
                print_line(b"PID  STATE");
                for i in 0..n as usize {
                    let p = &procs[i];
                    let mut line = [b' '; TERM_COLS];
                    // Write PID decimal.
                    let mut tmp = [0u8; 5];
                    let mut ti = 4usize;
                    let mut v = p.pid;
                    loop {
                        tmp[ti] = b'0' + (v % 10) as u8;
                        v /= 10;
                        if ti == 0 || v == 0 { break; }
                        ti -= 1;
                    }
                    let ndig = 5 - ti;
                    for k in 0..ndig { line[k] = tmp[ti + k]; }
                    line[5] = b' ';
                    let state_str: &[u8] = match p.state {
                        0 => b"empty",
                        1 => b"runnable",
                        2 => b"running",
                        3 => b"blocked",
                        4 => b"dead",
                        _ => b"?",
                    };
                    for (k, &b) in state_str.iter().enumerate() { line[6 + k] = b; }
                    print_line(&line[..6 + state_str.len()]);
                }
            } else {
                print_line(b"(no process info)");
            }
        }
        b"exit" => {
            print_line(b"Goodbye.");
            sys_exit(0);
        }
        b"" => {}
        _ => {
            // "unknown: <cmd>"
            let mut line = [b' '; TERM_COLS];
            let pfx = b"unknown: ";
            for (i, &b) in pfx.iter().enumerate() { line[i] = b; }
            let n = cmd.len().min(TERM_COLS - pfx.len());
            for (i, &b) in cmd[..n].iter().enumerate() { line[pfx.len() + i] = b; }
            print_line(&line[..pfx.len() + n]);
        }
    }
}

// ── IPC helpers ───────────────────────────────────────────────────────────────

fn send_rdp(wm_pid: u32, msg_type: u8, surface_id: u32, title: &[u8]) {
    let mut msg = RwmMsg::ZERO;
    msg.msg_type = msg_type;
    let kdp = unsafe { &mut msg.payload.rdp };
    kdp.surface_id = surface_id;
    let n = title.len().min(kdp.title.len().saturating_sub(1));
    kdp.title[..n].copy_from_slice(&title[..n]);
    let _ = sys_ipc_send(wm_pid, &msg, 0);
}

fn send_rdp_commit(wm_pid: u32, surface_id: u32, seq: u16) {
    let mut msg = RwmMsg::ZERO;
    msg.msg_type = RDP_COMMIT;
    msg.seq      = seq;
    let kdp = unsafe { &mut msg.payload.rdp };
    kdp.surface_id = surface_id;
    let _ = sys_ipc_send(wm_pid, &msg, 0);
}

fn log(s: &[u8]) {
    let _ = sys_write(1, s.as_ptr(), s.len());
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[no_mangle]
fn _start() -> ! {
    log(b"[terminal] starting\r\n");

    // 1. Create a kernel surface.
    let sid_raw = sys_surface_create();
    if sid_raw <= 0 {
        log(b"[terminal] surface_create failed\r\n");
        sys_exit(1);
    }
    let surface_id = sid_raw as u32;

    // 2. Get WM (compositor) PID.
    let wm_pid_raw = sys_get_compositor_pid();
    if wm_pid_raw <= 0 {
        // No compositor yet; wait a moment then retry once.
        for _ in 0..500_000u32 {
            unsafe { core::arch::asm!("pause", options(nomem, nostack)); }
        }
        let r2 = sys_get_compositor_pid();
        if r2 <= 0 {
            log(b"[terminal] no compositor found\r\n");
            let _ = sys_surface_destroy(surface_id);
            sys_exit(1);
        }
    }
    let wm_pid = if wm_pid_raw > 0 { wm_pid_raw as u32 } else {
        let r2 = sys_get_compositor_pid();
        if r2 <= 0 { let _ = sys_surface_destroy(surface_id); sys_exit(1); }
        r2 as u32
    };

    log(b"[terminal] connecting to WM\r\n");

    // 3. Connect to WM, advertising our surface.
    send_rdp(wm_pid, RDP_CONNECT, surface_id, b"terminal");

    // 4. Wait for RDP_GRANT (up to ~2M spins).
    let mut ipc_msg = RwmMsg::ZERO;
    let mut got_grant = false;
    for _ in 0..2_000_000u32 {
        if sys_ipc_recv(&mut ipc_msg, IPC_NONBLOCK) == 0
            && ipc_msg.msg_type == RDP_GRANT
        {
            got_grant = true;
            break;
        }
    }
    if !got_grant {
        log(b"[terminal] no grant from WM\r\n");
        let _ = sys_surface_destroy(surface_id);
        sys_exit(1);
    }

    // 5. Attach the pixel buffer to the surface.
    let buf_ptr = unsafe { PIXEL_BUF.as_ptr() as *const u8 };
    let r = sys_surface_attach(surface_id, buf_ptr, PX_W as u32, PX_H as u32, STRIDE);
    if r < 0 {
        log(b"[terminal] surface_attach failed\r\n");
        let _ = sys_surface_destroy(surface_id);
        sys_exit(1);
    }

    // 6. Initial render + commit.
    print_line(b"RogueOS Terminal - type 'help' for commands");
    show_prompt();
    unsafe { render(LINE_LEN + 2); }
    let mut frame_seq: u16 = 0;
    send_rdp_commit(wm_pid, surface_id, frame_seq);

    // ── Main event loop ──────────────────────────────────────────────────────
    let mut ev_key = KeyEvent { keycode: 0, pressed: false };
    let mut pending_commit = true;
    let mut dirty = false;

    loop {
        // Poll for WM IPC messages (non-blocking).
        while sys_ipc_recv(&mut ipc_msg, IPC_NONBLOCK) == 0 {
            match ipc_msg.msg_type {
                RDP_PRESENT_DONE => {
                    pending_commit = false;
                    if dirty {
                        dirty = false;
                        frame_seq = frame_seq.wrapping_add(1);
                        // Re-attach with updated pixel data.
                        let _ = sys_surface_attach(
                            surface_id, buf_ptr,
                            PX_W as u32, PX_H as u32, STRIDE,
                        );
                        send_rdp_commit(wm_pid, surface_id, frame_seq);
                        pending_commit = true;
                    }
                }
                RDP_KEY => {
                    let kdp = unsafe { ipc_msg.payload.rdp };
                    if kdp.key_state == 1 {
                        handle_key(kdp.key_code as u8, wm_pid, surface_id, &mut dirty);
                    }
                }
                RDP_CLOSE => {
                    let _ = sys_surface_destroy(surface_id);
                    sys_exit(0);
                }
                _ => {}
            }
        }

        // Also accept raw keyboard events (when terminal has focus without WM forwarding).
        let n = sys_poll_input(&mut ev_key);
        if n > 0 && ev_key.pressed {
            handle_key(ev_key.keycode, wm_pid, surface_id, &mut dirty);
        }

        // If we have unsent changes and no frame in flight, commit now.
        if dirty && !pending_commit {
            dirty = false;
            frame_seq = frame_seq.wrapping_add(1);
            let _ = sys_surface_attach(
                surface_id, buf_ptr,
                PX_W as u32, PX_H as u32, STRIDE,
            );
            send_rdp_commit(wm_pid, surface_id, frame_seq);
            pending_commit = true;
        }
    }
}

fn handle_key(key: u8, wm_pid: u32, surface_id: u32, dirty: &mut bool) {
    unsafe {
        match key {
            KEY_ENTER => {
                // Execute command.
                let cmd = &LINE_BUF[..LINE_LEN];
                // Copy to stack before running (run_command may mutate CHAR_BUF).
                let mut cmd_copy = [0u8; MAX_LINE];
                cmd_copy[..LINE_LEN].copy_from_slice(cmd);
                let cmd_len = LINE_LEN;
                LINE_LEN = 0;
                // Show what was typed.
                let mut echo = [b' '; TERM_COLS];
                echo[0] = b'>';
                echo[1] = b' ';
                echo[2..2 + cmd_len].copy_from_slice(&cmd_copy[..cmd_len]);
                print_line(&echo[..2 + cmd_len]);
                run_command(&cmd_copy[..cmd_len]);
                show_prompt();
                render(2);
                *dirty = true;
            }
            KEY_BACKSPACE => {
                if LINE_LEN > 0 {
                    LINE_LEN -= 1;
                    show_prompt();
                    render(LINE_LEN + 2);
                    *dirty = true;
                }
            }
            KEY_ESC => {
                // Disconnect and exit.
                let mut msg = RwmMsg::ZERO;
                msg.msg_type = 0x57; // RDP_DISCONNECT
                let kdp = &mut msg.payload.rdp;
                kdp.surface_id = surface_id;
                let _ = sys_ipc_send(wm_pid, &msg, 0);
                let _ = sys_surface_destroy(surface_id);
                sys_exit(0);
            }
            _ => {
                // Printable ASCII.
                if let Some(ch) = keycode_to_char(key) {
                    if LINE_LEN < MAX_LINE {
                        LINE_BUF[LINE_LEN] = ch;
                        LINE_LEN += 1;
                        show_prompt();
                        render(LINE_LEN + 2);
                        *dirty = true;
                    }
                }
            }
        }
    }
}

/// Convert a RogueOS keycode to a printable ASCII character, or None.
fn keycode_to_char(k: u8) -> Option<u8> {
    match k {
        KEY_SPACE  => Some(b' '),
        KEY_MINUS  => Some(b'-'),
        KEY_EQUAL  => Some(b'='),
        KEY_COMMA  => Some(b','),
        KEY_PERIOD => Some(b'.'),
        KEY_SLASH  => Some(b'/'),
        KEY_SEMI   => Some(b';'),
        KEY_QUOTE  => Some(b'\''),
        KEY_LBRACE => Some(b'['),
        KEY_RBRACE => Some(b']'),
        KEY_BSLASH => Some(b'\\'),
        KEY_GRAVE  => Some(b'`'),
        KEY_0      => Some(b'0'),
        KEY_1      => Some(b'1'),
        KEY_2      => Some(b'2'),
        KEY_3      => Some(b'3'),
        KEY_4      => Some(b'4'),
        KEY_5      => Some(b'5'),
        KEY_6      => Some(b'6'),
        KEY_7      => Some(b'7'),
        KEY_8      => Some(b'8'),
        KEY_9      => Some(b'9'),
        KEY_A      => Some(b'a'),
        KEY_B      => Some(b'b'),
        KEY_C      => Some(b'c'),
        KEY_D      => Some(b'd'),
        KEY_E      => Some(b'e'),
        KEY_F      => Some(b'f'),
        KEY_G      => Some(b'g'),
        KEY_H      => Some(b'h'),
        KEY_I      => Some(b'i'),
        KEY_J      => Some(b'j'),
        KEY_K      => Some(b'k'),
        KEY_L      => Some(b'l'),
        KEY_M      => Some(b'm'),
        KEY_N      => Some(b'n'),
        KEY_O      => Some(b'o'),
        KEY_P      => Some(b'p'),
        KEY_Q      => Some(b'q'),
        KEY_R      => Some(b'r'),
        KEY_S      => Some(b's'),
        KEY_T      => Some(b't'),
        KEY_U      => Some(b'u'),
        KEY_V      => Some(b'v'),
        KEY_W      => Some(b'w'),
        KEY_X      => Some(b'x'),
        KEY_Y      => Some(b'y'),
        KEY_Z      => Some(b'z'),
        _          => None,
    }
}
