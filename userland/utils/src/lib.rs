//! Utils: screenshot, lock, clipboard. Invoked by WM shortcuts or session subcommands.
//! On RogueOS (no_std): stubs (no framebuffer read, no display lock, no clipboard yet).

#![no_std]

/// Screenshot: capture current framebuffer. On RogueOS no-op (kernel has no read-back yet).
pub fn screenshot() {
    // TODO: when kernel exposes framebuffer read or export, write to file.
}

/// Lock: lock display / screensaver. On RogueOS no-op.
pub fn lock() {
    // TODO: when we have lock screen surface.
}

/// Clipboard paste: paste from clipboard. On RogueOS no-op.
pub fn clipboard_paste() {
    // TODO: when we have clipboard IPC.
}
