# Trash — moved / removed userland

Items here were removed from the active build to align with the in-kernel compositor and drop X11-specific host code.

- **rogue-compositor** — Host-only Wayland launcher (exec anvil). Compositor logic lives in `kernel/src/compositor` (tile_layout) and `kernel/src/display_server`; this crate was redundant for the OS build.
- **rwm-bar** — X11 status bar; depended on rwm-x11. Removed with rwm-x11.
- **rwm-x11** — Not present in tree (referenced only in build script and rwm-bar). Removed from `build/build_os.sh`.

Restore by moving crates back under `userland/` and re-adding them to the desktop build list in `build/build_os.sh` if needed.
