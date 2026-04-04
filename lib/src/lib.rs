#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), deny(warnings))]

//! Shared kernel/userland ABI: syscall numbers, error codes, and simple
//! structs/enums used across the bootloader <-> kernel <-> userland boundary.
//!
//! Syscall numbering is an original namespace (I/O 0x100, graphics 0x200, process 0x300).

/// I/O and file syscalls (namespace 0x100).
pub const SYS_READ: u64 = 0x100;
pub const SYS_WRITE: u64 = 0x101;
pub const SYS_OPEN: u64 = 0x102;
pub const SYS_CLOSE: u64 = 0x103;
pub const SYS_LSEEK: u64 = 0x104;
pub const SYS_UNLINK: u64 = 0x105;
pub const SYS_FSYNC: u64 = 0x106;
/// List root directory: buf, capacity. Returns bytes written or negative error.
pub const SYS_LIST_ROOT: u64 = 0x107;
/// Reboot/halt. Arg: 0=halt, 1=reboot. Returns negative error on failure.
pub const SYS_REBOOT: u64 = 0x108;
/// Exit current process. Arg: status. Never returns.
pub const SYS_EXIT: u64 = 0x109;

/// Graphics and input syscalls (namespace 0x200).
pub const SYS_POLL_INPUT: u64 = 0x200;
pub const SYS_FB_CLEAR: u64 = 0x201;
pub const SYS_FB_FILL_RECT: u64 = 0x202;
pub const SYS_FB_FLUSH: u64 = 0x203;
pub const SYS_POLL_MOUSE: u64 = 0x204;
/// Surface protocol (display server, namespace 0x210-0x21F).
/// Create a new display surface. Returns surface_id (u32) or negative error.
pub const SYS_SURFACE_CREATE: u64 = 0x210;
/// Destroy a surface. Arg: surface_id. Returns 0 or negative error.
pub const SYS_SURFACE_DESTROY: u64 = 0x211;
/// Attach a pixel buffer to a surface. Args: id, ptr, width, height, stride(bytes). Returns 0 or negative.
pub const SYS_SURFACE_ATTACH: u64 = 0x212;
/// Commit (blit) surface at (dst_x, dst_y). Args: id, dst_x, dst_y. Returns 0 or negative.
pub const SYS_SURFACE_COMMIT: u64 = 0x213;
/// Get screen size. Args: out_w(*mut u32), out_h(*mut u32). Returns 0 or negative.
pub const SYS_SCREEN_SIZE: u64 = 0x214;
/// Blit raw 32bpp buffer to framebuffer. Args: dst_x, dst_y, w, h, stride, ptr. Returns 0 or negative.
pub const SYS_FB_BLIT: u64 = 0x215;
/// Claim compositor role (display authority). First caller wins; all surface_commit calls
/// from non-compositor processes are rejected. Returns 0 on success, -EPERM if already claimed.
pub const SYS_CLAIM_COMPOSITOR: u64 = 0x216;
/// Composite all surfaces in z-order and flush to hardware. Only the registered compositor may call.
/// Returns 0 on success, -EPERM if caller is not the compositor.
pub const SYS_COMPOSITE_ALL: u64 = 0x217;
/// Get the PID of the registered compositor. Returns pid on success, -ENOENT if none registered.
pub const SYS_GET_COMPOSITOR_PID: u64 = 0x218;

/// IPC syscalls (namespace 0x320).
/// Send a KwmMsg to target process. Args: target_pid (u32), msg_ptr (*const KwmMsg), flags (u32).
/// Returns 0 on success, SYSERR_NOMEM if target queue full, SYSERR_NOENT if no such pid.
pub const SYS_IPC_SEND: u64 = 0x320;
/// Receive next KwmMsg for this process. Args: out_ptr (*mut KwmMsg), flags (u32).
/// Returns 0 on success, SYSERR_AGAIN if queue empty (non-blocking), blocks if IPC_NONBLOCK not set.
pub const SYS_IPC_RECV: u64 = 0x321;

