# reference/

Design and implementation references for RogueOS — **not built as part of the OS**.

| Directory | What it is |
|-----------|-----------|
| `dwm-rs/` | RogueOS-native Rust port of dwm (no_std, bare-metal); precursor to `userland/src/bin/wm.rs` |
| `dwm-src/` | Original C dwm source kept as algorithmic reference |
| `dwm-rs-desktop/` | X11/std Rust dwm port from rogue-desktop (Linux target); reference for layout algorithms |
| `dwm-src-c/` | C dwm source from rogue-desktop |
| `desktop-docs/` | X11/Wayland build notes ported from rogue-desktop |

These directories are excluded from the Cargo workspace and are read-only design references.
The actual RogueOS WM lives in `userland/src/bin/wm.rs` and uses the RDP protocol
(`userland/src/rdp.rs`) for secure, compositor-enforced window management.
