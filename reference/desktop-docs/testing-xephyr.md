# Testing with Xephyr (host Xorg)

Xephyr is a nested X server: it runs as a window on your host X display and provides a separate X display (e.g. `:1`) where you can run a window manager without replacing your current session. This is the standard way to test roguewm or dwm-rs on top of host Xorg.

## Prerequisites

- **Host Xorg** running (e.g. you are logged into an X session, `DISPLAY=:0`).
- **Xephyr** installed (package name is often `xorg-server-xephyr` or `xephyr`).
- Built binaries: from repo root, `cargo build --release`.

## Quick run

From repo root:

```bash
chmod +x scripts/run-xephyr.sh
./scripts/run-xephyr.sh
```

This will:

1. Start Xephyr on display `:1` (size 1280x720 by default).
2. Run **roguewm** inside that display.
3. Optionally start an xterm so you can see a window.

A window will appear on your host desktop; that window is the Xephyr display. Inside it, roguewm is managing windows. To exit, close the Xephyr window or kill the Xephyr process.

## Run dwm-rs instead

```bash
./scripts/run-xephyr.sh dwm-rs
```

## Driving tests from another terminal

From a second terminal on the same machine:

```bash
export DISPLAY=:1
xterm
```

That xterm will open inside the Xephyr/roguewm session. You can run commands, open more windows, and test keybinds (e.g. Super+Shift+Q to quit roguewm).

## Environment variables

- **DISPLAY** — Must be set to your host X (e.g. `:0`) when you start the script so Xephyr can open its window.
- **DISPLAY_XEPHYR** — Display number for the nested server (default `:1`). The script exports this for the WM and any child processes.
- **SIZE** — Xephyr screen size (default `1280x720`).

## Automated sanity test

```bash
./scripts/test-wm-xephyr.sh
```

This starts Xephyr, runs roguewm with a timeout, optionally opens an xterm, and checks that the WM is still running. It is minimal and always exits (timeout + trap). Use it as a template for CI or more elaborate tests.

## Notes

- Xephyr runs as a **window on the host X server**. It is not a separate VT or login session.
- For production you run Xorg (or your display manager) and start roguewm via `~/.xinitrc` or a session script; Xephyr is only for testing.
- If “cannot open display” appears, ensure `DISPLAY` is set and that no other process is already using the chosen Xephyr display number.
