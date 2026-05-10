/* kwm_linux.c — Linux shim for RogueOS syscalls.
 *
 * When compiled with -DROGUEOS_LINUX, rogueos.h declares k_syscallN() as
 * extern functions.  This file provides implementations that map RogueOS
 * syscall numbers to Linux equivalents so you can develop and test C apps
 * on a Linux host before running them on RogueOS.
 *
 * IPC (SYS_IPC_SEND / SYS_IPC_RECV) is not yet shimmed here — use the Unix
 * socket path or write directly to a file for quick local tests.
 *
 * Compile:
 *   gcc -DROGUEOS_LINUX -I../include -o myapp myapp.c kwm_linux.c libk.c
 */

#define _GNU_SOURCE
#include <unistd.h>
#include <sys/syscall.h>
#include <sys/mman.h>
#include <fcntl.h>
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>

#include "rogueos.h"

/* Map RogueOS syscall numbers → Linux actions. */
static long _dispatch(long num, long a1, long a2, long a3, long a4, long a5, long a6) {
    (void)a4; (void)a5; (void)a6;
    switch (num) {
    /* I/O */
    case SYS_READ:   return (long)read((int)a1,  (void *)a2, (size_t)a3);
    case SYS_WRITE:  return (long)write((int)a1, (void *)a2, (size_t)a3);
    case SYS_OPEN: {
        /* a1 = path ptr, a2 = path len (0 = null-terminated), a3 = flags */
        int lflags = O_RDONLY;
        if (a3 & O_WRONLY)  lflags = O_WRONLY;
        if (a3 & O_RDWR)    lflags = O_RDWR;
        if (a3 & O_CREAT)   lflags |= O_CREAT;
        if (a3 & O_TRUNC)   lflags |= O_TRUNC;
        return (long)open((const char *)a1, lflags, 0644);
    }
    case SYS_CLOSE:  return (long)close((int)a1);
    case SYS_LSEEK:  return (long)lseek((int)a1, (off_t)a2, (int)a3);
    case SYS_UNLINK: return (long)unlink((const char *)a1);
    case SYS_FSYNC:  return (long)fsync((int)a1);
    case SYS_EXIT:   _exit((int)a1); break;
    case SYS_GETPID: return (long)getpid();

    /* Graphics: on Linux just stub these out; real drawing needs minifb/SDL. */
    case SYS_FB_CLEAR:
    case SYS_FB_FILL_RECT:
    case SYS_FB_FLUSH:
    case SYS_FB_BLIT:
    case SYS_SCREEN_SIZE:
    case SYS_SURFACE_CREATE:
    case SYS_SURFACE_DESTROY:
    case SYS_SURFACE_ATTACH:
    case SYS_SURFACE_COMMIT:
    case SYS_POLL_INPUT:
    case SYS_POLL_MOUSE:
        return 0; /* no-op on Linux host */

    /* IPC: not shimmed yet — return EAGAIN for recv, 0 for send */
    case SYS_IPC_SEND:
        fprintf(stderr, "[kwm_linux] IPC send to pid=%ld (no-op on Linux)\n", a1);
        return 0;
    case SYS_IPC_RECV:
        return SYSERR_AGAIN; /* always empty queue on Linux host */

    default:
        fprintf(stderr, "[kwm_linux] unhandled syscall num=0x%lx\n", num);
        return -1;
    }
    return -1;
}

long k_syscall0(long num)                                              { return _dispatch(num,0,0,0,0,0,0); }
long k_syscall1(long num, long a1)                                     { return _dispatch(num,a1,0,0,0,0,0); }
long k_syscall2(long num, long a1, long a2)                            { return _dispatch(num,a1,a2,0,0,0,0); }
long k_syscall3(long num, long a1, long a2, long a3)                   { return _dispatch(num,a1,a2,a3,0,0,0); }
long k_syscall4(long num, long a1, long a2, long a3, long a4)          { return _dispatch(num,a1,a2,a3,a4,0,0); }
long k_syscall5(long num, long a1, long a2, long a3, long a4, long a5){ return _dispatch(num,a1,a2,a3,a4,a5,0); }
long k_syscall6(long num, long a1, long a2, long a3, long a4, long a5, long a6) { return _dispatch(num,a1,a2,a3,a4,a5,a6); }