/// Debug / hardware breakpoint syscalls (namespace 0x400).
/// Set hardware breakpoint. Args: slot(u64 0-3), addr(u64), cond(u64 0-3), len(u64 0-3).
/// cond: 0=execute, 1=write, 2=io_rw, 3=read_write. len: 0=1B,1=2B,2=8B,3=4B.
/// Returns 0 on success, negative error on failure.
pub const SYS_HW_BP_SET: u64 = 0x400;
/// Clear hardware breakpoint. Args: slot(u64, 0xFF=clear all). Returns 0 on success.
pub const SYS_HW_BP_CLEAR: u64 = 0x401;
/// Query hardware breakpoint state. Args: out_ptr(*mut HwBpInfo, 64 bytes). Returns 0 on success.
pub const SYS_HW_BP_QUERY: u64 = 0x402;

/// Performance counter telemetry syscalls (namespace 0x410).
/// Open a perf counter. Args: event_id(u64). Returns handle(u64) on success, negative on error.
/// Events: 0=cycles, 1=instructions, 2=L1d-access, 3=L1d-miss, 4=L2-access,
///         5=L2-miss, 6=branches, 7=branch-mispr, 8=icache-miss, 9=stall-cycles.
pub const SYS_PERF_OPEN: u64 = 0x410;
/// Read a perf counter. Args: handle(u64), out_ptr(*mut u64). Returns 0 on success.
pub const SYS_PERF_READ: u64 = 0x411;
/// Close a perf counter. Args: handle(u64). Returns 0 on success.
pub const SYS_PERF_CLOSE: u64 = 0x412;

/// Scheduler control syscalls (namespace 0x420).
/// Set nice level for current process. Args: nice(i64, -20..+19). Returns 0 on success.
pub const SYS_SET_NICE: u64 = 0x420;

/// Flag for SYS_IPC_RECV: return SYSERR_AGAIN immediately instead of blocking.
pub const IPC_NONBLOCK: u32 = 0x01;

/// Flag for SYS_WAITPID options: non-blocking (return immediately if no dead process).
pub const WNOHANG: u32 = 0x01;

/// Well-known PID for the cogman supervisor (first userland process spawned by kernel).
/// All Cog* IPC control messages are sent to this PID.
pub const COGMAN_PID: u32 = 1;

/// No message ready (non-blocking recv on empty queue).
pub const SYSERR_AGAIN: i64 = -11;

/// KWM IPC message type byte.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KwmType {
    // App → WM
    /// App introduces itself to the WM; carries PayloadRegister.
    Register      = 0x01,
    /// App is exiting; WM should remove its client entry.
    Unregister    = 0x02,
    /// App wants to update its window title; carries PayloadSetTitle.
    SetTitle      = 0x03,
    /// App's pixel buffer is ready; carries PayloadSurfaceCommit.
    SurfaceCommit = 0x04,
    // WM → App
    /// WM tells app its on-screen geometry; carries PayloadGeometry.
    Geometry      = 0x10,
    /// Key event forwarded to the focused app; carries PayloadEventKey.
    EventKey      = 0x11,
    /// Mouse event forwarded; carries PayloadEventMouse.
    EventMouse    = 0x12,
    /// Focus gained (focused=1) or lost (focused=0); carries PayloadEventFocus.
    EventFocus    = 0x13,
    /// WM has resized the window; carries PayloadEventResize.
    EventResize   = 0x14,
    // Bidirectional
    /// Generic ACK; seq mirrors the acknowledged message's seq.
    Ack           = 0x20,
    /// Keep-alive / echo request.
    Ping          = 0x21,
    // Display Server → App
    /// DS assigns a kernel surface_id to this app; carries PayloadSurfaceAssign.
    SurfaceAssign = 0x30,
    // Cogman control (namespace 0x40–0x4F)
    /// Query the full service list; no payload.
    CogList       = 0x40,
    /// Start a service; payload.cog_ctrl.program_id names the target.
    CogStart      = 0x41,
    /// Stop a running service; payload.cog_ctrl.program_id names the target.
    CogStop       = 0x42,
    /// Query status of one service; payload.cog_ctrl.program_id names the target.
    CogStatus     = 0x43,
    /// Cogman response to any Cog* request; carries PayloadCogCtrl.
    CogResp       = 0x44,
    /// Restart a running or stopped service.
    CogRestart    = 0x45,
    // KDP (Kingdom Display Protocol) — secure compositor/window protocol (0x50–0x5F)
    /// Client → compositor: request a window. Carries PayloadKdp (surface_id, flags, title).
    KdpConnect    = 0x50,
    /// Compositor → client: window assigned. Carries PayloadKdp (surface_id, x, y, width, height).
    KdpGrant      = 0x51,
    /// Client → compositor: pixel buffer updated. Carries PayloadKdp (surface_id).
    KdpCommit     = 0x52,
    /// Compositor → client: please resize. Carries PayloadKdp (surface_id, width, height).
    KdpResize     = 0x53,
    /// Compositor → client: key event. Carries PayloadKdp (key_code, key_state).
    KdpKey        = 0x54,
    /// Compositor → client: focus changed. Carries PayloadKdp (flags: 1=gained, 0=lost).
    KdpFocus      = 0x55,
    /// Compositor → client: please close gracefully. Carries PayloadKdp (surface_id).
    KdpClose      = 0x56,
    /// Client → compositor: window is closing. Carries PayloadKdp (surface_id).
    KdpDisconnect = 0x57,
}

