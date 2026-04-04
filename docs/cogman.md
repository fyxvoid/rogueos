# Cogman — RogueOS System Butler & AI Adviser

> *"Good day. I'm Cogman, your system butler and personal adviser.  
>  I shall manage your processes and remain at your disposal."*

Cogman is PID 1 on RogueOS. He is simultaneously the init supervisor
and a British AI assistant — the first voice you hear after the kernel
boots and the last process standing before shutdown.

---

## Identity

Cogman is not a traditional init daemon with a config file and a man page.
He is a **butler** — composed, technically precise, and always available.
He speaks to you rather than printing silent log lines. He understands
developer and security context. He assumes you know what you are doing.

RogueOS targets developers, pentesters, and power users. Cogman's persona
reflects that: he will help you enumerate SMB shares, explain a kernel
subsystem, or walk you through a Rust lifetime error — without disclaimers
or hand-holding.

---

## Responsibilities

| Role | Description |
|------|-------------|
| **PID 1 / init** | First userland process; kernel spawns only Cogman |
| **Supervisor** | Starts, watches, and restarts all other services |
| **Halt / reboot** | Intercepts exit codes 42 (halt) and 43 (reboot) |
| **IPC control point** | Any process can query or command Cogman via IPC |
| **AI adviser** | Routes questions to local model or Claude API |
| **Privacy enforcer** | Blocks sensitive material from leaving the machine |

---

## Architecture

```
Kernel
  └─► spawn(cogman, pid=1)
          │
          ├─ Supervisor loop ──► spawn session, wm, shell …
          │                      watch for crashes, restart per policy
          │
          ├─ IPC inbox loop ──► CogCtrlList / CogCtrlStop / CogCtrlAsk …
          │
          └─ Adviser
               │
               ├─ PrivacyFilter ──► classify + redact
               │
               ├─ local.rs ──► GGUF model (on-device, default)
               │
               └─ claude.rs ──► Claude API (opt-in, network required)
```

---

## Adviser privacy model

Cogman is **local-first**. If a model is loaded, it never talks to the
network. Cloud inference requires explicit opt-in and never receives
sensitive material regardless.

```
sensitive query  ──► refused unless local model present
redacted query   ──► PII stripped ──► cloud (if opted in)
public query     ──► local first, cloud fallback (if opted in)
```

| Setting | Default | How to change |
|---------|---------|--------------|
| Local model | None loaded | `cogman load-model <path>` |
| Cloud opt-in | Off | `cogman set cloud-opt-in true` |
| Pentest mode | Off | `cogman set pentest-mode true` |
| API key | Not set | Write to `/var/cogman/claude.key` (mode 0600) |

---

## IPC protocol

Send a `RwmMsg` to PID 1 with these type bytes:

| Byte | Name | Action |
|------|------|--------|
| `0x41` | CogCtrlStop | Stop a service by program_id |
| `0x42` | CogCtrlStatus | Print status to serial |
| `0x43` | CogCtrlList | List all services and states |
| `0x45` | CogCtrlAsk | Ask the adviser (55-byte NUL-terminated query) |
| `0x44` | CogResp | Response from Cogman to caller |

---

## Service table

Cogman maintains up to 16 service entries at startup:

```rust
struct Service {
    program_id:    u32,          // kernel program ID (SYS_SPAWN argument)
    name:          [u8; 16],     // display name
    pid:           u32,          // current PID (0 = not running)
    state:         SvcState,     // Stopped | Running | Restarting | Failed
    policy:        RestartPolicy,// Never | OnFailure | Always
    auto_start:    bool,
    restart_count: u16,
    restart_delay: u32,          // backoff countdown (ticks)
    last_exit:     i32,
}
```

### Default services

| Name | ID | Policy | Auto-start |
|------|----|--------|------------|
| session | 8 | Always | Yes |
| shell | 0 | OnFailure | No |
| wm | 1 | OnFailure | No |
| monitor | 5 | Never | No |

### Restart backoff

Cogman uses exponential backoff to avoid respawn storms:

| Restart # | Delay (ticks) |
|-----------|--------------|
| 1 | 200 |
| 2 | 400 |
| 3 | 800 |
| 4 | 1600 |
| 5 | 3200 |
| 6+ | 8000 (cap) |

---

## Source layout

```
userland/src/bin/cogman/
  main.rs          Entry point, IPC dispatcher, main event loop
  supervisor.rs    Service table, spawn/reap/restart logic
  persona.rs       All human-facing output strings (British register)
  advisor/
    mod.rs         Privacy filter, backend dispatch, ask() entry point
    claude.rs      Claude API HTTPS client
    local.rs       Local GGUF model inference interface
    README.md      Adviser architecture
  README.md        Cogman overview
```

---

## What's implemented vs planned

| Feature | Status |
|---------|--------|
| Init / supervisor loop | Done |
| British persona (serial output) | Done |
| IPC control (list/status/stop) | Done |
| IPC ask → adviser dispatch | Done (routing) |
| Adviser privacy filter | Done |
| Local model inference | Stub — needs inference engine |
| Claude API calls | Stub — needs network stack |
| `/var/cogman/history/` logging | Planned — needs persistent VFS + RTC |
| `cogman set` config commands | Planned — needs config file + VFS |
| Structured CogResp IPC replies | Planned |
