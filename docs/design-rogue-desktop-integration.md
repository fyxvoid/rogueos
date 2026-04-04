# Rogue Desktop ↔ RogueOS Integration Design

## TL;DR

**rogue-desktop is X11/Wayland-native. It cannot run on RogueOS directly.**
Its *algorithms* (layout engine, compositor effects) are already ported to
`rogueos/userland/`. The integration work is wiring those pieces together through
RogueOS's kernel Surface API instead of through an X server.

A **display server and compositor are both needed** — see rationale below.

---

## What rogue-desktop contains

| Component | Protocol | Portable to RogueOS? | Status |
|-----------|----------|---------------------|--------|
| `rogue-desktop/` (main WM) | X11 (`x11rb`, EWMH) | ❌ No — needs X server | Algorithms ported to `userland/rwm-core/` |
| `dwm-rs/` | X11 (`x11rb`) | ❌ No — needs X server | Ported to `userland/dwm-rs/` + `wm.rs` |
| `rogue-compositor/` | Wayland (Smithay/anvil) | ❌ No — needs Linux Wayland socket | Replace with kernel Surface API |
| `rogue-clip/` | X11 + Wayland clipboard | ❌ No — needs X/Wayland | Ported to `userland/rogue-clip/` (IPC-based) |
| `rogue-shot/` | X11 (`x11-screenshot`) | ❌ No — needs X server | Ported to `userland/rogue-shot/` (sys_fb_blit) |
| `rogue-lock/` | X11 (`xcb`, Cairo) | ❌ No — needs X server + Cairo | Ported to `userland/rogue-lock/` (framebuffer) |
| Layout algorithms | None | ✅ Pure logic | Live in `userland/rwm-core/src/layout.rs` |
| Compositor effects | None | ✅ Pure CPU render | Live in `userland/compositor/src/lib.rs` |
| 9-tag workspace model | None | ✅ Pure logic | Live in `userland/wm.rs` and `rwm-core/` |

**Bottom line:** All the *logic* from rogue-desktop is already in rogueos/userland.
The X11/Wayland *protocol glue* is replaced by RogueOS's custom IPC (RwmMsg) and
the kernel Surface API (SYS_SURFACE_*).

---

## Why a display server is needed

The current WM (`userland/src/bin/wm.rs`) is a **single-process monarch**: it owns
the framebuffer directly via `sys_fb_fill_rect` / `sys_fb_clear` / `sys_fb_flush`
and draws fake placeholder windows. Real apps cannot draw their own content.

A proper display server solves this:

```
App A ──RwmMsg::Register─────────────────────────┐
App B ──RwmMsg::Register──────────────────────────▼
                                         Display Server (userland)
App A ←─RwmMsg::Geometry (x,y,w,h)               │
App B ←─RwmMsg::Geometry (x,y,w,h)               │  Layout engine (rwm-core)
                                                  │
App A ──RwmMsg::SurfaceCommit(id, x,y,w,h)──────►│
App B ──RwmMsg::SurfaceCommit(id, x,y,w,h)──────►│
                                                  │  SYS_SURFACE_ATTACH + COMMIT
                                                  ▼
                                        Kernel Display Server
                                        (display/display_server)
                                                  │
                                                  ▼
                                        GOP Framebuffer (physical display)
```

Without a display server, apps can only draw via `sys_fb_*` and there is no
isolation — any app can scribble over any part of the screen.

---

## Why a compositor is needed

The kernel display server (`display/display_server/mod.rs`) does **opaque blit**
only — it copies pixels without alpha blending. To get:

- Window transparency (current compositor in `userland/compositor/` handles this)
- Rounded corners (already in `backend.rs` via `fill_rect_rounded`)
- Per-window decorations (border, title bar) drawn by the WM, not apps
- Z-order compositing (WM draws window frames above app content)

…the compositor must run **in userland** between the WM and the kernel surface
commit. The kernel is NOT the right place for alpha blending or decoration drawing
(this is the "push complexity to userland" rule from prompt.md).

---