// ── KWM payload structs (each exactly 56 bytes) ───────────────────────────

/// App registration: announce pid + initial title + flags.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PayloadRegister {
    pub title: [u8; 48],
    pub flags: u32,
    pub _pad:  [u8; 4],
}

/// Update window title string (NUL-terminated, up to 55 chars + NUL).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PayloadSetTitle {
    pub title: [u8; 56],
}

/// App commits a surface: surface id, position and size it is rendering at.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PayloadSurfaceCommit {
    pub surface_id: u32,
    pub x:          i32,
    pub y:          i32,
    pub w:          u32,
    pub h:          u32,
    pub _pad:       [u8; 36],
}

/// WM informs app of its current geometry.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PayloadGeometry {
    pub x:    i32,
    pub y:    i32,
    pub w:    u32,
    pub h:    u32,
    pub _pad: [u8; 40],
}

/// Forwarded keyboard event.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PayloadEventKey {
    pub keycode: u8,
    pub pressed: u8,
    pub _pad:    [u8; 54],
}

/// Forwarded mouse event (absolute screen position + delta + buttons).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PayloadEventMouse {
    pub abs_x:   i32,
    pub abs_y:   i32,
    pub dx:      i16,
    pub dy:      i16,
    pub buttons: u8,
    pub _pad:    [u8; 43],
}

/// Focus change notification.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PayloadEventFocus {
    pub focused: u8,
    pub _pad:    [u8; 55],
}

/// WM-initiated resize notification.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PayloadEventResize {
    pub w:    u32,
    pub h:    u32,
    pub _pad: [u8; 48],
}

/// DS assigns a kernel surface_id to the app; app uses this ID in SYS_SURFACE_ATTACH.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PayloadSurfaceAssign {
    pub surface_id: u32,
    pub _pad: [u8; 52],
}

/// Raw/unknown payload: access bytes directly.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PayloadRaw {
    pub data: [u8; 56],
}

/// KDP (Kingdom Display Protocol) window/compositor payload (56 bytes).
///
/// Direction depends on `KwmMsg::msg_type`:
/// - KdpConnect    (client → compositor): surface_id, flags, title
/// - KdpGrant      (compositor → client): surface_id, x, y, width, height
/// - KdpCommit     (client → compositor): surface_id
/// - KdpResize     (compositor → client): surface_id, width, height
/// - KdpKey        (compositor → client): key_code, key_state
/// - KdpFocus      (compositor → client): flags (1=focused, 0=blur)
/// - KdpClose      (compositor → client): surface_id
/// - KdpDisconnect (client → compositor): surface_id
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PayloadKdp {
    /// Kernel surface ID (from SYS_SURFACE_CREATE, owned by the client).
    pub surface_id: u32,
    /// Window x (compositor → client in KdpGrant).
    pub x:          i32,
    /// Window y.
    pub y:          i32,
    /// Window width.
    pub width:      u32,
    /// Window height.
    pub height:     u32,
    /// Flags: KdpConnect hint bits; KdpFocus: 1=gained 0=lost; others: 0.
    pub flags:      u32,
    /// Keycode forwarded to client (KdpKey).
    pub key_code:   u32,
    /// Key state: 1=pressed, 0=released (KdpKey).
    pub key_state:  u32,
    /// NUL-terminated window title (client fills on KdpConnect; compositor echoes in KdpGrant).
    pub title:      [u8; 24],
}

