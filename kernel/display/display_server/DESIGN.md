# Display Server Design

## Purpose

Minimal display server: one compositor client, surface abstraction, buffer attach and commit. Renders directly to the kernel-mapped framebuffer. Full repaint only; no damage tracking. Model is original; no Wayland, X11, or external display stack.

## Design

- **Surface**: Logical window identified by id; state holds optional buffer (ptr, width, height, stride).
- **Client connect**: Single client (compositor); receives a Surface.
- **Buffer attach / commit**: Client attaches a buffer to a surface; commit blits to framebuffer at (dst_x, dst_y) and flushes. No reference to external DRM/KMS or protocol design.
