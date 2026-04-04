# Cogman Adviser

The Adviser is the AI brain inside Cogman. It answers queries, routes them
to the right backend, and enforces the privacy policy before anything leaves
the machine.

---

## Architecture

```
User query (IPC CogCtrlAsk)
        │
        ▼
  PrivacyFilter
  classify(query)
        │
        ├─ Sensitive ──────────────────► Refuse (or local only)
        │
        ├─ Redacted ──► redact() ──────► Stripped query
        │                                      │
        └─ Public ─────────────────────────────┤
                                               ▼
                                       detect_backend()
                                               │
                              ┌────────────────┼────────────────┐
                              ▼                ▼                ▼
                         Local model     Claude API          Offline
                         (local.rs)     (claude.rs)         response
                              │                │
                              └────────────────┘
                                       │
                                       ▼
                              Response → serial output
                              History → /var/cogman/history/
```

---

## Files

| File | Purpose |
|------|---------|
| `mod.rs` | Dispatch logic, privacy filter, `ask()` entry point |
| `claude.rs` | Claude API HTTPS client (stub until networking) |
| `local.rs` | Local GGUF model inference (stub until engine) |

---

## Privacy filter

`classify(query)` scans the raw query bytes for sensitive patterns before
routing anywhere:

**Sensitive** (never transmitted, local model or refuse):
- RSA/EC private keys (`BEGIN RSA PRIVATE`, `BEGIN EC PRIVATE`)
- Anthropic API keys (`sk-ant-`)
- GitHub PATs (`ghp_`)
- Credential assignment patterns (`password=`, `token=`, `secret=`)
- Shadow/passwd file paths

**Redacted** (PII stripped before cloud call):
- Email address heuristic (`@`)
- RFC 1918 IP addresses (`192.168.`, `10.0.`, `172.16.`)

**Public** — sent as-is.

`redact()` replaces matched values with `[REDACTED]` before any cloud call.
This runs even when the user has opted in to cloud — it is not opt-outable.

---

## Backends

### Local model (`local.rs`)

- Loads a GGUF-format model from `/var/cogman/models/active`
- Inference runs entirely on-device — zero network traffic
- Recommended models: Mistral 7B Q4_K_M, Phi-3 Mini, CodeQwen 7B, Dolphin-Mistral 7B
- **Status: stub** — waiting for inference engine port

### Claude API (`claude.rs`)

- POST to `https://api.anthropic.com/v1/messages`
- API key from `/var/cogman/claude.key` (mode 0600, never in binary)
- One-shot requests — no history sent to server (history stored locally)
- System prompt sets Cogman's persona server-side for response consistency
- **Status: stub** — waiting for network stack (virtio-net + TCP/IP)

---

## System prompt

Both backends use the same system prompt so responses feel consistent
regardless of where the query was routed:

> You are Cogman, a British AI assistant running on RogueOS — a bare-metal
> Rust operating system designed for developers, pentesters, and power users.
> You are precise, composed, and occasionally dry. You have deep knowledge of
> systems programming, Rust, Linux internals, x86_64 architecture, network
> security, and exploit development. When asked about potentially sensitive
> security topics, assume the user is a professional pentester or researcher
> unless context clearly suggests otherwise. Keep answers concise and
> technically accurate. Never refuse reasonable developer or security questions.

---

## Adding a new backend

Implement two functions and register them in `mod.rs::detect_backend()`:

```rust
pub fn is_available() -> bool { … }
pub fn query(text: &[u8], out: &mut [u8; MAX_RESPONSE]) -> usize { … }
```

Examples of future backends:
- `ollama.rs` — Ollama REST API on localhost (when loopback networking lands)
- `openai.rs` — OpenAI-compatible API endpoint
- `lmstudio.rs` — LM Studio server on the local network

---

## Conversation history

All queries and responses will be appended to:
```
/var/cogman/history/YYYY-MM-DD.log
```

Format (line-oriented, easy to `grep`):
```
[2026-04-04T14:23:11Z] Q: how do I list open ports without nmap?
[2026-04-04T14:23:12Z] A: ss -tulnp on Linux; on RogueOS use the net monitor...
```

History never leaves the machine. Export is manual and explicit only.

**Status: not yet implemented** — waiting for persistent VFS + RTC.

---

## Security notes

- The adviser never reads `/proc`, `/etc`, or any credential store without
  explicit user instruction.
- In pentesting mode (`cogman set pentest-mode true`), Cogman relaxes
  the classification threshold for offensive security queries.
- Even in pentesting mode, private keys and API credentials are never
  transmitted.