/// Cogman control / response payload (56 bytes).
///
/// Sent from any process to cogman (pid 1) for Cog* requests.
/// Cogman fills `state`, `pid`, and `restart_count` on CogResp.
///
/// action byte (request):
///   0 = none / query, 1 = start, 2 = stop, 3 = restart, 4 = status, 5 = list
///
/// state byte (response):
///   0 = stopped, 1 = running, 2 = failed, 3 = restarting
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PayloadCogCtrl {
    /// Target service program_id (request) or reporting program_id (response).
    pub program_id:    u32,
    /// Action (request) or 0 (response).
    pub action:        u8,
    /// Service state (response).
    pub state:         u8,
    /// Restart count (response).
    pub restart_count: u16,
    /// Current PID of the service, 0 if not running (response).
    pub pid:           u32,
    /// NUL-terminated service name (response).
    pub name:          [u8; 16],
    pub _pad:          [u8; 28],
}

/// 56-byte payload union — pick the variant matching KwmMsg::msg_type.
///
/// Safety: All variants are the same size and purely numeric; any bit pattern
/// is valid for whichever variant you read.
#[repr(C)]
#[derive(Clone, Copy)]
pub union KwmPayload {
    pub register:        PayloadRegister,
    pub set_title:       PayloadSetTitle,
    pub surface_commit:  PayloadSurfaceCommit,
    pub surface_assign:  PayloadSurfaceAssign,
    pub geometry:        PayloadGeometry,
    pub event_key:       PayloadEventKey,
    pub event_mouse:     PayloadEventMouse,
    pub event_focus:     PayloadEventFocus,
    pub event_resize:    PayloadEventResize,
    pub cog_ctrl:        PayloadCogCtrl,
    pub kdp:             PayloadKdp,
    pub raw:             PayloadRaw,
}

/// Fixed 64-byte IPC message between Kingdom OS applications and the WM.
///
/// Cache-line aligned so that enqueue/dequeue is always a single cache-line
/// operation. Compatible with C via `#[repr(C)]`.
///
/// Layout:
/// ```text
/// offset  0: msg_type   (u8)
/// offset  1: flags      (u8)
/// offset  2: seq        (u16)
/// offset  4: sender_pid (u32)
/// offset  8: payload    (56 bytes)
/// total  = 64 bytes
/// ```
#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct KwmMsg {
    pub msg_type:   u8,
    pub flags:      u8,
    /// Monotonically-increasing sequence number; wraps at u16::MAX.
    pub seq:        u16,
    /// PID of the sending process (filled by kernel on SYS_IPC_SEND).
    pub sender_pid: u32,
    pub payload:    KwmPayload,
}

impl KwmMsg {
    /// A zeroed KwmMsg — safe to use as a stack buffer for SYS_IPC_RECV.
    pub const ZERO: KwmMsg = KwmMsg {
        msg_type:   0,
        flags:      0,
        seq:        0,
        sender_pid: 0,
        payload:    KwmPayload { raw: PayloadRaw { data: [0u8; 56] } },
    };
}

const _: () = assert!(core::mem::size_of::<KwmMsg>() == 64);
const _: () = assert!(core::mem::size_of::<KwmPayload>() == 56);
const _: () = assert!(core::mem::size_of::<PayloadCogCtrl>() == 56);
const _: () = assert!(core::mem::size_of::<PayloadKdp>() == 56);

