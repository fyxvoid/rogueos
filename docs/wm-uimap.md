# RogueOS Window Manager — UI Map

Canonical reference for wm.rs (prog_id=9). All coordinates, colors, keybindings, and protocol flows live here. Change this document first, then the code.

---

## Screen Layout

```
┌─────────────────────────────────────── 1280px ───────────────────────────────────────────┐
│ BAR (24px tall)                                                                           │
│ [1][2][3]...[9]  [TL]  ←── focused window title ──→                        HH:MM:SS     │
├───────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                           │
│  CLIENT AREA  (1280 × (height-24) px)                                                    │
│  Layout engine subdivides this rectangle per mode                                        │
│                                                                                           │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

| Constant    | Value | Meaning                        |
|-------------|-------|--------------------------------|
| BAR_H       | 24 px | Status bar height              |
| BORDER      | 2 px  | Window border width            |
| GAP         | 8 px  | Gap between tiles (gaps mode)  |
| TITLE_H     | 20 px | Per-client title bar height    |
| SCREEN_W    | 1280  | QEMU GOP default width         |
| SCREEN_H    | 800   | QEMU GOP default height        |

---

## Layout Modes

```
TILE (nmaster=1, mfact=55%)     MONOCLE               GRID (4 clients)
┌─────────┬──────┐              ┌────────────┐         ┌──────┬──────┐
│ master  │  s1  │              │            │         │  c1  │  c2  │
│         ├──────┤              │  focused   │         ├──────┼──────┤
│         │  s2  │              │            │         │  c3  │  c4  │
└─────────┴──────┘              └────────────┘         └──────┴──────┘

BSTACK (master on top)          SPIRAL (4 clients)     CENTERED MASTER
┌────────────────┐              ┌────┬───┬──┐           ┌──┬──────┬──┐
│    master      │              │    │ 2 │3 │           │s1│master│s2│
├────┬─────┬─────┤              │ 1  ├───┘  │           ├──┤      ├──┤
│ s1 │ s2  │ s3  │              │    │  4   │           │s3│      │s4│
└────┴─────┴─────┘              └────┴──────┘           └──┴──────┴──┘

FLOAT (free placement)
Windows retain their last geometry; drag to reposition.
```

---

## Keyboard Bindings

Mod key = Left Super (configurable in wm.rs `MOD_KEY`).

| Shortcut         | Action                               |
|------------------|--------------------------------------|
| Mod+1…9          | Switch to tag N                      |
| Mod+Shift+1…9    | Move focused window to tag N         |
| Mod+0            | View all tags                        |
| Mod+Tab          | Toggle to previous tagset            |
| Mod+J            | Focus next window                    |
| Mod+K            | Focus prev window                    |
| Mod+H            | Master width −5%                     |
| Mod+L            | Master width +5%                     |
| Mod+,            | Decrease nmaster                     |
| Mod+.            | Increase nmaster                     |
| Mod+Space        | Cycle layout →                       |
| Mod+Shift+Space  | Cycle layout ←                       |
| Mod+Enter        | Zoom (swap focused to master)        |
| Mod+D            | Spawn terminal (prog_id=12)          |
| Mod+E            | Spawn editor (prog_id=2)             |
| Mod+V            | Spawn viewer (prog_id=3)             |
| Mod+F            | Toggle fullscreen                    |
| Mod+T            | Toggle floating                      |
| Mod+G            | Toggle gaps                          |
| Mod+B            | Toggle bar                           |
| Mod+Shift+C      | Close focused client (RDP_CLOSE)     |
| Mod+Shift+Q      | Reboot (sys_reboot(1))               |

---

## Color Palette (Tokyo Night)

```rust
BG_BAR       = 0xFF1A1B26   // status bar background
BG_DESKTOP   = 0xFF24283B   // desktop fill
BG_WINDOW    = 0xFF1F2335   // unfocused window fill
BORDER_FOCUS = 0xFF7AA2F7   // focused border (blue)
BORDER_NORM  = 0xFF3D59A1   // unfocused border (dim blue)
TEXT_ACTIVE  = 0xFFCDD6F4   // active tag/title text
TEXT_NORMAL  = 0xFF565F89   // inactive text
TAG_ACTIVE   = 0xFF7AA2F7   // active tag highlight
TAG_OCCUPIED = 0xFF3D59A1   // tag with windows (dim)
```

---

## RDP Protocol State Machine

```
Terminal                    WM (compositor)
   │                             │
   │─ surface_create() ──────────│→ kernel: surface_id = N
   │─ IPC RDP_CONNECT ──────────→│  payload: {surface_id, title}
   │←─ IPC RDP_GRANT ───────────│  payload: {surface_id, x, y, w, h}
   │                             │
   │  [renders into pixel buf]   │
   │─ surface_attach(id,ptr,w,h)─│→ kernel: bind ptr to surface
   │─ IPC RDP_COMMIT ───────────→│  payload: {surface_id}
   │                             │  WM calls surface_commit(id, dst_x, dst_y)
   │←─ IPC RDP_PRESENT_DONE ────│  payload: {surface_id}
   │                             │
   │←─ IPC RDP_KEY ─────────────│  payload: {keycode, state}
   │←─ IPC RDP_RESIZE ──────────│  payload: {w, h}  (on geometry change)
   │←─ IPC RDP_CLOSE ───────────│  (polite shutdown request)
   │─ surface_destroy(id) ───────│→ kernel: free slot
   │─ sys_exit() ───────────────→│
