/* libk.c — minimal libc replacement for Kingdom OS C/C++ apps.
 *
 * Provides: memcpy, memmove, memset, memcmp, strlen, strcpy, strncpy,
 *           strcmp, strncmp, snprintf (subset), and a simple bump allocator.
 *
 * No dynamic linking; no OS libc.  Compile with -ffreestanding -nostdlib.
 */

#include <stdint.h>
#include <stddef.h>
#include <stdarg.h>
#include "kingdom.h"

/* ── Memory ─────────────────────────────────────────────────────────────── */

void *memcpy(void *dst, const void *src, size_t n) {
    uint8_t       *d = (uint8_t *)dst;
    const uint8_t *s = (const uint8_t *)src;
    for (size_t i = 0; i < n; i++) d[i] = s[i];
    return dst;
}

void *memmove(void *dst, const void *src, size_t n) {
    uint8_t       *d = (uint8_t *)dst;
    const uint8_t *s = (const uint8_t *)src;
    if (d < s) {
        for (size_t i = 0; i < n; i++) d[i] = s[i];
    } else {
        for (size_t i = n; i > 0; i--) d[i-1] = s[i-1];
    }
    return dst;
}

void *memset(void *dst, int c, size_t n) {
    uint8_t *d = (uint8_t *)dst;
    for (size_t i = 0; i < n; i++) d[i] = (uint8_t)c;
    return dst;
}

int memcmp(const void *a, const void *b, size_t n) {
    const uint8_t *p = (const uint8_t *)a;
    const uint8_t *q = (const uint8_t *)b;
    for (size_t i = 0; i < n; i++) {
        if (p[i] != q[i]) return (int)p[i] - (int)q[i];
    }
    return 0;
}

/* ── String ─────────────────────────────────────────────────────────────── */

size_t strlen(const char *s) {
    size_t n = 0;
    while (s[n]) n++;
    return n;
}

char *strcpy(char *dst, const char *src) {
    size_t i = 0;
    while ((dst[i] = src[i]) != 0) i++;
    return dst;
}

char *strncpy(char *dst, const char *src, size_t n) {
    size_t i = 0;
    while (i < n && src[i]) { dst[i] = src[i]; i++; }
    while (i < n) { dst[i] = 0; i++; }
    return dst;
}

int strcmp(const char *a, const char *b) {
    while (*a && *a == *b) { a++; b++; }
    return (unsigned char)*a - (unsigned char)*b;
}

int strncmp(const char *a, const char *b, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (!a[i] || a[i] != b[i])
            return (unsigned char)a[i] - (unsigned char)b[i];
    }
    return 0;
}

/* ── snprintf (subset: %d, %u, %x, %s, %c, %%) ─────────────────────────── */

static void _fmt_uint(char *buf, size_t *pos, size_t cap, unsigned long val, int base) {
    if (val == 0) {
        if (*pos < cap) buf[(*pos)++] = '0';
        return;
    }
    char tmp[32];
    int  len = 0;
    const char *digits = "0123456789abcdef";
    while (val) { tmp[len++] = digits[val % (unsigned)base]; val /= (unsigned)base; }
    for (int i = len - 1; i >= 0 && *pos < cap; i--)
        buf[(*pos)++] = tmp[i];
}

int snprintf(char *buf, size_t n, const char *fmt, ...) {
    if (n == 0) return 0;
    va_list ap;
    va_start(ap, fmt);
    size_t pos = 0;
    size_t cap = n - 1; /* leave room for NUL */
    while (*fmt && pos < cap) {
        if (*fmt != '%') { buf[pos++] = *fmt++; continue; }
        fmt++;
        switch (*fmt++) {
        case 'd': {
            long v = va_arg(ap, int);
            if (v < 0 && pos < cap) { buf[pos++] = '-'; v = -v; }
            _fmt_uint(buf, &pos, cap, (unsigned long)v, 10);
            break;
        }
        case 'u': _fmt_uint(buf, &pos, cap, va_arg(ap, unsigned int), 10); break;
        case 'x': _fmt_uint(buf, &pos, cap, va_arg(ap, unsigned int), 16); break;
        case 'l':
            if (*fmt == 'd') { fmt++; long v = va_arg(ap, long);
                if (v < 0 && pos < cap) { buf[pos++] = '-'; v = -v; }
                _fmt_uint(buf, &pos, cap, (unsigned long)v, 10); }
            else if (*fmt == 'u') { fmt++; _fmt_uint(buf, &pos, cap, va_arg(ap, unsigned long), 10); }
            else if (*fmt == 'x') { fmt++; _fmt_uint(buf, &pos, cap, va_arg(ap, unsigned long), 16); }
            break;
        case 's': {
            const char *s = va_arg(ap, const char *);
            if (!s) s = "(null)";
            while (*s && pos < cap) buf[pos++] = *s++;
            break;
        }
        case 'c': if (pos < cap) buf[pos++] = (char)va_arg(ap, int); break;
        case '%': if (pos < cap) buf[pos++] = '%'; break;
        default:  if (pos < cap) buf[pos++] = '?'; break;
        }
    }
    buf[pos] = '\0';
    va_end(ap);
    return (int)pos;
}

/* ── I/O helpers ─────────────────────────────────────────────────────────── */

void k_puts(const char *s) {
    k_write(1, s, strlen(s));
}

void k_putchar(char c) {
    k_write(1, &c, 1);
}

/* ── Bump allocator (1 MiB static heap) ─────────────────────────────────── */

#define HEAP_SIZE (1024 * 1024)
static uint8_t _heap[HEAP_SIZE];
static size_t  _heap_next = 0;

void *malloc(size_t size) {
    /* align to 16 bytes */
    size_t aligned = (_heap_next + 15) & ~(size_t)15;
    if (aligned + size > HEAP_SIZE) return 0;
    _heap_next = aligned + size;
    return &_heap[aligned];
}

void *calloc(size_t count, size_t size) {
    size_t total = count * size;
    void *p = malloc(total);
    if (p) memset(p, 0, total);
    return p;
}

/* free() is a no-op for a bump allocator — memory is never reclaimed */
void free(void *ptr) { (void)ptr; }