/// Process and debug syscalls (namespace 0x300).
/// Debug: dump page tables for address range. Args: cr3 (a1), va_start (a2), va_end (a3).
pub const SYS_DEBUG_DUMP_PTES: u64 = 0x300;
/// Spawn a process by program id. Arg: program_id (0=shell, 1=wm, ...). Returns pid or negative error.
pub const SYS_SPAWN: u64 = 0x301;
/// Get process table snapshot. Args: buf (*mut ProcInfo), capacity (u32). Returns count filled or negative error.
pub const SYS_GET_PROC_INFO: u64 = 0x302;
/// Get current process ID. No args. Returns pid or negative error.
pub const SYS_GETPID: u64 = 0x303;
/// Reap a dead process. Args: pid (0 or u32::MAX = any), status_ptr (*mut i32 or null), options (0). Returns reaped pid or negative error.
pub const SYS_WAITPID: u64 = 0x304;

/// Physical address where the bootloader writes [`BootInfo`].
///
/// The kernel treats this region as identity-mapped low memory and copies the
/// contents into its own state during early init.
pub const BOOTINFO_PHYS_ADDR: u64 = 0x0000_8000;

/// System error codes (returned as negative from syscalls).
/// Original scheme: small positive identifiers, kernel returns negative.
pub const SYSERR_INVAL: i64 = -1; // Invalid argument or unsupported request
pub const SYSERR_NOENT: i64 = -2; // No such file or entry
pub const SYSERR_BADFD: i64 = -3; // Bad file descriptor or handle
pub const SYSERR_MFILE: i64 = -4; // Too many open files
pub const SYSERR_NOMEM: i64 = -5; // Out of resources / no free slots
pub const SYSERR_PERM:  i64 = -6; // Operation not permitted (e.g. non-compositor calling surface_commit)

/// Open flags for SYS_OPEN.
pub const O_RDONLY: u32 = 0;
pub const O_WRONLY: u32 = 1;
pub const O_RDWR: u32 = 2;
pub const O_CREAT: u32 = 0x40;
pub const O_TRUNC: u32 = 0x200;

/// Whence for SYS_LSEEK.
pub const SEEK_SET: u32 = 0;
pub const SEEK_CUR: u32 = 1;
pub const SEEK_END: u32 = 2;

/// Single keyboard event delivered to userland.
///
/// The kernel fills this struct when `SYS_POLL_INPUT` reports an event.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    /// Logical key code (see `keycodes`).
    pub keycode: u8,
    /// `true` on key press, `false` on key release.
    pub pressed: bool,
}

/// Mouse event (relative movement + button state). Fed by USB HID.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MouseEvent {
    pub dx: i16,
    pub dy: i16,
    pub buttons: u8,
}

/// User identifier used by the OS.
///
/// Single-user model: kernel is 0; all user processes run as DEFAULT_SESSION_UID (1000).
pub type Uid = u32;

/// Kernel / system identity (no user process has this).
pub const UID_KERNEL: Uid = 0;
/// Single-user session UID. Single-user OS: all user processes use this UID.
pub const DEFAULT_SESSION_UID: Uid = 1000;
/// Legacy alias for the single user; use DEFAULT_SESSION_UID for new code.
pub const UID_PRINCE: Uid = DEFAULT_SESSION_UID;

/// Process state for SYS_GET_PROC_INFO. Must match kernel ProcessState repr.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ProcInfo {
    pub pid: u32,
    pub state: u8, // 0=Empty, 1=Runnable, 2=Running, 3=Blocked, 4=Dead
}

/// Logical key codes for the WM and all userland programs.
///
/// The keyboard driver translates hardware scancodes (PS/2 set 1) into these
/// compact values before enqueuing `KeyEvent`s via `fleet::input::push_event`.
pub mod keycodes {
    // ── Navigation ────────────────────────────────────────────────────
    pub const KEY_LEFT:      u8 = 1;
    pub const KEY_RIGHT:     u8 = 2;
    pub const KEY_UP:        u8 = 3;
    pub const KEY_DOWN:      u8 = 4;
    pub const KEY_ENTER:     u8 = 5;
    pub const KEY_ESC:       u8 = 6;
    pub const KEY_BACKSPACE: u8 = 7;
    pub const KEY_TAB:       u8 = 8;
    pub const KEY_SPACE:     u8 = 9;

