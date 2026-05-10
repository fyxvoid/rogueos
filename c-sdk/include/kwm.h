/* kwm.h — KWM IPC protocol for C/C++ RogueOS apps.
 *
 * KwmMsg is a 64-byte fixed-size message compatible with the Rust KwmMsg struct
 * in libs/src/lib.rs.  Layout is guaranteed by _Static_assert.
 *
 * Usage:
 *   KwmMsg msg = KWM_ZERO;
 *   msg.msg_type = KWM_REGISTER;
 *   // fill payload ...
 *   k_ipc_send(wm_pid, &msg, 0);
 */

#ifndef KWM_H
#define KWM_H

#include "rogueos.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ── Message type bytes ─────────────────────────────────────────────────── */
/* App → WM */
#define KWM_REGISTER       0x01
#define KWM_UNREGISTER     0x02
#define KWM_SET_TITLE      0x03
#define KWM_SURFACE_COMMIT 0x04
/* WM → App */
#define KWM_GEOMETRY       0x10
#define KWM_EVENT_KEY      0x11
#define KWM_EVENT_MOUSE    0x12
#define KWM_EVENT_FOCUS    0x13
#define KWM_EVENT_RESIZE   0x14
/* Bidirectional */
#define KWM_ACK            0x20
#define KWM_PING           0x21

/* ── Payload structs (each exactly 56 bytes) ────────────────────────────── */

typedef struct {
    uint8_t  title[48];
    uint32_t flags;
    uint8_t  _pad[4];
} KwmPayloadRegister;
_Static_assert(sizeof(KwmPayloadRegister) == 56, "KwmPayloadRegister must be 56 bytes");

typedef struct {
    uint8_t title[56];
} KwmPayloadSetTitle;
_Static_assert(sizeof(KwmPayloadSetTitle) == 56, "KwmPayloadSetTitle must be 56 bytes");

typedef struct {
    uint32_t surface_id;
    int32_t  x, y;
    uint32_t w, h;
    uint8_t  _pad[36];
} KwmPayloadSurfaceCommit;
_Static_assert(sizeof(KwmPayloadSurfaceCommit) == 56, "KwmPayloadSurfaceCommit must be 56 bytes");

typedef struct {
    int32_t  x, y;
    uint32_t w, h;
    uint8_t  _pad[40];
} KwmPayloadGeometry;
_Static_assert(sizeof(KwmPayloadGeometry) == 56, "KwmPayloadGeometry must be 56 bytes");

typedef struct {
    uint8_t keycode;
    uint8_t pressed;
    uint8_t _pad[54];
} KwmPayloadEventKey;
_Static_assert(sizeof(KwmPayloadEventKey) == 56, "KwmPayloadEventKey must be 56 bytes");

typedef struct {
    int32_t  abs_x, abs_y;
    int16_t  dx, dy;
    uint8_t  buttons;
    uint8_t  _pad[43];
} KwmPayloadEventMouse;
_Static_assert(sizeof(KwmPayloadEventMouse) == 56, "KwmPayloadEventMouse must be 56 bytes");

typedef struct {
    uint8_t focused;
    uint8_t _pad[55];
} KwmPayloadEventFocus;
_Static_assert(sizeof(KwmPayloadEventFocus) == 56, "KwmPayloadEventFocus must be 56 bytes");

typedef struct {
    uint32_t w, h;
    uint8_t  _pad[48];
} KwmPayloadEventResize;
_Static_assert(sizeof(KwmPayloadEventResize) == 56, "KwmPayloadEventResize must be 56 bytes");

/* ── Payload union ──────────────────────────────────────────────────────── */

typedef union {
    KwmPayloadRegister      reg;
    KwmPayloadSetTitle      set_title;
    KwmPayloadSurfaceCommit surface_commit;
    KwmPayloadGeometry      geometry;
    KwmPayloadEventKey      event_key;
    KwmPayloadEventMouse    event_mouse;
    KwmPayloadEventFocus    event_focus;
    KwmPayloadEventResize   event_resize;
    uint8_t                 raw[56];
} KwmPayload;
_Static_assert(sizeof(KwmPayload) == 56, "KwmPayload must be 56 bytes");

/* ── KwmMsg (64 bytes, cache-line aligned) ──────────────────────────────── */

typedef struct __attribute__((aligned(64))) {
    uint8_t    msg_type;
    uint8_t    flags;
    uint16_t   seq;
    uint32_t   sender_pid;
    KwmPayload payload;
} KwmMsg;
_Static_assert(sizeof(KwmMsg) == 64, "KwmMsg must be 64 bytes");

/* Zero-initialised KwmMsg for stack use */
#define KWM_ZERO ((KwmMsg){0})

/* ── IPC syscall wrappers ────────────────────────────────────────────────── */

static inline long k_ipc_send(uint32_t target_pid, const KwmMsg *msg, uint32_t flags) {
    return k_syscall3(SYS_IPC_SEND, (long)target_pid, (long)msg, (long)flags);
}

static inline long k_ipc_recv(KwmMsg *out, uint32_t flags) {
    return k_syscall2(SYS_IPC_RECV, (long)out, (long)flags);
}

#ifdef __cplusplus
} /* extern "C" */
#endif
#endif /* KWM_H */
