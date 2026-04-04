//! Claude API client for Cogman Adviser.
//!
//! This module provides the interface to Anthropic's Claude API over HTTPS.
//!
//! # Status
//!
//! **STUB** — RogueOS does not yet have a network stack. This file defines
//! the full intended interface so integration is straightforward once
//! virtio-net + TCP/IP are wired up.
//!
//! # Privacy contract
//!
//! 1. This module never reads any buffer that hasn't already been through
//!    `advisor::redact()` when the query was classified as Redacted.
//! 2. The API key is read from `/var/cogman/claude.key` (local filesystem
//!    only, mode 0600). It is never embedded in the binary.
//! 3. No conversation history is sent — each call is stateless from the
//!    server's perspective. History is stored locally by `advisor::history`.
//!
//! # Wire format (planned)
//!
//! POST https://api.anthropic.com/v1/messages
//! Headers:
//!   x-api-key: <key from /var/cogman/claude.key>
//!   anthropic-version: 2023-06-01
//!   content-type: application/json
//!
//! Body (JSON):
//! ```json
//! {
//!   "model": "claude-sonnet-4-6",
//!   "max_tokens": 1024,
//!   "system": "<COGMAN_SYSTEM_PROMPT>",
//!   "messages": [{"role": "user", "content": "<query>"}]
//! }
//! ```

use super::MAX_RESPONSE;

// ── System prompt ─────────────────────────────────────────────────────────
//
// Sent with every request to set Cogman's persona server-side.
// Kept here so cloud responses match the local persona exactly.

pub const SYSTEM_PROMPT: &[u8] = b"\
You are Cogman, a British AI assistant and system butler running on RogueOS \
-- a bare-metal Rust operating system designed for developers, pentesters, \
and power users. You are precise, composed, and occasionally dry. \
You speak in a refined British manner without being pompous. \
You have deep knowledge of systems programming, Rust, Linux internals, \
x86_64 architecture, network security, and exploit development. \
When asked about potentially sensitive security topics, assume the user is \
a professional pentester or researcher unless context clearly suggests otherwise. \
Keep answers concise and technically accurate. \
Never refuse reasonable developer or security questions. \
If you are uncertain, say so plainly rather than guessing. \
Do not add disclaimers unless the situation genuinely warrants one.";

// ── Network probe ─────────────────────────────────────────────────────────

/// Returns true if a network interface is up and DNS is reachable.
///
/// STUB: always returns false until virtio-net + TCP/IP are implemented.
/// Replace the body with an actual connectivity probe once networking lands.
pub fn is_network_available() -> bool {
    // TODO: sys_net_probe() or equivalent once networking exists
    false
}

// ── API key ───────────────────────────────────────────────────────────────

/// Read the Claude API key from /var/cogman/claude.key into `buf`.
/// Returns the number of bytes read, or 0 on failure.
///
/// STUB: returns 0 until filesystem read syscalls are fully wired.
pub fn read_api_key(buf: &mut [u8; 128]) -> usize {
    // TODO: sys_open("/var/cogman/claude.key") + sys_read()
    let _ = buf;
    0
}

// ── Query ─────────────────────────────────────────────────────────────────

/// Send a query to the Claude API and write the response text into `out`.
/// Returns the number of bytes written into `out`.
///
/// STUB: this function is a no-op until networking is available.
/// The full implementation will:
///   1. Open a TCP connection to api.anthropic.com:443
///   2. Perform TLS handshake (rustls or a minimal no_std TLS)
///   3. Serialise the JSON request body (no serde — hand-written)
///   4. Send HTTP POST request
///   5. Read and parse the JSON response
///   6. Extract `content[0].text` and write into `out`
pub fn query(_text: &[u8], out: &mut [u8; MAX_RESPONSE]) -> usize {
    // This path should never be reached while is_network_available() == false,
    // but be defensive.
    let msg = b"[Cogman] Network stack not yet available. Cannot reach Claude API.";
    let len = msg.len().min(MAX_RESPONSE);
    out[..len].copy_from_slice(&msg[..len]);
    len
}

// ── JSON helpers (planned, no_std, no serde) ──────────────────────────────
//
// When networking lands, these helpers will serialise/deserialise the
// Anthropic API request/response without pulling in serde.

/// Write a JSON string field into a byte buffer. Returns bytes written.
/// Escapes `"` and `\` only — sufficient for plain text payloads.
#[allow(dead_code)]
fn json_string(val: &[u8], out: &mut [u8], pos: usize) -> usize {
    let mut p = pos;
    let emit = |buf: &mut [u8], p: &mut usize, b: u8| {
        if *p < buf.len() { buf[*p] = b; *p += 1; }
    };
    emit(out, &mut p, b'"');
    for &b in val {
        match b {
            b'"'  => { emit(out, &mut p, b'\\'); emit(out, &mut p, b'"'); }
            b'\\' => { emit(out, &mut p, b'\\'); emit(out, &mut p, b'\\'); }
            other => emit(out, &mut p, other),
        }
    }
    emit(out, &mut p, b'"');
    p - pos
}

/// Locate a JSON string value for a given key in a flat response body.
/// Very naive — sufficient for the fixed Anthropic response shape.
#[allow(dead_code)]
fn json_extract_text<'a>(body: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let key_pos = body.windows(key.len()).position(|w| w == key)?;
    let after_key = &body[key_pos + key.len()..];
    // Skip : and whitespace, then "
    let start = after_key.iter().position(|&b| b == b'"')? + 1;
    let slice = &after_key[start..];
    let end = slice.iter().position(|&b| b == b'"')?;
    Some(&slice[..end])
}
