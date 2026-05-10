# RogueOS GUI Roadmap — dwm-style Window Manager

Goal: a keyboard-driven tiling WM (dwm-style: master+stack, tags, status bar)
running natively on RogueOS without X11 or Wayland.

---

## Stage 1 — Framebuffer Pixel Output  ← START HERE

**Goal:** a userland binary draws a colored rectangle and it appears in the QEMU GTK window.

What to do:
- Verify `SYS_SCREEN_SIZE` returns correct width/height from UEFI framebuffer info
- Verify `SYS_FB_FILL_RECT` writes ARGB pixels to the linear framebuffer
- Verify `SYS_FB_FLUSH` (or confirm framebuffer is directly mapped, no flush needed)
- Write a one-file test binary (`userland/src/bin/fbtest.rs`) that fills the screen red, then draws a white box

Files to touch:
- `kernel/drivers/framebuffer.rs` — confirm framebuffer base addr is correctly mapped from UEFI GOP
- `kernel/syscall/dispatcher/gfx.rs` — confirm FB_FILL_RECT and SCREEN_SIZE dispatch
- `userland/src/bin/fbtest.rs` — new test binary

Done when: QEMU GTK window shows colored pixels.

---

## Stage 2 — Keyboard Input Pipeline

**Goal:** a userland binary prints keycodes it receives to serial.

What to do:
- Confirm PS2 IRQ1 handler stores scancodes into a kernel ring buffer
- Confirm `SYS_POLL_INPUT` drains that buffer into a `KeyEvent` struct
- Write test binary (`userland/src/bin/inputtest.rs`) that calls `poll_input` in a loop and writes keycode to serial

Files to touch:
- `kernel/arch/x86_64/ps2.rs` — verify IRQ handler stores into shared ring
- `kernel/syscall/dispatcher/gfx.rs` — verify POLL_INPUT reads from ring
- `userland/src/bin/inputtest.rs` — new test binary

Done when: pressing a key in QEMU produces a scancode on serial output.

---

## Stage 3 — Bitmap Font / Text Rendering

**Goal:** render ASCII text to the framebuffer from userland.

What to do:
- Embed an 8×16 bitmap font (PSF1 or raw bitmask array) in the userland crate
- Write `draw_char(x, y, ch, fg, bg)` using `SYS_FB_FILL_RECT` or direct pixel writes
- Write `draw_str(x, y, s, fg, bg)`
- Test: draw "RogueOS" on screen at boot

Where to put it:
- `userland/src/lib.rs` or a new `userland/src/font.rs`
- Use the existing `SYS_FB_BLIT` syscall to blit a glyph row as a pixel strip

Done when: text appears in the QEMU window.

---

## Stage 4 — Minimal WM Loop (No Apps Yet)

**Goal:** a process that owns the framebuffer, draws a status bar, and responds to keyboard shortcuts.

What to do:
- Write/complete `userland/src/bin/wm.rs` main loop:
  1. Get screen size
  2. Clear screen (dark background)
  3. Draw status bar at top (solid color + "RogueOS" text)
  4. Loop: `poll_input`, handle global shortcuts (quit, etc.)
- No windows yet — just prove the WM owns the screen and responds to input

Uses:
- Stage 1 (framebuffer)
- Stage 2 (input)
- Stage 3 (text)

Done when: WM starts, shows a status bar, pressing Q exits cleanly.

---

## Stage 5 — App Surface Protocol

**Goal:** a second process can register with the WM and submit pixel buffers.

What to do:
- Define the IPC handshake:
  1. App sends `KWM_REGISTER` to WM pid=1
  2. WM sends `KWM_GEOMETRY` back (x, y, w, h)
  3. App calls `SYS_SURFACE_CREATE` → gets surface_id
  4. App fills its buffer, calls `SYS_SURFACE_ATTACH` + `SYS_SURFACE_COMMIT`
  5. WM receives commit notification, blits app's buffer to its screen region
- Implement `SYS_SURFACE_*` syscalls end-to-end in kernel (currently dispatcher stubs)

