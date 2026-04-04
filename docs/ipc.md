# IPC Protocol

RogueOS processes communicate through a kernel-mediated message-passing system. There is no shared memory by default. Every message is a fixed 64-byte `RwmMsg` (one cache line).

---

## Message Format

```
struct RwmMsg {               // 64 bytes, align(64)
    msg_type:   u8,           // RwmType discriminant
    flags:      u8,           // reserved; set to 0
    seq:        u16,          // caller-assigned sequence number (for ACK matching)
    sender_pid: u32,          // filled by the kernel on SYS_IPC_SEND
    payload:    [u8; 56],     // union — interpret based on msg_type
}
```

The `payload` field is a 56-byte union. Always access the correct variant for the `msg_type`.

---

## Sending and Receiving

```rust
// Send (non-blocking attempt)
let r = sys_ipc_send(target_pid, &msg, IPC_NONBLOCK);
// r == 0: delivered to target's queue
// r == SYSERR_NOMEM: target queue full (256 slots)
// r == SYSERR_NOENT: no process with that PID

// Receive (non-blocking)
let mut msg = RwmMsg::ZERO;
let r = sys_ipc_recv(&mut msg, IPC_NONBLOCK);
// r == 0: msg filled
// r == SYSERR_AGAIN: queue empty

// Receive (blocking — yields until a message arrives)
let r = sys_ipc_recv(&mut msg, 0);
```

---

## Message Types

### App → Window Manager

| Type | Value | Payload | Description |
|------|-------|---------|-------------|
| `Register` | 0x01 | `PayloadRegister` | App introduces itself; sends title and flags |
| `Unregister` | 0x02 | — | App is exiting; WM should remove its entry |
| `SetTitle` | 0x03 | `PayloadSetTitle` | Update window title string |
| `SurfaceCommit` | 0x04 | `PayloadSurfaceCommit` | Pixel buffer is ready at given surface_id |

### Window Manager → App

| Type | Value | Payload | Description |
|------|-------|---------|-------------|
| `Geometry` | 0x10 | `PayloadGeometry` | WM informs app of its on-screen position/size |
| `EventKey` | 0x11 | `PayloadEventKey` | Forwarded keyboard event |
| `EventMouse` | 0x12 | `PayloadEventMouse` | Forwarded mouse event |
| `EventFocus` | 0x13 | `PayloadEventFocus` | Focus gained (1) or lost (0) |
| `EventResize` | 0x14 | `PayloadEventResize` | WM has resized the window |

### Bidirectional

| Type | Value | Payload | Description |
|------|-------|---------|-------------|
| `Ack` | 0x20 | — | Acknowledge; `seq` mirrors the request's `seq` |
| `Ping` | 0x21 | — | Keepalive; sender expects an `Ack` response |

### Display Server → App

| Type | Value | Payload | Description |
|------|-------|---------|-------------|
| `SurfaceAssign` | 0x30 | `PayloadSurfaceAssign` | DS assigns a kernel surface_id to the app |

### Cogman Control (0x40–0x45)

Sent to `COGMAN_PID` (1) to manage services. Cogman replies with `CogResp`.

| Type | Value | Payload | Description |
|------|-------|---------|-------------|
| `CogList` | 0x40 | — | Request list of all services |
| `CogStart` | 0x41 | `PayloadCogCtrl` | Start a service by `program_id` |
| `CogStop` | 0x42 | `PayloadCogCtrl` | Stop a running service |
| `CogStatus` | 0x43 | `PayloadCogCtrl` | Query status of one service |
| `CogResp` | 0x44 | `PayloadCogCtrl` | Cogman's response to any Cog* request |
| `CogRestart` | 0x45 | `PayloadCogCtrl` | Restart a service |

---

## Payload Layouts

All payloads are exactly **56 bytes**.

### `PayloadRegister` (0x01)
```
title: [u8; 48]    NUL-terminated window title
flags: u32
_pad:  [u8; 4]
```

### `PayloadSetTitle` (0x03)
```
title: [u8; 56]    NUL-terminated, up to 55 chars
```

### `PayloadSurfaceCommit` (0x04)
```
surface_id: u32
x:          i32    position hint
y:          i32
w:          u32    render dimensions
h:          u32
_pad:       [u8; 36]
```

### `PayloadGeometry` (0x10)
```
x:    i32
y:    i32
w:    u32
h:    u32
_pad: [u8; 40]
```

### `PayloadEventKey` (0x11)
```
keycode: u8    see lib/src/lib.rs :: keycodes::*
pressed: u8    1 = down, 0 = up
_pad:    [u8; 54]
```

### `PayloadEventMouse` (0x12)
```
abs_x:   i32     absolute screen position
abs_y:   i32
dx:      i16     delta since last event
dy:      i16
buttons: u8      bit 0 = left, bit 1 = right, bit 2 = middle
_pad:    [u8; 43]
```

### `PayloadEventFocus` (0x13)
```
focused: u8    1 = gained focus, 0 = lost focus
_pad:    [u8; 55]
```

### `PayloadEventResize` (0x14)
```
w:    u32
h:    u32
_pad: [u8; 48]
```

### `PayloadSurfaceAssign` (0x30)
```
surface_id: u32    use this ID for SYS_SURFACE_ATTACH / SYS_SURFACE_COMMIT
_pad:       [u8; 52]
```

### `PayloadCogCtrl` (0x40–0x45)
```
program_id:    u32     target service (request) or reporting service (response)
action:        u8      request: 0=none,1=start,2=stop,3=restart,4=status,5=list
state:         u8      response: 0=stopped,1=running,2=failed,3=restarting
restart_count: u16     response: how many times this service has been restarted
pid:           u32     response: current PID (0 if not running)
name:          [u8; 16] NUL-terminated service name
_pad:          [u8; 28]
```

---

## Querying Cogman

Any process can query the supervisor:

```rust
// List all services
let mut req = RwmMsg::ZERO;
req.msg_type = RwmType::CogList as u8;
sys_ipc_send(COGMAN_PID, &req, 0);

// Cogman sends one CogResp per service
loop {
    let mut resp = RwmMsg::ZERO;
    if sys_ipc_recv(&mut resp, IPC_NONBLOCK) < 0 { break; }
    if resp.msg_type == RwmType::CogResp as u8 {
        let ctrl = unsafe { &resp.payload.cog_ctrl };
        // ctrl.program_id, ctrl.state, ctrl.pid, ctrl.name, ctrl.restart_count
    }
}

// Stop a service
let mut req = RwmMsg::ZERO;
req.msg_type = RwmType::CogStop as u8;
unsafe { req.payload.cog_ctrl.program_id = 8; } // stop session
sys_ipc_send(COGMAN_PID, &req, 0);
```

---

## Key Codes

Defined in `lib/src/lib.rs :: keycodes`. Selected values:

| Constant | Value |
|----------|-------|
| `KEY_ENTER` | 5 |
| `KEY_ESC` | 6 |
| `KEY_BACKSPACE` | 7 |
| `KEY_TAB` | 8 |
| `KEY_SPACE` | 9 |
| `KEY_MOD` (Super/Win) | 10 |
| `KEY_SHIFT` | 11 |
| `KEY_CTRL` | 12 |
| `KEY_ALT` | 13 |
| `KEY_1` … `KEY_0` | 20–29 |
| `KEY_A` … `KEY_Z` | 30–55 |
| `KEY_F1` … `KEY_F12` | 70–81 |
