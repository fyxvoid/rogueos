# Syscall ABI Reference

Kingdom uses the x86_64 `SYSCALL`/`SYSRET` fast path. Syscall numbers are defined in `lib/src/lib.rs` and shared between kernel and userland.

---

## Calling Convention

| Register | Role |
|----------|------|
| `rax` | Syscall number (input) / return value (output) |
| `rdi` | Argument 1 |
| `rsi` | Argument 2 |
| `rdx` | Argument 3 |
| `r10` | Argument 4 (rcx is clobbered by SYSCALL) |
| `r8`  | Argument 5 |
| `r9`  | Argument 6 |

Return value: positive or zero on success; negative `SYSERR_*` on failure.

All user pointers are validated by the kernel before access. Passing an invalid pointer returns `SYSERR_INVAL` rather than faulting.

---

## Error Codes

| Constant | Value | Meaning |
|----------|-------|---------|
| `SYSERR_INVAL` | -1 | Invalid argument or unsupported operation |
| `SYSERR_NOENT` | -2 | No such file, process, or entry |
| `SYSERR_BADFD` | -3 | Bad file descriptor |
| `SYSERR_MFILE` | -4 | Too many open files |
| `SYSERR_NOMEM` | -5 | Out of memory or queue slots |
| `SYSERR_AGAIN` | -11 | Resource temporarily unavailable (try again) |

---

## I/O Namespace (0x100)

### `SYS_READ` = 0x100
```
rdi: fd (u32)
rsi: buf (*mut u8)
rdx: len (usize)
→   bytes read, or negative error
```

### `SYS_WRITE` = 0x101
```
rdi: fd (u32)   [1 = serial/stdout]
rsi: buf (*const u8)
rdx: len (usize)
→   bytes written, or negative error
```

### `SYS_OPEN` = 0x102
```
rdi: path (*const u8)
rsi: path_len (usize)
rdx: flags (u32)   [O_RDONLY=0, O_WRONLY=1, O_RDWR=2, O_CREAT=0x40, O_TRUNC=0x200]
→   fd (positive), or negative error
```

### `SYS_CLOSE` = 0x103
```
rdi: fd (u32)
→   0, or negative error
```

### `SYS_LSEEK` = 0x104
```
rdi: fd (u32)
rsi: offset (i64)
rdx: whence (u32)   [SEEK_SET=0, SEEK_CUR=1, SEEK_END=2]
→   new offset, or negative error
```

### `SYS_UNLINK` = 0x105
```
rdi: path (*const u8)
rsi: path_len (usize)
→   0, or negative error
```

### `SYS_FSYNC` = 0x106
```
rdi: fd (u32)
→   0, or negative error
```

### `SYS_LIST_ROOT` = 0x107
```
rdi: buf (*mut u8)   [output: newline-separated filenames]
rsi: capacity (usize)
→   bytes written, or negative error
```

### `SYS_REBOOT` = 0x108
```
rdi: mode (u32)   [0 = halt, 1 = reboot]
→   negative error on failure; does not return on success
```

### `SYS_EXIT` = 0x109
```
rdi: status (i32)
→   never returns
```

---

## Graphics Namespace (0x200)

### `SYS_POLL_INPUT` = 0x200
```
rdi: ev (*mut KeyEvent)
→   > 0 if event written, 0 if no event, negative error
```
`KeyEvent` layout: `{ keycode: u8, pressed: bool }` — 2 bytes.

### `SYS_FB_CLEAR` = 0x201
```
rdi: color (u32, X8R8G8B8)
→   0, or negative error
```

### `SYS_FB_FILL_RECT` = 0x202
```
rdi: x (u32)
rsi: y (u32)
rdx: w (u32)
r10: h (u32)
r8:  color (u32, X8R8G8B8)
→   0, or negative error
```

### `SYS_FB_FLUSH` = 0x203
```
→   0, or negative error
```

### `SYS_POLL_MOUSE` = 0x204
```
rdi: ev (*mut MouseEvent)
→   > 0 if event written, 0 if no event, negative error
```
`MouseEvent` layout: `{ dx: i16, dy: i16, buttons: u8 }` — 5 bytes.

### `SYS_FB_BLIT` = 0x215
```
rdi: dst_x (u32)
rsi: dst_y (u32)
rdx: w (u32)
r10: h (u32)
r8:  stride (u32, bytes per row)
r9:  buf (*const u8, 32bpp ARGB)
→   0, or negative error
```

### Surface Protocol (0x210–0x214)

#### `SYS_SURFACE_CREATE` = 0x210
```
→   surface_id (u32, > 0), or negative error
```

#### `SYS_SURFACE_DESTROY` = 0x211
```
rdi: surface_id (u32)
→   0, or negative error
```

