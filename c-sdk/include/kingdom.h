/* kingdom.h — Kingdom OS syscall ABI for C/C++ applications.
 *
 * All syscalls use: rax=number, rdi/rsi/rdx/r10/r8/r9=args (note: 4th arg in
 * r10, not rcx, because SYSCALL clobbers rcx).  Return value in rax.
 *
 * Compile for bare-metal Kingdom OS:
 *   x86_64-elf-gcc -ffreestanding -nostdlib -o app.elf app.c crt0.o libk.o
 *
 * For Linux dev testing, compile with -DKINGDOM_LINUX and link linux/kwm_linux.c
 * to remap syscalls to Linux equivalents.
 */

#ifndef KINGDOM_H
#define KINGDOM_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <stddef.h>

/* ── Syscall numbers ──────────────────────────────────────────────────── */

#define SYS_READ           0x100
#define SYS_WRITE          0x101
#define SYS_OPEN           0x102
#define SYS_CLOSE          0x103
#define SYS_LSEEK          0x104
#define SYS_UNLINK         0x105
#define SYS_FSYNC          0x106
#define SYS_LIST_ROOT      0x107
#define SYS_REBOOT         0x108
#define SYS_EXIT           0x109

#define SYS_POLL_INPUT     0x200
#define SYS_FB_CLEAR       0x201
#define SYS_FB_FILL_RECT   0x202
#define SYS_FB_FLUSH       0x203
#define SYS_POLL_MOUSE     0x204
#define SYS_SURFACE_CREATE 0x210
#define SYS_SURFACE_DESTROY 0x211
#define SYS_SURFACE_ATTACH 0x212
#define SYS_SURFACE_COMMIT 0x213
#define SYS_SCREEN_SIZE    0x214
#define SYS_FB_BLIT        0x215

#define SYS_SPAWN          0x301
#define SYS_GET_PROC_INFO  0x302
#define SYS_GETPID         0x303
#define SYS_WAITPID        0x304

#define SYS_IPC_SEND       0x320
#define SYS_IPC_RECV       0x321

/* ── Error codes (returned as negative from syscalls) ─────────────────── */
#define SYSERR_INVAL  (-1)
#define SYSERR_NOENT  (-2)
#define SYSERR_BADFD  (-3)
#define SYSERR_MFILE  (-4)
#define SYSERR_NOMEM  (-5)
#define SYSERR_AGAIN  (-11)

/* ── IPC flags ────────────────────────────────────────────────────────── */
#define IPC_NONBLOCK  0x01u

/* ── Open flags (guarded so they don't clash with Linux <fcntl.h>) ─────── */
#ifndef O_RDONLY
#define O_RDONLY  0
#endif
#ifndef O_WRONLY
#define O_WRONLY  1
#endif
#ifndef O_RDWR
#define O_RDWR    2
#endif
#ifndef O_CREAT
#define O_CREAT   0x40
#endif
#ifndef O_TRUNC
#define O_TRUNC   0x200
#endif

/* ── Seek whence (guarded for Linux compatibility) ─────────────────────── */
#ifndef SEEK_SET
#define SEEK_SET 0
#endif
#ifndef SEEK_CUR
#define SEEK_CUR 1
#endif
#ifndef SEEK_END
#define SEEK_END 2
#endif

/* ── Key event ─────────────────────────────────────────────────────────── */
typedef struct {
    uint8_t keycode;
    uint8_t pressed; /* 1=press, 0=release */
} KeyEvent;

/* ── Mouse event ───────────────────────────────────────────────────────── */
typedef struct {
    int16_t dx;
    int16_t dy;
    uint8_t buttons;
} MouseEvent;

/* ── Raw syscall helpers (x86-64 inline asm) ───────────────────────────── */

#ifndef KINGDOM_LINUX /* bare-metal: real SYSCALL instruction */

static inline long
k_syscall0(long num) {
    long ret;
    __asm__ volatile ("syscall"
        : "=a"(ret) : "a"(num) : "rcx","r11","memory");
    return ret;
}

static inline long
k_syscall1(long num, long a1) {
    long ret;
    __asm__ volatile ("syscall"
        : "=a"(ret) : "a"(num), "D"(a1) : "rcx","r11","memory");
    return ret;
}

static inline long
k_syscall2(long num, long a1, long a2) {
    long ret;
    __asm__ volatile ("syscall"
        : "=a"(ret) : "a"(num), "D"(a1), "S"(a2) : "rcx","r11","memory");
    return ret;
}

static inline long
k_syscall3(long num, long a1, long a2, long a3) {
    long ret;
    __asm__ volatile ("syscall"
        : "=a"(ret) : "a"(num), "D"(a1), "S"(a2), "d"(a3) : "rcx","r11","memory");
    return ret;
}

