# Cogman

> *"I shall manage your processes and remain at your disposal."*

Cogman is the first userland process on RogueOS. He is a British AI assistant
and system butler who happens to also run PID 1.

---

## Two hats, one process

### Hat 1 — System Butler (init / supervisor)

Cogman is the first thing the kernel hands control to. He brings up every
other service, watches them, and restarts the ones that need restarting. Halt
and reboot flow through him. The kernel only needs to know one program ID.

| Responsibility | Detail |
|---------------|--------|
| First process | Spawned by the kernel as PID 1 |
| Auto-start services | session, shell, wm (configurable) |
| Restart policy | Per-service: Never / OnFailure / Always |
| Halt / reboot | Shell exits with code 42 (halt) or 43 (reboot) |
| IPC control | Any process can send CogCtrlList, CogCtrlStop, CogCtrlStatus |

### Hat 2 — Personal AI Adviser

Instead of silent log lines and notification popups, Cogman speaks to you.
He routes questions to a local model (privacy-first) or to the Claude API
(opt-in, PII-stripped) when networking is available.

```
You:     cogman ask "how do I enumerate SMB shares without touching Metasploit?"
Cogman:  Quite. Use crackmapexec or smbclient -L //host -N for anonymous
         enumeration. nmap --script smb-enum-shares is also rather effective...
```

---

## Folder structure

```
cogman/
  main.rs          Entry point — init loop + IPC dispatcher
  supervisor.rs    Service table, spawn/reap, restart policies
  persona.rs       All human-facing speech (British register)
  advisor/
    mod.rs         Dispatch: privacy filter → local or cloud
    claude.rs      Claude API client (HTTPS, stub until networking lands)
    local.rs       Local GGUF model interface (stub until engine ships)
    README.md      Adviser architecture deep-dive
```

---

## Persona

Cogman speaks in a composed, precise British manner. He is never flustered,
occasionally dry, and always technically accurate. He is not a chatbot —
he is a professional tool that happens to converse.

Examples of how Cogman speaks:

| Event | Output |
|-------|--------|
| Boot | `Good day. I'm Cogman, your system butler and personal adviser.` |
| Spawn | `Bringing up session (pid=3)` |
| Crash + restart | `session has stepped out (exit=1). Restarting session. This is restart number 1.` |
| Clean exit | `session has exited cleanly and I shan't restart it per policy.` |
| Halt | `Halt requested. Powering down now. Goodbye.` |
| Sensitive query | `I'm afraid that query contains sensitive material. I can only handle it with a local model loaded.` |

---

## IPC control protocol

Send a `RwmMsg` to PID 1 (Cogman) with one of these types:

| Type byte | Name | Payload |
|-----------|------|---------|
| `0x41` | CogCtrlStop | `u32` program_id to stop |
| `0x42` | CogCtrlStatus | empty — Cogman replies on serial |
| `0x43` | CogCtrlList | empty — Cogman lists all services |
| `0x45` | CogCtrlAsk | up to 55 bytes of NUL-terminated query text |
| `0x44` | CogResp | response from Cogman (future: structured reply) |

---

## Privacy model

```
Local model loaded?  ──YES──► query stays on-device, always
        │
        NO
        │
Cloud opt-in?  ──YES──► classify query
        │                    │
        NO                   ├─ Sensitive ──► refuse / local only
        │                    ├─ Redacted  ──► strip PII, then send
        │                    └─ Public    ──► send as-is
        │
  Offline response
```

**Sensitive** material (API keys, private keys, `/etc/shadow`, tokens) is
**never** transmitted regardless of opt-in status.

Cloud opt-in is off by default. Enable with:
```
cogman set cloud-opt-in true
```

The API key lives in `/var/cogman/claude.key` (mode 0600). It is never
embedded in the binary or transmitted in any log.

---

## Target audience

RogueOS — and therefore Cogman — is built for:

1. **Developers** — Rust, systems, embedded, kernel hackers
2. **Pentesters** — Cogman understands offensive and defensive security
3. **Power users** — people who want to know what their computer is doing

Cogman does not treat the user as a liability. He assumes competence.

---

## Adding a local model

```
# Copy a GGUF model into the models directory
cp ~/mistral-7b-q4_k_m.gguf /var/cogman/models/

# Set it as active
ln -sf /var/cogman/models/mistral-7b-q4_k_m.gguf /var/cogman/models/active

# Cogman will load it on next boot, or send:
cogman load-model /var/cogman/models/mistral-7b-q4_k_m.gguf
```

Recommended: any GGUF-format model compatible with llama.cpp.

---

## What's not implemented yet

| Feature | Blocker |
|---------|---------|
| Local model inference | Inference engine (llama.cpp port or native) |
| Claude API calls | Network stack (virtio-net + TCP/IP) |
| Structured IPC replies | CogResp payload format finalization |
| `cogman load-model` command | VFS mmap + model loader |
| `/var/cogman/history/` logging | Persistent VFS writes |
| `cogman set` config commands | Config file format + VFS |