```

---

## Window Lifecycle State Machine

```
NONE → [supervisor spawns prog_id] → CONNECTING
  CONNECTING: waiting for RDP_CONNECT IPC from client
  → ACTIVE: RDP_CONNECT received; GRANT sent; geometry set by layout engine
    → FULLSCREEN: Mod+F                 (back: Mod+F)
    → FLOATING:   Mod+T                 (back: Mod+T)
  → CLOSING: RDP_CLOSE sent by WM
  → GONE: surface_destroy received OR process exited
```

---

## Fixed Limits

| Constant         | Value    | Location                        |
|------------------|----------|---------------------------------|
| MAX_CLIENTS      | 16       | wm.rs                           |
| MAX_TAGS         | 9        | wm.rs                           |
| MAX_SERVICES     | 16       | supervisor.rs                   |
| IPC_QUEUE_DEPTH  | 32       | kernel/process/ipc.rs           |
| MAX_SURFACES     | 16       | display_server/mod.rs           |
| TITLE_MAX        | 20 chars | wm.rs Client.title              |
| STACK_SLOT_PAGES | 9        | process.rs (8 stack + 1 guard)  |
| USER_STACK_TOP   | 0x7fff_ffff_f000 | process.rs               |

---

## Binary Load Addresses (shared CR3)

All binaries coexist in the same page table. No overlaps allowed.

| Program   | prog_id | Load Base  | Linker Script   |
|-----------|---------|------------|-----------------|
| cogman    | 10      | 0x0400000  | cogman.ld       |
| shell     | 0       | 0x0600000  | shell.ld        |
| fbtest    | 11      | 0x0A00000  | fbtest.ld       |
| session   | 8       | 0x0C00000  | session.ld      |
| wm        | 9       | 0x0E00000  | wm.ld           |
| rwm       | 1       | 0x1C00000  | rwm.ld          |
| terminal  | 12      | 0x1E00000  | terminal.ld     |

**Guard rule:** before adding a new binary, pick an address ≥ 0x0200000 away from existing entries and add it to this table.

---

## Boot Chain

```
Kernel → cogman (pid=1)
  cogman → fbtest (pid=2, auto-start)
  fbtest exits →
  cogman → wm (pid=3, chained)
    wm claims compositor
    wm maps framebuffer
    wm renders desktop + status bar
    Mod+D → wm spawns terminal (pid=4)
```

---

## Pitfall Prevention

| Pitfall                  | Status     | Guard rule                                                                 |
|--------------------------|------------|----------------------------------------------------------------------------|
| syscall rcx/r11 clobbers | ✅ Fixed   | Every syscall wrapper in lib.rs MUST have `out("rcx") _` and `out("r11") _` |
| Shared-CR3 stack overwrite | ✅ Fixed  | stack_top_for_pid() assigns unique VA per process                          |
| Kernel rebuild cache miss | ✅ Fixed  | `make image` auto-touches kernel/build.rs                                  |
| Service name on stack    | ✅ Fixed   | prog_name() always returns &'static [u8]                                   |
| prog_id=1/9 naming mess  | ✅ Fixed   | prog_id=1→rwm, prog_id=9→wm in supervisor and kernel comments              |
| Compositor double-claim  | Handled    | Kernel rejects second claim; wm logs and proceeds                          |
| IPC queue overflow       | Known limit | WM drains IPC every loop; depth=32 is sufficient for terminal frame rate  |