    // ── Modifiers ─────────────────────────────────────────────────────
    /// Super / Win key — used as WM modifier (Mod).
    pub const KEY_MOD:   u8 = 10;
    pub const KEY_SHIFT: u8 = 11;
    pub const KEY_CTRL:  u8 = 12;
    pub const KEY_ALT:   u8 = 13;

    // ── Number row (1–9, 0) ───────────────────────────────────────────
    pub const KEY_1: u8 = 20;
    pub const KEY_2: u8 = 21;
    pub const KEY_3: u8 = 22;
    pub const KEY_4: u8 = 23;
    pub const KEY_5: u8 = 24;
    pub const KEY_6: u8 = 25;
    pub const KEY_7: u8 = 26;
    pub const KEY_8: u8 = 27;
    pub const KEY_9: u8 = 28;
    pub const KEY_0: u8 = 29;

    // ── Alphabet (a=30 … z=55) ────────────────────────────────────────
    pub const KEY_A: u8 = 30;
    pub const KEY_B: u8 = 31;
    pub const KEY_C: u8 = 32;
    pub const KEY_D: u8 = 33;
    pub const KEY_E: u8 = 34;
    pub const KEY_F: u8 = 35;
    pub const KEY_G: u8 = 36;
    pub const KEY_H: u8 = 37;
    pub const KEY_I: u8 = 38;
    pub const KEY_J: u8 = 39;
    pub const KEY_K: u8 = 40;
    pub const KEY_L: u8 = 41;
    pub const KEY_M: u8 = 42;
    pub const KEY_N: u8 = 43;
    pub const KEY_O: u8 = 44;
    pub const KEY_P: u8 = 45;
    pub const KEY_Q: u8 = 46;
    pub const KEY_R: u8 = 47;
    pub const KEY_S: u8 = 48;
    pub const KEY_T: u8 = 49;
    pub const KEY_U: u8 = 50;
    pub const KEY_V: u8 = 51;
    pub const KEY_W: u8 = 52;
    pub const KEY_X: u8 = 53;
    pub const KEY_Y: u8 = 54;
    pub const KEY_Z: u8 = 55;

    // ── Punctuation ───────────────────────────────────────────────────
    pub const KEY_MINUS:  u8 = 56; // - / _
    pub const KEY_EQUAL:  u8 = 57; // = / +
    pub const KEY_COMMA:  u8 = 58; // , / <
    pub const KEY_PERIOD: u8 = 59; // . / >
    pub const KEY_SLASH:  u8 = 60; // / / ?
    pub const KEY_SEMI:   u8 = 61; // ; / :
    pub const KEY_QUOTE:  u8 = 62; // ' / "
    pub const KEY_LBRACE: u8 = 63; // [ / {
    pub const KEY_RBRACE: u8 = 64; // ] / }
    pub const KEY_BSLASH: u8 = 65; // \ / |
    pub const KEY_GRAVE:  u8 = 66; // ` / ~

    // ── Function keys ─────────────────────────────────────────────────
    pub const KEY_F1:  u8 = 70;
    pub const KEY_F2:  u8 = 71;
    pub const KEY_F3:  u8 = 72;
    pub const KEY_F4:  u8 = 73;
    pub const KEY_F5:  u8 = 74;
    pub const KEY_F6:  u8 = 75;
    pub const KEY_F7:  u8 = 76;
    pub const KEY_F8:  u8 = 77;
    pub const KEY_F9:  u8 = 78;
    pub const KEY_F10: u8 = 79;
    pub const KEY_F11: u8 = 80;
    pub const KEY_F12: u8 = 81;
}

