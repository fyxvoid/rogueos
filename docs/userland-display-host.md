# Userland display stack (unified userland)

This document describes the **unified userland** (single program: server + compositor + WM + utils), how to run and test it, and how it relates to the RogueOS kernel.

## Unified userland layout

| Crate / binary | Role |
|----------------|------|
| **userland/core** | Config (transparency, corner_radius, shortcuts), `DisplayBackend` trait. |
| **userland/server** | Display server abstraction; holds backend, exposes `commit` and `backend_mut`. |
| **userland/compositor** | Compositor: transparency and rounded corners (config-driven, shortcut-adjustable). |
| **userland/wm** | Window list, focus, shortcut → action mapping. |
| **userland/utils** | Screenshot, lock, clipboard (stubs on RogueOS; logic can be added for host). |
| **session** binary | Main binary: runs server + compositor + WM in one process; handles input and shortcuts. |
| **init** binary | Spawns only the **session** binary (program_id 1). |

On **RogueOS** (target `x86_64-unknown-none`), the session uses the **kernel backend** (`sys_fb_*`). On host, a future **host backend** (X11 or headless) would allow the same code to run without the kernel.

## Roles: kernel vs userland

| Component      | Kernel | Userland |
|----------------|--------|----------|
| Display server | **Director** (manages surfaces/buffers). | **server** crate: owns `DisplayBackend`; on RogueOS uses `sys_fb_*`. |
| Compositor     | **Painter** (kernel compositor). | **compositor** crate: transparency, rounded corners. |
| WM             | — | **wm** crate: focus, shortcuts. |
| Utils          | — | **utils** crate: screenshot, lock, clipboard (stubs on RogueOS). |

## How to run and test

### Userland tests (no kernel)

From the repo root:

```bash
./scripts/run_display_stack.sh
```

This runs `cargo test -p userland-core -p userland-compositor` (config clamp, composite with headless backend).

### All Rust tests

```bash
./scripts/test_all_rust.sh
```

Runs tests for libs, userland-core, userland-compositor.

### Build session and init (RogueOS target)

```bash
cargo build -p userland --target x86_64-unknown-none --release --bin session --bin init
```

Binaries are in `target/x86_64-unknown-none/release/` (session, init, wm, shell, etc.). The kernel build embeds `session.elf` and registers it as program_id 1; init spawns only that binary.

### Config

Single config (default in code): transparency (default/min/max), corner_radius (default/min/max), shortcuts. Adjustable only via fixed shortcuts (no user scripts).

## Integration with kernel

See [Design: display stack evolution](design-display-evolution.md). In short:

- **Today**: init spawns the **session** binary (program id 1). Session uses `sys_fb_*` and `sys_poll_input`.
- **Later**: Kernel may expose surface syscalls; the userland server backend would call those instead of (or in addition to) `sys_fb_*`.

Host crates (dwm-rs, rwm-core, rwm-config, rogue-shot, rogue-lock, rogue-clip) have been **removed from the workspace** as part of the unified userland migration. Their logic can be re-integrated behind a host backend or feature flags if needed.
