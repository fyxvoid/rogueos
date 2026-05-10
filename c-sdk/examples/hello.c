/* hello.c — minimal RogueOS / Linux hello-world app.
 *
 * Build for Linux test:  make example-linux
 * Build for RogueOS:     x86_64-elf-gcc -ffreestanding -nostdlib -I../include \
 *                        -o hello.elf hello.c ../build/bare/crt0.o ../build/bare/libk.a
 *
 * The main entry for RogueOS apps is kmain() (not main()).
 * On bare metal, crt0.S zeros BSS then calls kmain().
 * On Linux, we add a thin main() shim at the bottom.
 */

#include "rogueos.h"
#include "kwm.h"

/* ── libk prototypes (defined in libk.c) ─────────────────────────────── */
int    snprintf(char *buf, size_t n, const char *fmt, ...);
void   k_puts(const char *s);
void * malloc(size_t size);

/* ─────────────────────────────────────────────────────────────────────── */

void kmain(void) {
    char buf[64];
    long pid = k_getpid();

    snprintf(buf, sizeof(buf), "Hello from RogueOS! pid=%ld\n", pid);
    k_puts(buf);

    /* Demonstrate IPC types compile cleanly. */
    KwmMsg msg = KWM_ZERO;
    msg.msg_type = KWM_REGISTER;

    snprintf((char *)msg.payload.reg.title, 48, "Hello App");
    msg.payload.reg.flags = 0;

    /* On Linux this is a no-op; on RogueOS it sends to the WM. */
    k_ipc_send(1 /* wm_pid=1 */, &msg, 0);

    k_puts("Done.\n");
    k_exit(0);
}

/* Linux shim: main() → kmain() */
#ifdef ROGUEOS_LINUX
int main(void) { kmain(); return 0; }
#endif