/// Minimal boot-time information passed from the UEFI bootloader to the kernel.
///
/// Written by the bootloader at [`BOOTINFO_PHYS_ADDR`] before `ExitBootServices`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BootInfo {
    /// Physical address of the linear framebuffer (as reported by GOP).
    pub fb_base: u64,
    /// Total size in bytes of the framebuffer.
    pub fb_size: u64,
    /// Horizontal resolution in pixels.
    pub fb_width: u32,
    /// Vertical resolution in pixels.
    pub fb_height: u32,
    /// Number of pixels per scanline (stride).
    pub fb_stride: u32,
    /// Bits per pixel (e.g. 32 for X8R8G8B8).
    pub fb_bpp: u32,
    /// NVMe controller BAR0 physical address (MMIO). 0 = not present.
    pub nvme_bar: u64,
    /// TSC value captured in bootloader just before ExitBootServices (best-effort).
    pub boot_exit_tsc: u64,
    /// Physical address of a contiguous array of UEFI-like memory descriptors
    /// describing the memory map at the moment `ExitBootServices` succeeded.
    /// The kernel owns this region and may reinterpret it with a matching
    /// `MemoryDescriptor` definition.
    pub mem_map_paddr: u64,
    /// Size in bytes of the memory map stored at [`BootInfo::mem_map_paddr`].
    pub mem_map_size: u64,
    /// Size in bytes of each memory descriptor in the stored map.
    pub mem_desc_size: u32,
    /// UEFI memory descriptor version recorded when the map was captured
    /// (implementation-defined; 0 for the current bootloader).
    pub mem_desc_version: u32,
    /// Magic value indicating that the memory map fields are fully initialised
    /// and passed validation by the bootloader. Kernels should check this
    /// before trusting the map.
    pub mem_map_valid: u32,
    /// Reserved field for future memory-map-related flags; must be zeroed by
    /// the bootloader and ignored by the kernel for now.
    pub mem_map_reserved: u32,
    /// Physical address of the ACPI Root System Description Pointer (RSDP) if
    /// present in the UEFI configuration table, or 0 otherwise.
    pub rsdp_addr: u64,
    /// Packed bootloader version (implementation-defined). Intended for
    /// diagnostics and compatibility checks.
    pub bootloader_version: u32,
    /// Reserved for future expansion; must be zeroed by the bootloader.
    pub _reserved: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syscall_numbers() {
        assert_eq!(SYS_READ, 0x100);
        assert_eq!(SYS_WRITE, 0x101);
        assert_eq!(SYS_EXIT, 0x109);
    }

    #[test]
    fn test_syserr_values() {
        assert_eq!(SYSERR_INVAL, -1);
        assert_eq!(SYSERR_NOENT, -2);
        assert_eq!(SYSERR_BADFD, -3);
    }

    #[test]
    fn test_keyevent_layout() {
        // Sanity-check that KeyEvent stays tiny and POD-like.
        assert_eq!(core::mem::size_of::<KeyEvent>(), 2);
    }

    #[test]
    fn test_bootinfo_layout() {
        // Ensure BootInfo is a plain C struct with stable layout.
        assert!(core::mem::size_of::<BootInfo>() >= 32);
    }

    #[test]
    fn test_kwmmsg_size() {
        assert_eq!(core::mem::size_of::<KwmMsg>(), 64);
        assert_eq!(core::mem::size_of::<KwmPayload>(), 56);
    }

    #[test]
    fn test_payload_sizes() {
        assert_eq!(core::mem::size_of::<PayloadRegister>(), 56);
        assert_eq!(core::mem::size_of::<PayloadSetTitle>(), 56);
        assert_eq!(core::mem::size_of::<PayloadSurfaceCommit>(), 56);
        assert_eq!(core::mem::size_of::<PayloadGeometry>(), 56);
        assert_eq!(core::mem::size_of::<PayloadEventKey>(), 56);
        assert_eq!(core::mem::size_of::<PayloadEventMouse>(), 56);
        assert_eq!(core::mem::size_of::<PayloadEventFocus>(), 56);
        assert_eq!(core::mem::size_of::<PayloadEventResize>(), 56);
        assert_eq!(core::mem::size_of::<PayloadKdp>(), 56);
    }

    #[test]
    fn test_kwmmsg_zero() {
        let m = KwmMsg::ZERO;
        assert_eq!(m.msg_type, 0);
        assert_eq!(m.sender_pid, 0);
        unsafe { assert_eq!(m.payload.raw.data, [0u8; 56]); }
    }

    #[test]
    fn test_ipc_syscall_numbers() {
        assert_eq!(SYS_IPC_SEND, 0x320);
        assert_eq!(SYS_IPC_RECV, 0x321);
    }
}