#### `SYS_SURFACE_ATTACH` = 0x212
```
rdi: surface_id (u32)
rsi: buf (*const u8, 32bpp ARGB)
rdx: width (u32)
r10: height (u32)
r8:  stride (u32, bytes)
→   0, or negative error
```

#### `SYS_SURFACE_COMMIT` = 0x213
```
rdi: surface_id (u32)
rsi: dst_x (u32)
rdx: dst_y (u32)
→   0, or negative error
```

#### `SYS_SCREEN_SIZE` = 0x214
```
rdi: out_w (*mut u32)
rsi: out_h (*mut u32)
→   0, or negative error
```

---

## Process Namespace (0x300)

### `SYS_DEBUG_DUMP_PTES` = 0x300
```
rdi: cr3 (u64)
rsi: va_start (u64)
rdx: va_end (u64)
→   0 (debug output on serial)
```

### `SYS_SPAWN` = 0x301
```
rdi: program_id (u32)   [see program ID table]
→   pid (u32, > 0), or negative error
```
Spawns a new process from the registered ELF for `program_id`. The new process starts at its ELF entry point with a fresh user stack.

### `SYS_GET_PROC_INFO` = 0x302
```
rdi: buf (*mut ProcInfo)
rsi: capacity (u32)
→   count filled, or negative error
```
`ProcInfo` layout: `{ pid: u32, state: u8 }`. States: 0=Empty, 1=Runnable, 2=Running, 3=Blocked, 4=Dead.

### `SYS_GETPID` = 0x303
```
→   pid (u32), or negative error
```

### `SYS_WAITPID` = 0x304
```
rdi: pid (u32)      [0 or u32::MAX = any dead process]
rsi: status (*mut i32, may be null)
rdx: options (u32)  [WNOHANG = 0x01]
→   reaped pid, SYSERR_AGAIN (nothing to reap, WNOHANG set), or SYSERR_INVAL
```

---

## IPC Namespace (0x320)

### `SYS_IPC_SEND` = 0x320
```
rdi: target_pid (u32)
rsi: msg (*const KwmMsg)   [64 bytes]
rdx: flags (u32)           [0 = block if queue full; IPC_NONBLOCK = 0x01]
→   0, SYSERR_NOMEM (queue full), or SYSERR_NOENT (no such pid)
```
The kernel fills `msg.sender_pid` with the calling process's PID.

### `SYS_IPC_RECV` = 0x321
```
rdi: out (*mut KwmMsg)   [64 bytes]
rsi: flags (u32)         [0 = block; IPC_NONBLOCK = 0x01]
→   0 (msg written), or SYSERR_AGAIN (queue empty, non-blocking)
```

---

## Debug Namespace (0x400)

### `SYS_HW_BP_SET` = 0x400
```
rdi: slot (u64, 0–3)
rsi: addr (u64)
rdx: cond (u64)   [0=execute, 1=write, 2=io_rw, 3=read_write]
r10: len  (u64)   [0=1B, 1=2B, 2=8B, 3=4B]
→   0, or negative error
```

### `SYS_HW_BP_CLEAR` = 0x401
```
rdi: slot (u64)   [0xFF = clear all]
→   0, or negative error
```

### `SYS_HW_BP_QUERY` = 0x402
```
rdi: out (*mut HwBpInfo, 64 bytes)
→   0, or negative error
```

---

## Performance Namespace (0x410)

### `SYS_PERF_OPEN` = 0x410
```
rdi: event_id (u64)
     0 = cycles          1 = instructions   2 = L1d-access
     3 = L1d-miss        4 = L2-access      5 = L2-miss
     6 = branches        7 = branch-mispr   8 = icache-miss
     9 = stall-cycles
→   handle (u64, > 0), or negative error
```

### `SYS_PERF_READ` = 0x411
```
rdi: handle (u64)
rsi: out (*mut u64)
→   0, or negative error
```

### `SYS_PERF_CLOSE` = 0x412
```
rdi: handle (u64)
→   0, or negative error
```

---

## Scheduler Namespace (0x420)

### `SYS_SET_NICE` = 0x420
```
rdi: nice (i64, -20 to +19)
→   0, or negative error
```
Higher nice = lower priority. Maps to the EEVDF scheduler's virtual deadline calculation.

---

## Key Constants

```rust
// IPC flags
IPC_NONBLOCK: u32 = 0x01

// waitpid options
WNOHANG: u32 = 0x01

// Well-known PIDs
COGMAN_PID: u32 = 1   // cogman supervisor always gets PID 1

// Open flags
O_RDONLY: u32 = 0
O_WRONLY: u32 = 1
O_RDWR:   u32 = 2
O_CREAT:  u32 = 0x40
O_TRUNC:  u32 = 0x200

// Seek whence
SEEK_SET: u32 = 0
SEEK_CUR: u32 = 1
SEEK_END: u32 = 2

// UIDs
UID_KERNEL:          u32 = 0
DEFAULT_SESSION_UID: u32 = 1000
```
