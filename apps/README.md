# apps/

RogueOS-native graphical applications. Each crate is part of the Cargo workspace
and built for the `x86_64-unknown-none` bare-metal target like the rest of userland.

| Crate | Description |
|-------|-------------|
| `rogue-clip/` | Clipboard manager — stores and retrieves cut/copied content via IPC |
| `rogue-lock/` | Screen lock — blanks the framebuffer and requires a PIN to unlock |
| `rogue-shot/` | Screenshot utility — captures framebuffer regions to the VFS |

These apps use the RDP protocol (`userland/src/rdp.rs`) to render into their own pixel
buffers and have the `rogue_wm` compositor blit them to the screen.