Files to touch:
- `kernel/syscall/dispatcher/gfx.rs` — implement SURFACE_CREATE/ATTACH/COMMIT
- `kernel/display/compositor/mod.rs` — hold surface registry, blit on commit
- `lib/src/lib.rs` — KwmMsg already defined, verify layout matches
- `userland/src/bin/wm.rs` — handle REGISTER, send GEOMETRY, blit on COMMIT

Done when: a hello-world app draws "Hello" in its assigned window area.

---

## Stage 6 — Tiling Layout

**Goal:** WM tiles multiple app windows using dwm master+stack algorithm.

What to do:
- Wire `rwm-core`'s layout engine (`userland/rwm-core/src/layout.rs`) into the WM binary
- WM maintains a client list; on new app REGISTER, recompute tile layout and send updated GEOMETRY to all clients
- Implement monocle (fullscreen) and master+stack layouts
- Keyboard shortcuts: `Mod+Enter` (spawn terminal), `Mod+J/K` (focus next/prev), `Mod+D` (monocle toggle)

Files to touch:
- `userland/src/bin/wm.rs` — integrate rwm-core layout, handle keyboard shortcuts
- `userland/rwm-core/src/layout.rs` — already written, just wire it in
- `userland/rwm-config/src/lib.rs` — key binding config

Done when: two apps tile side by side; focus switches with keyboard.

---

## Stage 7 — Status Bar

**Goal:** status bar shows tags, focused window title, and a clock.

What to do:
- Add tag indicators (1–9) with active/occupied/urgent highlight
- Show focused app name (from KWM_SET_TITLE message)
- Show time (need `SYS_GETTIME` syscall or read from CMOS)
- Refresh bar on each event loop tick

Files to touch:
- `kernel/syscall/dispatcher/misc.rs` — add SYS_GETTIME if not present
- `userland/src/bin/wm.rs` — bar rendering

Done when: bar shows correct tags and time.

---

## Stage 8 — Tags (Virtual Desktops)

**Goal:** apps can be assigned to tags; only the current tag's apps are visible.

What to do:
- Each client has a tag bitmask (default: tag 1)
- `Mod+1..9` switches active tag; `Mod+Shift+1..9` moves focused client to tag
- WM sends `KWM_GEOMETRY` with w=h=0 to hidden clients (they stop drawing)
- Bring back when tag is selected

Files to touch:
- `userland/rwm-core/src/state.rs` — tag tracking already stubbed
- `userland/src/bin/wm.rs` — handle tag key bindings

Done when: apps disappear and reappear by tag key.

---

## Stage 9 — Floating Windows

**Goal:** individual windows can be toggled to floating (drag/resize with mouse).

What to do:
- Confirm `SYS_POLL_MOUSE` works (Stage 2 follow-up for mouse)
- In floating mode, WM tracks drag offset and sends new GEOMETRY on mouse move
- `Mod+Shift+Space` toggles floating for focused client

Done when: a window can be dragged around the screen.

---

## Stage 10 — First Real App: Terminal

**Goal:** a usable terminal emulator running under the WM.

What to do:
- Write `userland/src/bin/terminal.rs`:
  - Registers with WM, gets surface
  - Draws a black background + blinking cursor
  - Reads keyboard events forwarded by WM (`KWM_EVENT_KEY`)
  - Runs the `shell` binary as a child process, pipes I/O
- Shell output rendered as text grid using Stage 3 font

This is the capstone — when terminal works, the OS is actually usable.

---

## What NOT to Build Yet

- Mouse cursor rendering (keyboard-driven WM, skip for now)
- Window decorations / titlebars (dwm has none)
- Hardware GPU acceleration
- Network stack
- Audio
- File manager

---

## Implementation Order Summary

```
1. Framebuffer pixels work from userland         [~1 day]
2. Keyboard input works from userland            [~1 day]
3. Bitmap font renders text to screen            [~1 day]
4. Minimal WM loop owns screen + status bar      [~2 days]
5. App surface protocol (IPC + blit)             [~3 days]
6. Tiling layout (rwm-core wired in)             [~2 days]
7. Status bar (tags + clock)                     [~1 day]
8. Tags / virtual desktops                       [~1 day]
9. Floating windows (mouse)                      [~2 days]
10. Terminal emulator                            [~3 days]
```

Total estimated: ~17 focused dev days to a usable dwm-like desktop.
