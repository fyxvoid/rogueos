### Display Stack: Framebuffer, Display Server, Compositor, WM

This document describes the graphics pipeline from kernel framebuffer to userland WM.

#### Framebuffer driver

- Module: `[kernel/drivers/framebuffer.rs]`.
- Initialization:
  - BootInfo provides GOP framebuffer parameters (base, size, width, height, stride, bpp).
  - `init_from_boot_info` records these and exposes them as a singleton `info() -> Option<FbInfo>`.
- Rendering helpers:
  - `clear(color: u32)` fills the entire framebuffer with a solid color (X8R8G8B8).
  - `fill_rect(x, y, w, h, color)` clips to screen bounds and writes pixel data row by row.
  - `blit(dst_x, dst_y, w, h, stride, src_ptr)` copies from a user-provided ARGB buffer into the framebuffer with clipping.
  - `flush()` is currently a no-op; drawing is immediate.
- Test pattern:
  - `draw_test_pattern()`:
    - Safely returns early if framebuffer info is missing or invalid.
    - Draws a red background, gradient band, and a green rectangle to verify that framebuffer mapping is correct.

#### Kernel display server (Director)

- Module: `[kernel/display/display_server/mod.rs]`.
- Responsibilities:
  - Maintains a small fixed set of “surfaces” (`MAX_SURFACES = 8`) on top of the framebuffer.
  - Tracks **one client** (the compositor/WM) via `CLIENT_CONNECTED`.
- API (kernel-internal, not syscall-based yet):
  - `client_connect() -> Option<Surface>`:
    - Returns a new `Surface` handle (`id: u32`) if no client is connected; otherwise returns `None`.
  - `buffer_attach(surface, ptr, width, height, stride) -> bool`:
    - Validates dimensions and stride; stores `(ptr, width, height, stride)` in the surface state.
  - `commit(surface, dst_x, dst_y) -> bool`:
    - Locates the surface, validates buffer and non-null pointer, and then:
      - Calls `framebuffer::blit(dst_x, dst_y, w, h, stride, ptr)`.
      - Calls `framebuffer::flush()`.
    - Returns `true` on success, `false` otherwise.
  - `screen_size() -> (u32, u32)`:
    - Returns the fixed logical screen size derived from `FB_WIDTH`/`FB_HEIGHT`.

At present, only kernel code uses this API; userland WM is drawing directly via syscalls (no surface protocol).

#### Userland WM

- Module: `[userland/src/bin/wm.rs]`.
- Model:
  - A toy, **single-process window manager** that:
    - Maintains three fixed windows (`WINDOWS` array) with positions and sizes.
    - Owns background and window drawing entirely in userland.
  - Startup:
    - Entry point `_start` logs `[WM] started`, draws an initial scene, and enters an input loop.
- Rendering:
  - `draw_scene(focused)`:
    - Clears the framebuffer via `sys_fb_clear`.
    - Draws every window using `draw_window`, with colors depending on whether the window is focused.
    - Flushes via `sys_fb_flush`.
  - `draw_window`:
    - Uses `sys_fb_fill_rect` to draw the window body and a simple 2px border.
- Input:
  - Uses `sys_poll_input` (key events) in a loop.
  - Moves focus left/right across the window array and redraws on ENTER/ESC.
  - Logs periodic “tick” messages for liveness.

#### Syscall interface between WM and kernel

- Syscalls (from `[userland/src/lib.rs]` / `[lib/src/syscall_consts.rs]`):
  - `SYS_FB_CLEAR` → `sys_fb_clear(color)`.
  - `SYS_FB_FILL_RECT` → `sys_fb_fill_rect(x, y, w, h, color)`.
  - `SYS_FB_FLUSH` → `sys_fb_flush()`.
  - `SYS_POLL_INPUT` → `sys_poll_input(KeyEvent)`.
- In the kernel:
  - `syscall::dispatcher` implements these syscalls by calling into `drivers::framebuffer` and `drivers::input`.
  - There is no explicit syscall yet for “surface attach/commit”; the WM interacts directly with the framebuffer.

#### Comparison vs modern display/compositor stacks

Current model:

- **Single global framebuffer**:
  - WM draws directly into the framebuffer via syscalls, with no intermediate surfaces or compositing protocol.
- **Minimal kernel display server**:
  - Director’s `display_server` module offers a surface abstraction, but it is **not yet exposed to userland**.
  - No damage tracking, vsync awareness, or double-buffering at the kernel API level.
- **Single compositor/WM**:
  - Only one WM process, hard-wired via init; there is no client registry or per-app surfaces.

Gaps vs modern expectations:

- No explicit kernel/user protocol for surfaces and buffers (no Wayland/X11-style separation).
- No per-client surfaces; WM and apps are effectively the same from the kernel’s perspective.
- No explicit composition step separate from drawing (WM does both policy and rendering).

These gaps and potential evolution steps (surface protocol between userland WM and kernel, damage tracking, layering of multiple clients) are outlined in `design-display-evolution.md`.

