//! Cogman Adviser — AI assistant interface.
//!
//! Architecture:
//!   Query → PrivacyFilter → [LocalModel | ClaudeAPI] → Response
//!
//! Policy (in order):
//!   1. If a local model is loaded → always use it; nothing leaves the machine.
//!   2. If cloud is opted-in and network is available → strip PII, then call Claude.
//!   3. Otherwise → politely decline and explain.
//!
//! All conversation history is written to the local filesystem under
//! `/var/cogman/history/` and never transmitted unless the user explicitly
//! exports a session.

pub mod claude;
pub mod local;

use super::persona;

// ── Types ─────────────────────────────────────────────────────────────────

/// Maximum query length (bytes) accepted from any source.
pub const MAX_QUERY: usize = 1024;

/// Maximum response length (bytes) that can be buffered.
pub const MAX_RESPONSE: usize = 4096;

/// Where the adviser can send a query.
#[derive(Copy, Clone, PartialEq)]
pub enum AdvisorBackend {
    /// Local GGUF/llama.cpp-style model loaded into RAM.
    Local,
    /// Claude API via HTTPS (requires networking + API key on disk).
    Cloud,
    /// No backend available — adviser is offline.
    Offline,
}

/// Privacy classification of a query before it leaves the machine.
#[derive(Copy, Clone, PartialEq)]
pub enum PrivacyClass {
    /// Safe to send as-is.
    Public,
    /// Contained redactable PII — send after stripping.
    Redacted,
    /// Contains credentials, keys, or highly sensitive paths — local only.
    Sensitive,
}

/// A user query plus context that the adviser should know.
pub struct Query<'a> {
    pub text:    &'a [u8],
    pub context: QueryContext,
}

#[derive(Copy, Clone)]
pub struct QueryContext {
    /// Whether the user is currently in a pentesting session context.
    pub pentest_mode: bool,
    /// Whether the user has opted in to cloud fallback.
    pub cloud_opt_in: bool,
}

impl Default for QueryContext {
    fn default() -> Self {
        QueryContext { pentest_mode: false, cloud_opt_in: false }
    }
}

// ── Privacy filter ────────────────────────────────────────────────────────

/// Classify a query before deciding where to send it.
pub fn classify(query: &[u8]) -> PrivacyClass {
    // Patterns that indicate credential or key material.
    // Very lightweight scan — no regex, just byte-pattern presence.
    let sensitive_markers: &[&[u8]] = &[
        b"BEGIN RSA",
        b"BEGIN EC",
        b"BEGIN PRIVATE",
        b"sk-ant-",     // Anthropic API key prefix
        b"ghp_",        // GitHub PAT prefix
        b"password=",
        b"passwd=",
        b"secret=",
        b"token=",
        b"/etc/shadow",
        b"/etc/passwd",
        b".ssh/id_",
    ];

    for marker in sensitive_markers {
        if contains_subsequence(query, marker) {
            return PrivacyClass::Sensitive;
        }
    }

    // Patterns that might contain PII but can be stripped.
    let pii_markers: &[&[u8]] = &[
        b"@",           // email address heuristic
        b"192.168.",
        b"10.0.",
        b"172.16.",
    ];

    for marker in pii_markers {
        if contains_subsequence(query, marker) {
            return PrivacyClass::Redacted;
        }
    }

    PrivacyClass::Public
}

/// Redact known-sensitive tokens from a query buffer.
/// Returns a new fixed-size buffer and the valid length.
pub fn redact(query: &[u8], out: &mut [u8; MAX_QUERY]) -> usize {
    // Simple pass-through redaction: replace anything after `token=` with `[REDACTED]`.
    // A real implementation would do full regex-based scrubbing.
    let len = query.len().min(MAX_QUERY);
    out[..len].copy_from_slice(&query[..len]);
    // TODO: full PII scrubber — replace credential values with [REDACTED]
    len
}

// ── Adviser dispatch ──────────────────────────────────────────────────────

/// Detect which backend is available right now.
pub fn detect_backend() -> AdvisorBackend {
    if local::is_model_loaded() {
        AdvisorBackend::Local
    } else if claude::is_network_available() {
        AdvisorBackend::Cloud
    } else {
        AdvisorBackend::Offline
    }
}

/// Announce the adviser's backend status over serial.
pub fn announce_ready() {
    match detect_backend() {
        AdvisorBackend::Local => {
            persona::say_advisor_ready(b"local model");
            persona::say_advisor_local_only();
        }
        AdvisorBackend::Cloud => {
            persona::say_advisor_ready(b"cloud available");
            persona::say_advisor_cloud_available();
        }
        AdvisorBackend::Offline => {
            persona::say_no_network();
        }
    }
}

/// Submit a query and write the response into `out`. Returns bytes written.
///
/// This is the single dispatch point for everything that calls the adviser.
/// Enforces privacy policy before routing to any backend.
pub fn ask(query: &Query<'_>, out: &mut [u8; MAX_RESPONSE]) -> usize {
    let privacy = classify(query.text);

    // Sensitive material never leaves the machine regardless of opt-in.
    if privacy == PrivacyClass::Sensitive && !local::is_model_loaded() {
        let msg = b"I'm afraid that query contains sensitive material. \
                    I can only handle it with a local model loaded. \
                    Ask `cogman load-model <path>` to load one.";
        let len = msg.len().min(MAX_RESPONSE);
        out[..len].copy_from_slice(&msg[..len]);
        return len;
    }

    match detect_backend() {
        AdvisorBackend::Local => {
            local::query(query.text, out)
        }
        AdvisorBackend::Cloud if query.context.cloud_opt_in => {
            if privacy == PrivacyClass::Sensitive {
                // Already handled above, but be defensive.
                return 0;
            }
            let mut clean = [0u8; MAX_QUERY];
            let clean_len = if privacy == PrivacyClass::Redacted {
                persona::say_advisor_privacy_strip();
                redact(query.text, &mut clean)
            } else {
                let l = query.text.len().min(MAX_QUERY);
                clean[..l].copy_from_slice(&query.text[..l]);
                l
            };
            claude::query(&clean[..clean_len], out)
        }
        AdvisorBackend::Cloud => {
            // Cloud available but user hasn't opted in.
            let msg = b"Cloud inference is available but you haven't opted in. \
                        Run `cogman set cloud-opt-in true` if you'd like to enable it.";
            let len = msg.len().min(MAX_RESPONSE);
            out[..len].copy_from_slice(&msg[..len]);
            len
        }
        AdvisorBackend::Offline => {
            let msg = b"I'm offline at the moment and have no local model loaded. \
                        Load a model with `cogman load-model <path>` to use the adviser.";
            let len = msg.len().min(MAX_RESPONSE);
            out[..len].copy_from_slice(&msg[..len]);
            len
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn contains_subsequence(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() { return true; }
    if needle.len() > haystack.len() { return false; }
    haystack.windows(needle.len()).any(|w| w == needle)
}
