# Cogman — RogueOS Init Supervisor

Cogman is the first userland process on RogueOS. It runs as PID 1 and is responsible for spawning, watching, and healing every other process on the system.

---

## Responsibilities

| Role | Description |
|------|-------------|
| Init | First userland process after kernel handoff |
| Supervisor | Watches child processes; restarts them on crash |
| Control point | Accepts IPC commands from any process to start/stop/query services |
| Service table | Maintains a static registry of known services and their current state |

---

## Source

`userland/src/bin/cogman.rs` — `#![no_std]`, `#![no_main]`, no heap required.

---

## Service Table

Cogman maintains a fixed-size table of up to 16 service entries:

```rust
struct ServiceEntry {
    program_id:    u32,           // kernel program ID (for SYS_SPAWN)
    name:          [u8; 16],      // NUL-terminated display name
    pid:           u32,           // current PID (0 = not running)
    state:         SvcState,      // Stopped | Running | Failed | Restarting
    policy:        RestartPolicy, // Never | OnFailure | Always
    auto_start:    bool,          // spawn at cogman startup
    restart_count: u16,           // number of restarts so far
    restart_at:    u32,           // countdown to next restart attempt
    last_exit:     i32,           // most recent exit status
}
```

### Restart Policies

| Policy | Restart condition |
|--------|-------------------|
| `Never` | Never restart (manual services, one-shot programs) |
| `OnFailure` | Restart only if exit status is non-zero |
| `Always` | Restart unconditionally (critical services like session) |

### Default Services

| Name | Program ID | Policy | Auto-start |
|------|-----------|--------|------------|
| session | 8 | Always | Yes |
| shell | 0 | OnFailure | No |
| monitor | 5 | OnFailure | No |

---

## Supervisor Loop

```
loop:
  1. reap_dead()
     sys_waitpid(ANY, status, WNOHANG)
     While reaped > 0:
       Find service entry by pid
       Update state: Stopped (policy=Never or clean exit) or Restarting
       Set restart_at = RESTART_DELAY_ITERS (≈300 ms)

  2. tick_restarts()
     For each Restarting entry: decrement restart_at

  3. start_pending()
     For each entry where:
       state == Stopped && auto_start == true
       OR state == Restarting && restart_at == 0
     → increment restart_count
     → sys_spawn(program_id)
     → set state = Running, pid = new_pid

  4. handle_ipc()
     sys_ipc_recv(IPC_NONBLOCK) — drain all queued messages
     Dispatch by RwmType

  5. spin(1000 poll_input ticks)  ≈ 10 ms delay
```

---

## IPC Control Interface

Cogman listens on PID 1. Any process can send these messages:

### CogList (0x40)
No payload. Cogman responds with one `CogResp` message per registered service.

### CogStart (0x41)
```
payload.cog_ctrl.program_id = <target>
```
Marks the service as auto_start if it is stopped or failed. Cogman will spawn it on the next `start_pending` iteration. Responds with `CogResp` showing new state.

### CogStop (0x42)
```
payload.cog_ctrl.program_id = <target>
```
Sets restart policy to `Never` and state to `Stopped` before the kill, so the SIGCHLD-equivalent path (reap_dead) does not restart it. Responds with `CogResp`.

### CogStatus (0x43)
```
payload.cog_ctrl.program_id = <target>
```
Responds with a single `CogResp` for that service.

### CogRestart (0x45)
```
payload.cog_ctrl.program_id = <target>
```
Forces state to `Restarting` with `restart_at = 0`, causing an immediate respawn on the next loop iteration. Responds with `CogResp`.

### Ping (0x21)
Cogman responds with `Ack` (seq mirrored).

### CogResp (0x44) — Response format
```
payload.cog_ctrl:
  program_id:    which service this is about
  state:         0=stopped, 1=running, 2=failed, 3=restarting
  restart_count: how many times it has been restarted
  pid:           current PID (0 if not running)
  name:          NUL-terminated name, e.g. "session\0..."
```

---

## Example: Query All Services from Shell

```rust
// In a userland program
use libs::{RwmMsg, RwmType, IPC_NONBLOCK, COGMAN_PID};
use userland::{sys_ipc_send, sys_ipc_recv, sys_write};

fn query_services() {
    let mut req = RwmMsg::ZERO;
    req.msg_type = RwmType::CogList as u8;
    sys_ipc_send(COGMAN_PID, &req, 0);

    loop {
        let mut resp = RwmMsg::ZERO;
        if sys_ipc_recv(&mut resp, IPC_NONBLOCK) < 0 { break; }
        if resp.msg_type != RwmType::CogResp as u8 { continue; }
        let ctrl = unsafe { &resp.payload.cog_ctrl };
        let state = match ctrl.state {
            0 => b"stopped   " as &[u8],
            1 => b"running   ",
            2 => b"failed    ",
            3 => b"restarting",
            _ => b"unknown   ",
        };
        sys_write(1, state.as_ptr(), state.len());
        sys_write(1, b"  pid=".as_ptr(), 6);
        // ... print pid and name
    }
}
```

---

## Serial Log Output

All cogman events are written to fd 1 (serial) with the prefix `[COGMAN]`:

```
[COGMAN] v2 init start
[COGMAN] starting auto-start services
[COGMAN] spawned svc=session pid=2
[COGMAN] supervisor loop running
[COGMAN] reaped pid=2 status=1
[COGMAN] scheduling restart svc=session
[COGMAN] spawned svc=session pid=3
```

---

## Integration with the Kernel

The kernel embeds the cogman ELF at compile time:

```rust
// kernel/init/main.rs
const COGMAN_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cogman.elf"));

// Phase 7: spawn cogman as init
let first_elf = if COGMAN_ELF starts with ELF magic {
    COGMAN_ELF
} else {
    INIT_ELF   // fallback to legacy steward
};
create_user_process(first_elf) → run_first_process()
```

Cogman is also registered as program_id 10 so it can be respawned via `SYS_SPAWN(10)` if needed, but in normal operation only one instance should run.

---

## Extending the Service Table

To add a new auto-start service, edit `userland/src/bin/cogman.rs` in the `TABLE` constant initializer:

```rust
t[3] = Some(ServiceEntry::new(
    PROG_MYAPP,            // program_id registered in kernel/init/main.rs
    b"myapp\0\0\0\0\0\0\0\0\0\0\0",  // 16 bytes exactly
    RestartPolicy::OnFailure,
    true,                  // auto_start
));
```

Then register the ELF in `kernel/init/main.rs`:
```rust
const MYAPP_ELF: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/myapp.elf"));
// ...
crate::kernel::programs::register(11, MYAPP_ELF);  // expand MAX_PROGRAMS in programs.rs too
```
