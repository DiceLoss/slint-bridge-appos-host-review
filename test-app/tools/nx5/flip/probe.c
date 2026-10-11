/* Diagnostic: read-only documented DHD MMIO, no SDK and no pixel writes.
 * Optional PAN tests require the existing display owner to be stopped.
 * A register observation is not a scanout retirement fence.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <linux/fb.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

struct event { uint64_t ns, irq; uint32_t update, addr; };
static struct event events[10000];
static unsigned length, dropped;
static atomic_int sampling;
static volatile const uint32_t *regs;

static uint64_t now_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000u + t.tv_nsec;
}
static void delay_us(unsigned us) {
    if (us < 35000) {
        uint64_t until = now_ns() + (uint64_t)us * 1000;
        while (now_ns() < until) {}
        return;
    }
    struct timespec t = { us / 1000000, (us % 1000000) * 1000 };
    while (nanosleep(&t, &t) && errno == EINTR) {}
}
static uint64_t irq_count(void) {
    FILE *f = fopen("/proc/interrupts", "r");
    if (!f) { perror("interrupts"); exit(2); }
    char line[256]; uint64_t result = 0;
    while (fgets(line, sizeof line, f)) {
        unsigned irq; unsigned long long a, b;
        if (strstr(line, "vou@34100000") &&
            sscanf(line, "%u: %llu %llu", &irq, &a, &b) == 3) result += a + b;
    }
    fclose(f);
    return result;
}
static void *sample(void *unused) {
    (void)unused;
    cpu_set_t cpus; CPU_ZERO(&cpus); CPU_SET(1, &cpus);
    if (pthread_setaffinity_np(pthread_self(), sizeof cpus, &cpus)) exit(2);
    struct event prev = {0};
    while (atomic_load(&sampling)) {
        struct event e;
        e.irq = irq_count();
        e.update = regs[0x08 / 4]; e.addr = regs[0x134 / 4];
        e.ns = now_ns();
        if (!length || e.irq != prev.irq || e.update != prev.update || e.addr != prev.addr) {
            if (length < sizeof events / sizeof events[0]) events[length++] = e;
            else ++dropped;
            prev = e;
        }
        delay_us(50);
    }
    return NULL;
}
static void require_stopped_owner(void) {
    const char *owner = getenv("NX5_STOPPED_OWNER");
    char path[96], line[256]; int stopped = 0;
    if (!owner || !*owner || strspn(owner, "0123456789") != strlen(owner)) exit(2);
    snprintf(path, sizeof path, "/proc/%s/status", owner);
    FILE *f = fopen(path, "r");
    if (f) {
        while (fgets(line, sizeof line, f))
            if (!strncmp(line, "State:", 6) && strchr(line, 'T')) stopped = 1;
        fclose(f);
    }
    if (!stopped) { fputs("display owner must be stopped\n", stderr); exit(2); }
}
int main(int argc, char **argv) {
    cpu_set_t cpus; CPU_ZERO(&cpus); CPU_SET(0, &cpus);
    if (sched_setaffinity(0, sizeof cpus, &cpus)) return 2;
    int flips = argc == 2 && !strcmp(argv[1], "--flips");
    if (argc != 1 && !flips) return 2;
    char model[32] = {0};
    FILE *f = fopen("/proc/device-tree/model", "r");
    if (!f) return 2;
    fread(model, 1, sizeof model - 1, f); fclose(f);
    if (strcmp(model, "nx5")) return 2;
    if (flips) require_stopped_owner();
    int mem = open("/dev/mem", O_RDONLY | O_SYNC);
    if (mem < 0) { perror("mem"); return 2; }
    regs = mmap(NULL, 4096, PROT_READ, MAP_SHARED, mem, 0x34101000);
    if (regs == MAP_FAILED) { perror("mmap"); return 2; }
    unsigned offsets[] = {0, 8, 12, 0x14, 0x24, 0x28, 0x2c, 0x74, 0x78, 0x7c, 0x8c, 0xb4, 0xbc};
    for (unsigned i = 0; i < sizeof offsets / sizeof offsets[0]; ++i)
        printf("reg offset=%03x value=%08x\n", offsets[i], regs[offsets[i] / 4]);
    int fd = -1, failed = 0;
    struct fb_var_screeninfo original = {0}, v = {0};
    if (flips) {
        fd = open("/dev/fb0", O_RDWR);
        struct fb_fix_screeninfo fix;
        if (fd < 0 || ioctl(fd, FBIOGET_VSCREENINFO, &original) ||
            ioctl(fd, FBIOGET_FSCREENINFO, &fix)) return 2;
        if (strncmp(fix.id, "fyfb", 4) || original.xres != 1024 || original.yres != 600 ||
            original.yres_virtual != 1200 || original.bits_per_pixel != 32 ||
            fix.line_length != 4096 || (original.yoffset != 0 && original.yoffset != 600)) return 2;
        v = original;
    }
    pthread_t thread;
    atomic_store(&sampling, 1);
    if (pthread_create(&thread, NULL, sample, NULL)) return 2;
    delay_us(1000000);
    if (flips) {
        for (unsigned i = 0; i < 256; ++i) {
            /* Zero preparation, then fixed and swept preparation phases. */
            unsigned pause = i < 128 ? 0 : i < 160 ? 8000 : i < 192 ? 15000 : (i - 192) * 500;
            delay_us(pause);
            v.yoffset = 600 - v.yoffset;
            uint64_t start = now_ns();
            int rc = ioctl(fd, FBIOPAN_DISPLAY, &v);
            uint64_t end = now_ns();
            struct fb_var_screeninfo actual;
            int read_rc = ioctl(fd, FBIOGET_VSCREENINFO, &actual);
            printf("flip index=%u pause_us=%u start=%" PRIu64 " end=%" PRIu64
                   " offset=%u rc=%d read_rc=%d actual=%u update=%08x addr=%08x\n",
                   i, pause, start, end, v.yoffset, rc, read_rc,
                   read_rc ? UINT32_MAX : actual.yoffset, regs[2], regs[0x134 / 4]);
            if (rc || read_rc || actual.yoffset != v.yoffset) { failed = 1; break; }
        }
        if (!failed && ioctl(fd, FBIOPAN_DISPLAY, &original)) failed = 1;
    } else delay_us(9000000);
    delay_us(100000);
    atomic_store(&sampling, 0); pthread_join(thread, NULL);
    for (unsigned i = 0; i < length; ++i)
        printf("sample ns=%" PRIu64 " irq=%" PRIu64 " update=%08x addr=%08x\n",
               events[i].ns, events[i].irq, events[i].update, events[i].addr);
    printf("done failed=%d dropped=%u restored=%d\n", failed, dropped, flips && !failed);
    if (fd >= 0) close(fd);
    munmap((void *)regs, 4096); close(mem);
    return failed || dropped ? 1 : 0;
}
