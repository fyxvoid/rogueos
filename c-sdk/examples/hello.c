/* hello.c — minimal Kingdom OS / Linux hello-world app.
 *
 * Build for Linux test:  make example-linux
 * Build for Kingdom OS:  x86_64-elf-gcc -ffreestanding -nostdlib -I../include \
 *                        -o hello.elf hello.c ../build/bare/crt0.o ../build/bare/libk.a
 *
 * The main entry for Kingdom OS apps is kmain() (not main()).
 * On bare metal, crt0.S zeros BSS then calls kmain().
 * On Linux, we add a thin main() shim at the bottom.
 */

#include "kingdom.h"
#include "kwm.h"

/* ── libk prototypes (defined in libk.c) ─────────────────────────────── */
int    snprintf(char *buf, size_t n, const char *fmt, ...);
void   k_puts(const char *s);
void * malloc(size_t size);

/* ─────────────────────────────────────────────────────────────────────── */

void kmain(void) {
    char buf[64];
    long pid = k_getpid();

    snprintf(buf, sizeof(buf), "Hello from Kingdom OS! pid=%ld\n", pid);
    k_puts(buf);

    /* Demonstrate IPC types compile cleanly. */
    KwmMsg msg = KWM_ZERO;
    msg.msg_type = KWM_REGISTER;

    snprintf((char *)msg.payload.reg.title, 48, "Hello App");
    msg.payload.reg.flags = 0;

    /* On Linux this is a no-op; on Kingdom OS it sends to the WM. */
    k_ipc_send(1 /* wm_pid=1 */, &msg, 0);

    k_puts("Done.\n");
    k_exit(0);
}

/* Linux shim: main() → kmain() */
#ifdef KINGDOM_LINUX
int main(void) { kmain(); return 0; }
#endif