/* 4th arg goes in r10 (not rcx) because SYSCALL clobbers rcx */
static inline long
k_syscall4(long num, long a1, long a2, long a3, long a4) {
    long ret;
    register long r10 __asm__("r10") = a4;
    __asm__ volatile ("syscall"
        : "=a"(ret) : "a"(num), "D"(a1), "S"(a2), "d"(a3), "r"(r10)
        : "rcx","r11","memory");
    return ret;
}

static inline long
k_syscall5(long num, long a1, long a2, long a3, long a4, long a5) {
    long ret;
    register long r10 __asm__("r10") = a4;
    register long r8  __asm__("r8")  = a5;
    __asm__ volatile ("syscall"
        : "=a"(ret) : "a"(num), "D"(a1), "S"(a2), "d"(a3), "r"(r10), "r"(r8)
        : "rcx","r11","memory");
    return ret;
}

static inline long
k_syscall6(long num, long a1, long a2, long a3, long a4, long a5, long a6) {
    long ret;
    register long r10 __asm__("r10") = a4;
    register long r8  __asm__("r8")  = a5;
    register long r9  __asm__("r9")  = a6;
    __asm__ volatile ("syscall"
        : "=a"(ret) : "a"(num), "D"(a1), "S"(a2), "d"(a3), "r"(r10), "r"(r8), "r"(r9)
        : "rcx","r11","memory");
    return ret;
}

#else /* KINGDOM_LINUX — redirect to linux/kwm_linux.c shim */

long k_syscall0(long num);
long k_syscall1(long num, long a1);
long k_syscall2(long num, long a1, long a2);
long k_syscall3(long num, long a1, long a2, long a3);
long k_syscall4(long num, long a1, long a2, long a3, long a4);
long k_syscall5(long num, long a1, long a2, long a3, long a4, long a5);
long k_syscall6(long num, long a1, long a2, long a3, long a4, long a5, long a6);

#endif /* KINGDOM_LINUX */

/* ── High-level syscall wrappers ─────────────────────────────────────── */

static inline void   k_exit(int status)       { k_syscall1(SYS_EXIT,  status); __builtin_unreachable(); }
static inline long   k_read(int fd, void *buf, size_t n)  { return k_syscall3(SYS_READ,  fd, (long)buf, (long)n); }
static inline long   k_write(int fd, const void *buf, size_t n) { return k_syscall3(SYS_WRITE, fd, (long)buf, (long)n); }
static inline long   k_open(const char *path, size_t plen, uint32_t flags) { return k_syscall3(SYS_OPEN, (long)path, (long)plen, flags); }
static inline long   k_close(int fd)          { return k_syscall1(SYS_CLOSE, fd); }
static inline long   k_getpid(void)           { return k_syscall0(SYS_GETPID); }
static inline long   k_spawn(uint32_t prog_id){ return k_syscall1(SYS_SPAWN, prog_id); }
static inline long   k_reboot(uint32_t mode)  { return k_syscall1(SYS_REBOOT, mode); }

static inline long   k_fb_clear(uint32_t color)   { return k_syscall1(SYS_FB_CLEAR, color); }
static inline long   k_fb_fill_rect(uint32_t x, uint32_t y, uint32_t w, uint32_t h, uint32_t color) {
    return k_syscall5(SYS_FB_FILL_RECT, x, y, w, h, color);
}
static inline long   k_fb_flush(void)             { return k_syscall0(SYS_FB_FLUSH); }
static inline long   k_screen_size(uint32_t *w, uint32_t *h) {
    return k_syscall2(SYS_SCREEN_SIZE, (long)w, (long)h);
}
static inline long   k_fb_blit(uint32_t dx, uint32_t dy, uint32_t w, uint32_t h,
                                uint32_t stride, const void *pixels) {
    return k_syscall6(SYS_FB_BLIT, dx, dy, w, h, stride, (long)pixels);
}
static inline long   k_poll_input(KeyEvent *ev)   { return k_syscall1(SYS_POLL_INPUT, (long)ev); }
static inline long   k_poll_mouse(MouseEvent *ev)  { return k_syscall1(SYS_POLL_MOUSE, (long)ev); }

static inline long   k_surface_create(void)        { return k_syscall0(SYS_SURFACE_CREATE); }
static inline long   k_surface_destroy(uint32_t id){ return k_syscall1(SYS_SURFACE_DESTROY, id); }
static inline long   k_surface_attach(uint32_t id, const void *buf, uint32_t w, uint32_t h, uint32_t stride) {
    return k_syscall5(SYS_SURFACE_ATTACH, id, (long)buf, w, h, stride);
}
static inline long   k_surface_commit(uint32_t id, uint32_t x, uint32_t y) {
    return k_syscall3(SYS_SURFACE_COMMIT, id, x, y);
}

#ifdef __cplusplus
} /* extern "C" */
#endif
#endif /* KINGDOM_H */
