### Design: Display Stack Evolution

This document proposes incremental improvements to the display stack to better match modern compositor/WM expectations.

#### Phase: Integrate userland WM with kernel surface API

- **Context:** The host display stack (X11 + dwm-rs + rogue utils) is runnable and tested via `scripts/run_display_stack.sh`; see [userland-display-host.md](userland-display-host.md). Integration with the kernel is the next step.
- **Init/WM spawn (unchanged until kernel is ready):** [userland/src/bin/init.rs](../userland/src/bin/init.rs) spawns Director (WM binary, program id 1) then Throne (shell, program id 0). No change to spawn order; the WM continues to use `sys_fb_*` until surface syscalls exist.
- **Kernel work (post–host stack stable):** Add the three syscalls (`SYS_DISPLAY_CONNECT`, `SYS_DISPLAY_ATTACH`, `SYS_DISPLAY_COMMIT`) in the decrees dispatcher, proxying to [kernel/display/display_server](../kernel/display/display_server/mod.rs) (`client_connect`, `buffer_attach`, `commit`).
- **Userland WM refactor:** Once the syscalls are in place, refactor [userland/src/bin/wm.rs](../userland/src/bin/wm.rs) to allocate per-window buffers, render into them, and use attach/commit instead of raw `sys_fb_fill_rect`. Until then, keep using `sys_fb_*` so the current kernel still runs the same WM.

#### Stage 1 — Surface protocol for WM

- **Goal:** Stop drawing directly to the framebuffer from WM; instead, have WM manage buffers/surfaces via a kernel protocol.
- **Steps:**
  - Expose a small set of syscalls that proxy to `display::display_server`:
    - `SYS_DISPLAY_CONNECT` → returns a surface ID for the WM.
    - `SYS_DISPLAY_ATTACH(surface_id, buf_ptr, w, h, stride)` → calls `buffer_attach`.
    - `SYS_DISPLAY_COMMIT(surface_id, dst_x, dst_y)` → calls `commit`.
  - Update WM:
    - Allocate a backing buffer in user-space for each window.
    - Render into those buffers, then attach/commit via the new syscalls.
  - Keep the current “single WM” model; no need for multiple clients yet.

#### Stage 2 — Multiple client surfaces

- **Goal:** Allow multiple user processes to own surfaces (e.g. separate apps).
- **Steps:**
  - Extend `SurfaceState` to track an owning PID.
  - Allow multiple `client_connect` calls, one per PID, up to `MAX_SURFACES`.
  - Enforce that only the owning process can attach/commit to its surface via PID checks in the syscalls.
  - Let WM act as a special client that composes other surfaces (e.g. simple tiling/windowing logic).

#### Stage 3 — Damage tracking and partial redraw

- **Goal:** Avoid redrawing the entire screen for every change.
- **Steps:**
  - Add an internal damage region list to the display server.
  - On each `commit(surface, dst_x, dst_y)`, add the affected rectangle to the damage list.
  - Add an internal `repaint()` function that:
    - Walks damage regions and re-blits only those areas from attached buffers to the framebuffer.
    - Coalesces overlapping damage rectangles.
  - WM can trigger `repaint()` explicitly via a syscall, or the kernel can trigger it on a timer.

#### Stage 4 — Future directions

- **Vsync-friendly rendering**
  - Integrate simple timing awareness (e.g. repaint at a fixed refresh cadence) if the platform exposes it.
- **Backing buffers and double-buffering**
  - Allow each surface to maintain front/back buffers and swap on commit.
  - Introduce a “pending” vs “committed” buffer state in `SurfaceState`.
- **Wayland-like model (long-term)**
  - Move more policy (window placement, focus, decorations) fully into userland WM.
  - Keep the kernel’s role limited to:
    - Surface/buffer management.
    - Input delivery.
    - Framebuffer access enforcement and clipping.