## Target architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  USERLAND                                                        │
│                                                                  │
│  ┌──────────┐   RwmMsg IPC    ┌────────────────────────────┐   │
│  │  App A   │◄───────────────►│                            │   │
│  └──────────┘                 │   rogue_ds                 │   │
│  ┌──────────┐   RwmMsg IPC    │   (Display Server binary)  │   │
│  │  App B   │◄───────────────►│                            │   │
│  └──────────┘                 │  ┌──────────────────────┐  │   │
│                               │  │  WM (rwm-core)       │  │   │
│                               │  │  Layouts: Tile/Mono/  │  │   │
│                               │  │  Grid/BStack/Spiral   │  │   │
│                               │  │  9-tag workspace      │  │   │
│                               │  └──────────────────────┘  │   │
│                               │  ┌──────────────────────┐  │   │
│                               │  │  Compositor          │  │   │
│                               │  │  Alpha blend         │  │   │
│                               │  │  Rounded corners     │  │   │
│                               │  │  Damage tracking     │  │   │
│                               │  └──────────────────────┘  │   │
│                               └────────────┬───────────────┘   │
│                                            │ SYS_SURFACE_*      │
└────────────────────────────────────────────┼────────────────────┘
                                             │
┌────────────────────────────────────────────┼────────────────────┐
│  KERNEL                                    ▼                     │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  display/display_server  (up to 16 surface slots)      │   │
│  │  SYS_SURFACE_CREATE / ATTACH / COMMIT / DESTROY         │   │
│  └────────────────────────────┬────────────────────────────┘   │
│                               │ blit()                          │
│  ┌────────────────────────────▼────────────────────────────┐   │
│  │  drivers/framebuffer  (GOP 1920×1080 32bpp ARGB)          │   │
│  │  AMD SME encrypted physical memory                      │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

---

## What "integrate" means concretely

### 1. Already done (rogue-desktop → rogueos/userland)

| rogue-desktop | rogueos/userland equivalent | State |
|---------------|-----------------------------|-------|
| `rwm-core` layouts | `userland/rwm-core/src/layout.rs` | ✅ Complete (7 layouts) |
| 9-tag workspace | `userland/wm.rs` Wm struct | ✅ Complete |
| Compositor transparency/rounding | `userland/compositor/src/lib.rs` | ✅ Complete |
| `DisplayBackend` trait | `userland/core/src/backend.rs` | ✅ Complete |
| `KernelBackend` (sys_fb_*) | `userland/src/backend_kernel.rs` | ✅ Complete |
| dwm tile/monocle/grid algorithms | `userland/rwm-core/src/layout.rs` | ✅ Complete |
| rogue-clip (clipboard) | `userland/rogue-clip/` | ✅ Ported (IPC-based) |
| rogue-shot (screenshot) | `userland/rogue-shot/` | ✅ Ported (sys_fb_blit) |
| rogue-lock (lock screen) | `userland/rogue-lock/` | ✅ Ported (framebuffer) |

### 2. Written in this session

| What | File | What it does |
|------|------|-------------|
| Display server binary | `userland/src/bin/rogue_ds.rs` | Full multi-client IPC compositor loop |
| Surface-backed compositor | `userland/compositor/src/lib.rs` (extended) | Alpha blend per surface |
| WM surface migration | `userland/src/bin/wm.rs` (updated) | Uses SYS_SURFACE_* per window |

### 3. Not needed / explicitly excluded

| rogue-desktop piece | Why excluded |
|---------------------|-------------|
| `rogue-compositor/` (Smithay) | Requires Linux + Wayland socket. Replaced by kernel Surface API. |
| `rwm-x11` | Requires X server. Replaced by RwmMsg IPC. |
| `rwm-bar` (X11 drawing) | Replaced by framebuffer bar in `wm.rs`. |
| `rwm-plugin` (Lua) | `mlua` links against glibc. Excluded until no_std Lua port exists. |
| `rogue-lock` PAM auth | `libpam-sys` requires glibc. Lock binary uses fallback PIN. |

---

## Kernel surface API status

The kernel surface API is **fully wired end-to-end**:

