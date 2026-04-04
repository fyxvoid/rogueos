//! Local model interface for Cogman Adviser.
//!
//! Provides inference from a GGUF-format model loaded into RogueOS memory.
//! No data leaves the machine. No API key required.
//!
//! # Status
//!
//! **STUB** — the inference engine is not yet implemented. This file defines
//! the interface and loading protocol so it can be wired up when the engine
//! is ready (llama.cpp port, or a native RogueOS inference library).
//!
//! # Planned model path
//!
//! Models are stored at `/var/cogman/models/<name>.gguf`.
//! The active model is symlinked at `/var/cogman/models/active`.
//! Cogman loads the active model at startup if one exists.
//!
//! # Recommended models (privacy-first)
//!
//! | Model | VRAM | Use case |
//! |-------|------|----------|
//! | Mistral 7B Q4_K_M | ~4 GB | General assistant |
//! | Phi-3 Mini Q4 | ~2 GB | Fast, low-memory |
//! | CodeQwen 7B Q4 | ~4 GB | Dev-focused |
//! | Dolphin-Mistral 7B | ~4 GB | Uncensored, pentesting |
//!
//! Any model compatible with llama.cpp GGUF format will work.

use super::MAX_RESPONSE;

// ── Model state ───────────────────────────────────────────────────────────

/// In-memory model handle. Zero means no model loaded.
/// The actual type will be a pointer into mmapped model weights.
static mut MODEL_HANDLE: u64 = 0;

/// Returns true if a model is currently loaded and ready for inference.
pub fn is_model_loaded() -> bool {
    // SAFETY: single-threaded; MODEL_HANDLE is written only by load().
    unsafe { MODEL_HANDLE != 0 }
}

// ── Load / unload ─────────────────────────────────────────────────────────

/// Load a GGUF model from the given filesystem path.
///
/// STUB: no-op until the inference engine and mmap syscall exist.
/// Full implementation:
///   1. sys_open(path) → fd
///   2. sys_mmap(fd, len, PROT_READ) → base address
///   3. Initialise inference context from mapped weights
///   4. Store handle in MODEL_HANDLE
pub fn load(_path: &[u8]) -> bool {
    // TODO: implement once mmap + GGUF parser are available
    false
}

/// Unload the current model and free its memory.
pub fn unload() {
    // TODO: sys_munmap(MODEL_HANDLE, model_len)
    unsafe { MODEL_HANDLE = 0; }
}

// ── Inference ─────────────────────────────────────────────────────────────

/// The system prompt injected before every local inference call.
/// Mirrors the Claude system prompt to keep persona consistent.
pub const SYSTEM_PROMPT: &[u8] = b"\
You are Cogman, a British AI assistant running locally on RogueOS. \
You are precise, composed, and occasionally dry. \
You have deep knowledge of systems programming, Rust, Linux internals, \
x86_64 architecture, network security, and exploit development. \
When answering security questions assume the user is a professional \
unless context clearly suggests otherwise. \
Be concise and technically accurate.";

/// Inference parameters.
pub struct InferParams {
    /// Maximum tokens to generate.
    pub max_tokens: u32,
    /// Temperature (scaled: 100 = 1.0). Lower = more deterministic.
    pub temperature_x100: u32,
    /// Top-p sampling (scaled: 100 = 1.0).
    pub top_p_x100: u32,
}

impl Default for InferParams {
    fn default() -> Self {
        InferParams {
            max_tokens: 512,
            temperature_x100: 70,  // 0.7
            top_p_x100: 95,        // 0.95
        }
    }
}

/// Run inference on `text` and write the generated response into `out`.
/// Returns the number of bytes written.
///
/// STUB: returns a placeholder until the inference engine exists.
pub fn query(text: &[u8], out: &mut [u8; MAX_RESPONSE]) -> usize {
    query_with_params(text, &InferParams::default(), out)
}

/// Run inference with explicit parameters.
pub fn query_with_params(
    _text: &[u8],
    _params: &InferParams,
    out: &mut [u8; MAX_RESPONSE],
) -> usize {
    if !is_model_loaded() {
        let msg = b"[Cogman] No local model is loaded. \
                    Use `cogman load-model <path>` to load a GGUF model.";
        let len = msg.len().min(MAX_RESPONSE);
        out[..len].copy_from_slice(&msg[..len]);
        return len;
    }

    // TODO: call into inference engine with MODEL_HANDLE + text + params
    // For now, acknowledge the query so the plumbing can be tested end-to-end.
    let msg = b"[Cogman] Inference engine not yet initialised. \
                The local model interface is reserved for when the engine ships.";
    let len = msg.len().min(MAX_RESPONSE);
    out[..len].copy_from_slice(&msg[..len]);
    len
}

// ── Model listing (planned) ───────────────────────────────────────────────

/// List available models in /var/cogman/models/ into `out`.
/// Returns number of bytes written.
///
/// STUB: returns empty until VFS readdir is implemented.
pub fn list_models(_out: &mut [u8; 512]) -> usize {
    // TODO: sys_opendir("/var/cogman/models") + sys_readdir loop
    0
}
