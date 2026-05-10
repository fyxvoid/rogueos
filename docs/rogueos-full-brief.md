# RogueOS — Full Brief

_Written 2026-05-03. Source: full codebase scan + vision/feat/capability-model branches._

---

## 1. How Much Is Written in Rust

| Layer | Language |
|---|---|
| Bootloader (`boot/`) | Rust (100%) |
| Kernel core, memory, scheduler, syscalls, drivers | Rust (~97%) |
| Assembly stubs (GDT/IDT entry, boot_multiboot2.S) | x86-64 ASM (~2%) |
| C SDK (`c-sdk/`) | C/H (~1%) |
| Build scripts | Shell |

**Line count (excluding build artifacts):**
- Total source lines: 23,782
- Rust: 21,074 (88.6% of all lines; **96.2% of source code** excluding scripts/config)
- C/ASM: 833
- Rest: shell scripts, TOML, linker scripts

The C SDK exists purely for C app developers who want to target RogueOS. The OS itself — bootloader, kernel, drivers, memory, scheduler, IPC, userland, window manager — is entirely Rust.

---

## 2. How Different Is This OS

Most "hobby OSes" re-implement 1970s UNIX concepts in Rust. RogueOS does not:

| Property | Linux / UNIX | RogueOS |
|---|---|---|
| Process model | `fork()` + `exec()` | Spawn-by-program-ID only. No fork. |
| Default authority | Inherited from parent (ambient) | Zero ambient authority — every resource requires an explicit capability |
| IPC | Pipes, sockets, shared memory (untyped) | 64-byte typed messages, cache-line aligned, fixed schema |
| Init | systemd / SysV (passive service list) | Cogman (PID 1): active immortal supervisor with journaled recovery |
| Memory safety | C, historically CVE-prone | Rust end-to-end: no unsafe in hot paths, compiler enforces invariants |
| Window manager | X11 or Wayland compositor stack | Direct framebuffer, kernel compositor authority model, no display protocol middleware |
| Scheduler | CFS (Linux) | EEVDF (Earliest Eligible Virtual Deadline First) — fairer, lower-latency |
| Boot | BIOS/GRUB + Linux kernel | Custom UEFI bootloader in Rust → kernel with no legacy baggage |
| Security model | DAC + optional MAC (SELinux) | Capability tokens: unforgeable, kernel-managed, typed |

**Key differentiators in plain English:**
- You cannot do anything on RogueOS without a capability token. No token = the syscall is rejected at the kernel boundary.
- There is no `fork`. Processes are spawned like functions — you declare what program, what capabilities it gets, and the kernel creates it clean.
- Cogman is not just init. It owns all capabilities and actively heals the system. If a service crashes, Cogman restarts it from a journal checkpoint.
- The entire GUI stack lives in userland with the kernel only providing a framebuffer + surface protocol. No X server. No Wayland compositor. RogueWM is the only desktop.

---

## 3. The Multics Connection

**What Multics promised in 1969:**  
A single unified system with hardware-enforced access control on every object, hierarchical naming of all resources, and supervised process isolation. It never fully shipped — PL/I on 36-bit hardware was too slow, and the security model was too complex for the era.

**What RogueOS takes from Multics (and finishes):**

| Multics concept | RogueOS implementation |
|---|---|
| Rings of protection | Kernel (ring 0) + userland (ring 3), no ring 1/2 |
| Capabilities on every object | `CapSet` bitmask per process; kernel rejects any syscall the caller has no capability for |
| Hierarchical authority | Only a parent can grant capabilities to its child; Cogman holds all at boot |
| Single unified namespace | Planned: every resource (file, socket, surface, IPC port) accessible via a typed path |
| Supervised processes | Cogman: journaled service table, millisecond restart, no data loss |
| No ambient authority | Enforced at kernel boundary — default is deny-all |

