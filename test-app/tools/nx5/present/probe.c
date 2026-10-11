/* Diagnostic only. No framebuffer writes and no MMIO writes.
 * Register definitions: NX5 Processor User Guide, sections 10.1.5.2.
 * Register readback is NOT proof of the pixels reaching the panel.
 */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <linux/fb.h>
#include <pthread.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

#define DHD_BASE 0x34101000u
#define MAX_EVENTS 30000
struct registers { uint32_t cfg, update, work, status, mask, raw, addr; };
struct event { uint64_t ns; struct registers r; };
static volatile const uint32_t *regs;
static struct event events[MAX_EVENTS];
static unsigned event_count;
static unsigned dropped;
static atomic_int sampling;
static volatile sig_atomic_t interrupted;
static unsigned long fb_base;
static unsigned fb_stride;

static uint64_t now_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000u + t.tv_nsec;
}
static void delay_us(unsigned us) {
    struct timespec t = {us / 1000000u, (us % 1000000u) * 1000u};
    while (nanosleep(&t, &t) && errno == EINTR && !interrupted) {}
}
static void on_signal(int sig) { interrupted = sig; }
static struct registers read_regs(void) {
    /* Only documented read-safe words; never read/modify INT_CLR. */
    struct registers r;
    r.cfg = regs[0x00 / 4];
    r.update = regs[0x08 / 4];
    r.work = regs[0x0c / 4];
    r.status = regs[0x10 / 4];
    r.mask = regs[0x14 / 4];
    r.raw = regs[0x1c / 4];
    r.addr = regs[0x134 / 4];
    return r;
}
static void print_regs(const char *kind, uint64_t ns, struct registers r) {
    printf("%s ns=%" PRIu64 " cfg=%08x update=%08x work=%08x status=%08x mask=%08x raw=%08x addr=%08x\n",
           kind, ns, r.cfg, r.update, r.work, r.status, r.mask, r.raw, r.addr);
}
static uint64_t irq_count(void) {
    FILE *f = fopen("/proc/interrupts", "r");
    if (!f) return UINT64_MAX;
    char line[256]; uint64_t result = UINT64_MAX;
    while (fgets(line, sizeof line, f)) {
        unsigned irq; unsigned long long a, b;
        if (strstr(line, "vou@34100000") &&
            sscanf(line, "%u: %llu %llu", &irq, &a, &b) == 3 && irq == 137) {
            result = a + b;
            break;
        }
    }
    fclose(f);
    return result;
}
static void *sample(void *unused) {
    (void)unused;
    struct registers previous = {0};
    while (atomic_load(&sampling)) {
        uint64_t ns = now_ns();
        struct registers r = read_regs();
        if (!event_count || memcmp(&r, &previous, sizeof r)) {
            if (event_count < MAX_EVENTS) events[event_count++] = (struct event){ns, r};
            else ++dropped;
            previous = r;
        }
        delay_us(50);
    }
    return NULL;
}
static int flip(int fd, const struct fb_var_screeninfo *original,
                unsigned offset, int step) {
    struct fb_var_screeninfo v = *original, actual = {0};
    v.yoffset = offset;
    /* Match the existing NX5 driver path; don't alter activate/mode flags. */
    uint64_t irq0 = irq_count(), start = now_ns();
    int rc = ioctl(fd, FBIOPUT_VSCREENINFO, &v);
    int saved_errno = rc < 0 ? errno : 0;
    uint64_t end = now_ns();
    struct registers after = read_regs();
    uint64_t observed = now_ns(), irq1 = irq_count();
    int read_rc = ioctl(fd, FBIOGET_VSCREENINFO, &actual);
    printf("flip step=%d start=%" PRIu64 " end=%" PRIu64 " observed=%" PRIu64
           " requested=%u actual=%u expected_addr=%08lx rc=%d errno=%d read_rc=%d"
           " irq_before=%" PRIu64 " irq_after=%" PRIu64 "\n",
           step, start, end, observed, offset, actual.yoffset,
           fb_base + (unsigned long)offset * fb_stride,
           rc, saved_errno, read_rc, irq0, irq1);
    print_regs("returned", observed, after);
    fflush(stdout);
    return rc < 0 || read_rc < 0 || actual.yoffset != offset;
}