```
lib/src/lib.rs          SYS_SURFACE_CREATE = 0x210  ✅
                        SYS_SURFACE_ATTACH  = 0x212  ✅
                        SYS_SURFACE_COMMIT  = 0x213  ✅
                        SYS_SURFACE_DESTROY = 0x211  ✅

kernel/syscall/dispatcher/gfx.rs   sys_surface_*()  ✅ implemented
kernel/display/display_server/mod.rs  surface_*()  ✅ implemented
kernel/drivers/framebuffer.rs            blit()        ✅ implemented
```

**Gap before this session:** WM was calling `sys_fb_fill_rect` directly, bypassing
the surface protocol entirely. Fixed in this session.

---

## Surface protocol flow (per frame)

```
1. rogue_ds starts up:
   - calls SYS_SCREEN_SIZE → (1920, 1080)
   - allocates surfaces: bar_surf = SYS_SURFACE_CREATE()
                         bg_surf  = SYS_SURFACE_CREATE()

2. App connects:
   - App calls SYS_IPC_SEND(ds_pid, RwmMsg::Register { title, flags })
   - rogue_ds receives via SYS_IPC_RECV
   - rogue_ds calls SYS_SURFACE_CREATE() → win_surf_id
   - rogue_ds calls SYS_IPC_SEND(app_pid, RwmMsg::Geometry { x,y,w,h })
   - rogue_ds calls SYS_IPC_SEND(app_pid, RwmMsg::SurfaceAssign { surface_id })

3. App renders:
   - App draws into its local pixel buffer (heap-allocated)
   - App calls SYS_SURFACE_ATTACH(win_surf_id, buf_ptr, w, h, stride)
   - App calls SYS_IPC_SEND(ds_pid, RwmMsg::SurfaceCommit { surface_id, x, y, w, h })

4. rogue_ds composites (each input event or on timer):
   - calls SYS_SURFACE_ATTACH(bg_surf, bg_buf, sw, sh, sw*4)
   - calls SYS_SURFACE_COMMIT(bg_surf, 0, 0)          ← background
   - for each window in z-order:
       calls SYS_SURFACE_ATTACH(win_surf, win_buf, w, h, w*4)
       calls SYS_SURFACE_COMMIT(win_surf, x, y)        ← window content
       draws frame/border via sys_fb_fill_rect          ← decoration
   - calls SYS_SURFACE_COMMIT(bar_surf, 0, sh - BAR_H) ← status bar

5. Input dispatch:
   - rogue_ds polls sys_poll_input() / sys_poll_mouse()
   - routes RwmMsg::EventKey to focused app via SYS_IPC_SEND
   - WM shortcuts handled internally (Mod+keys)
```

---

## Compositor: do we need to write a new one?

The existing `userland/compositor/src/lib.rs` uses `fill_rect_rounded` which calls
the `DisplayBackend::fill_rect` method — it draws **opaque** rectangles only.

For the surface-based model we need **alpha-blending** when compositing app content
over the background. We extend the compositor with:

1. `blend_surface(dst: &mut [u32], src: &[u32], alpha: u8)` — per-pixel alpha blend
2. `composite_surface(backend, surf_buf, x, y, w, h, alpha)` — blends a surface onto
   the destination buffer

This stays in userland. The kernel surface `blit()` remains opaque (memcpy),
and the WM composites into a merged buffer before calling ATTACH+COMMIT.

---

## Files written in this session

```
rogueos/
  docs/
    design-rogue-desktop-integration.md    ← this file
  kernel/
    arch/x86_64/sme.rs                     ← AMD SME enablement at boot
    arch/x86_64/debug_regs.rs              ← DR0-DR3 hardware breakpoints
    arch/x86_64/perf.rs                    ← AMD PMU perf counters
    process/scheduler/eevdf.rs               ← EEVDF scheduler (replaces round-robin)
    syscall/dispatcher/debug.rs            ← SYS_HW_BP_* syscall handlers
    syscall/dispatcher/perf.rs             ← SYS_PERF_* syscall handlers
  userland/
    src/bin/rogue_ds.rs                    ← Display server + compositor binary
    compositor/src/lib.rs                  ← Extended with alpha blending
    src/bin/wm.rs                          ← Migrated to SYS_SURFACE_* protocol
```