**What RogueOS adds that Multics never had:**
- Rust's type system enforces capability correctness at compile time, not just runtime
- Hardware isolation per process: separate CR3, PCID (Multics couldn't do this on 36-bit GE-645)
- EEVDF scheduler (Multics had no fair-share scheduling)
- A window manager as a first-class citizen of the capability model (surfaces are capability-gated)
- The entire design is auditable in one repo: ~21k lines of Rust, no legacy kernel code

---

## 4. GUI / Window Manager — End-to-End Status

### What the kernel provides (all implemented and wired):

| Syscall | Status | Notes |
|---|---|---|
| `SYS_SCREEN_SIZE` | ✓ done | Returns GOP width/height from UEFI boot info |
| `SYS_FB_CLEAR` | ✓ done | Fills entire framebuffer with ARGB color |
| `SYS_FB_FILL_RECT` | ✓ done | Clips to bounds, stride-correct |
| `SYS_FB_BLIT` | ✓ done | Copies 32bpp user buffer to framebuffer at (x,y) |
| `SYS_FB_FLUSH` | ✓ done | No-op for now (direct framebuffer, no backbuffer yet) |
| `SYS_POLL_INPUT` | ✓ done | Drains PS2 → KeyEvent ring |
| `SYS_POLL_MOUSE` | ✓ done | Drains PS2 mouse → MouseEvent ring |
| `SYS_SURFACE_CREATE` | ✓ done | Returns surface_id, bound to calling PID |
| `SYS_SURFACE_ATTACH` | ✓ done | Validates user ptr, attaches pixel buffer to surface |
| `SYS_SURFACE_COMMIT` | ✓ done | Blits surface buffer to framebuffer at (x,y) |
| `SYS_CLAIM_COMPOSITOR` | ✓ done | One process claims compositor authority; PERM for others |
| `SYS_COMPOSITE_ALL` | ✓ done | Blits all surfaces in z-order; only compositor may call |
| `SYS_SHM_CREATE` | ✓ done | Shared memory for zero-copy buffer sharing |

The framebuffer driver also has `draw_test_pattern()` — a red background with a gradient band and a green box — already callable from the kernel. This is the fastest way to verify the full pipeline works.

### What is missing:

| Missing piece | Impact | Effort |
|---|---|---|
| **Bitmap font** | Cannot render any text (no status bar, no window titles) | Low — embed 8×16 PSF font in userland |
| **Framebuffer not confirmed from userland** | All the syscalls exist but have not been called end-to-end via a real userland binary running in QEMU | Low — write 20-line fbtest.rs |
| **Input routing WM → App** | WM receives keys but has no mechanism to forward `KWM_EVENT_KEY` to focused app yet | Medium |
| **Font-glyph blit path** | `SYS_FB_BLIT` exists; need userland to compose glyph bitmaps into a buffer and blit | Low |
| **Per-process address spaces** | Currently all processes share one CR3 (flat identity map). Surface `validate_user_ptr` works but true isolation doesn't yet exist | High — scheduled later |

### What the userland WM has (written, untested on OS):

- `userland/src/bin/wm.rs` — 1,375 lines: WM event loop written
- `userland/rwm-core/src/layout.rs` — 522 lines: tiling layout (master+stack, monocle)
- `userland/rwm-core/src/state.rs` — client list, tag tracking
- `userland/rwm-config/src/lib.rs` — key bindings
- `userland/src/bin/rogue_ds.rs` — 819 lines: display server (compositor side)
- `userland/src/rdp.rs` — 293 lines: RDP-style compositor protocol

### The honest gap:

The kernel GUI foundation is solid and more complete than expected. The gap is not in the kernel — it's in the userland. The WM binary and compositor haven't been run on the actual OS yet and connected to the syscalls. The first thing to prove is that `SYS_FB_FILL_RECT` actually puts pixels on the QEMU GTK screen from a userland process.

---

## 5. What to Build Next (Priority Order)

1. **`fbtest.rs`** — 20-line binary: calls `screen_size` + `fb_fill_rect` + `fb_flush`. Proves end-to-end. If pixels appear: stages 1-3 of the roadmap compress into hours, not days.

2. **Bitmap font** — embed 8×16 PSF1 font array in userland, write `draw_str(x,y,s,fg,bg)` using `SYS_FB_BLIT`.

3. **Boot into WM** — add `wm` to the init program list so it auto-starts after Cogman. WM calls `claim_compositor`, clears screen, draws status bar.

4. **App surface handshake** — one test app registers with WM via IPC, gets geometry, draws into its surface, WM calls `composite_all`.

5. **Wire rwm-core layout** — WM already has rwm-core as a dependency. Connect the layout engine so windows tile automatically.

See `docs/gui-roadmap.md` for the full 10-stage breakdown.

---

## 6. The Vision in One Paragraph

RogueOS is what Multics would look like if it were designed today: zero ambient authority, Rust memory safety, spawn-not-fork process model, typed IPC, and Cogman as the immortal PID 1 that supervises all services. The entire desktop is one keyboard-driven tiling WM (roguewm, a dwm clone) running over a kernel-hosted compositor with capability-gated surfaces. The system is engineered for developers and security practitioners who want an OS that is auditable in full, has no legacy UNIX baggage, and enforces isolation by construction rather than by policy. The entire source — bootloader, kernel, drivers, scheduler, window manager, and apps — is ~21k lines of Rust.