int main(int argc, char **argv) {
    /* Snapshot by default. Flips require an explicit count and stopped owner. */
    unsigned count = 0;
    if (argc == 3 && !strcmp(argv[1], "--flips")) {
        char *end; unsigned long n = strtoul(argv[2], &end, 10);
        if (*end || n < 1 || n > 400) return 2;
        count = (unsigned)n;
    } else if (argc != 1) {
        fprintf(stderr, "usage: %s [--flips 1..400]\n", argv[0]); return 2;
    }
    struct sigaction sa = {0}; sa.sa_handler = on_signal;
    sigaction(SIGINT, &sa, NULL); sigaction(SIGTERM, &sa, NULL);
    int fd = open("/dev/fb0", count ? O_RDWR : O_RDONLY);
    if (fd < 0) { perror("open framebuffer"); return 3; }
    struct fb_fix_screeninfo f = {0}; struct fb_var_screeninfo v = {0};
    if (ioctl(fd, FBIOGET_FSCREENINFO, &f) || ioctl(fd, FBIOGET_VSCREENINFO, &v)) {
        perror("framebuffer info"); return 3;
    }
    FILE *model_file = fopen("/proc/device-tree/model", "r");
    char model[64] = {0};
    if (model_file) { fread(model, 1, sizeof model - 1, model_file); fclose(model_file); }
    if (strcmp(model, "nx5") || strncmp(f.id, "fyfb", 4)) {
        fprintf(stderr, "not the expected nx5/fyfb target; refusing MMIO access\n"); return 4;
    }
    int memfd = open("/dev/mem", O_RDONLY | O_SYNC);
    if (memfd < 0) { perror("open /dev/mem"); return 3; }
    regs = mmap(NULL, 4096, PROT_READ, MAP_SHARED, memfd, DHD_BASE);
    if (regs == MAP_FAILED) { perror("map documented DHD registers"); return 3; }
    fb_base = f.smem_start; fb_stride = f.line_length;
    printf("device driver=%.16s base=%08lx length=%u stride=%u width=%u height=%u"
           " virtual_height=%u bpp=%u original=%u activate=%u flips=%u\n",
           f.id, f.smem_start, f.smem_len, f.line_length, v.xres, v.yres,
           v.yres_virtual, v.bits_per_pixel, v.yoffset, v.activate, count);
    print_regs("initial", now_ns(), read_regs());
    if (!count) { munmap((void *)regs, 4096); close(memfd); close(fd); return 0; }
    uint64_t page = (uint64_t)f.line_length * v.yres;
    if (strncmp(f.id, "fyfb", 4) || v.xres != 1024 || v.yres != 600 ||
        v.bits_per_pixel != 32 || f.line_length != 4096 ||
        v.yres_virtual < v.yres * 2 || f.smem_len < page * 2 ||
        (v.yoffset != 0 && v.yoffset != v.yres)) {
        fprintf(stderr, "unexpected NX5 layout; refusing flips\n"); return 4;
    }
    const char *owner = getenv("NX5_STOPPED_OWNER");
    if (!owner || !*owner) {
        fprintf(stderr, "stop and verify the UI owner; set NX5_STOPPED_OWNER to its PID\n"); return 4;
    }
    char path[96], line[256];
    snprintf(path, sizeof path, "/proc/%s/status", owner);
    FILE *status = fopen(path, "r"); int stopped = 0;
    if (status) {
        while (fgets(line, sizeof line, status))
            if (!strncmp(line, "State:", 6) && strchr(line, 'T')) stopped = 1;
        fclose(status);
    }
    if (!stopped) { fprintf(stderr, "UI owner is not stopped\n"); return 4; }
    pthread_t thread; atomic_store(&sampling, 1);
    if (pthread_create(&thread, NULL, sample, NULL)) return 5;
    int failed = 0; unsigned offset = v.yoffset;
    for (unsigned i = 0; i < count && !interrupted; ++i) {
        /* Vary phase without repeating the previous unsafe unpaced probe. */
        delay_us(35000u + (i * 7919u) % 30000u);
        offset = offset ? 0 : v.yres;
        if (flip(fd, &v, offset, (int)i)) { failed = 1; break; }
    }
    delay_us(50000);
    int restore = flip(fd, &v, v.yoffset, -1);
    delay_us(50000);
    atomic_store(&sampling, 0); pthread_join(thread, NULL);
    for (unsigned i = 0; i < event_count; ++i)
        print_regs("sample", events[i].ns, events[i].r);
    printf("done failed=%d interrupted=%d restored=%d events=%u dropped=%u\n",
           failed, interrupted, !restore, event_count, dropped);
    munmap((void *)regs, 4096); close(memfd); close(fd);
    return failed || restore || interrupted || dropped ? 1 : 0;
}
