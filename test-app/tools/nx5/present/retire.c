/* NX5 diagnostic: poison the retired page immediately after blocking PUT returns.
 * Observe composed pixels using WBC, rather than certifying cached yoffset.
 * Only valid for the documented NX5 layout, with the existing UI stopped.
 */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <linux/fb.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include "mpi_vo.h"
#include "recovery.h"

#define WIDTH 1024u
#define HEIGHT 600u
#define PAGE_BYTES (WIDTH * HEIGHT * 4u)
#define MAX_FRAMES 3000u
struct capture { uint64_t pts, ns; unsigned low, high, middle; };
static struct capture captures[MAX_FRAMES];
static atomic_uint capture_count;
static atomic_int running, capture_error;
static int memfd;

static uint64_t now_ns(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000u + t.tv_nsec;
}
static void delay_us(unsigned us) {
    struct timespec t = {us / 1000000u, (us % 1000000u) * 1000u};
    while (nanosleep(&t, &t) && errno == EINTR) {}
}
static int call(const char *name, int rc) {
    printf("%s rc=%08x\n", name, rc); fflush(stdout); return rc;
}
static void fill(uint8_t *page, uint32_t color) {
    uint32_t *p = (uint32_t *)page;
    for (unsigned i = 0; i < WIDTH * HEIGHT; ++i) p[i] = color;
    __sync_synchronize();
}
static int present(int fd, const struct fb_var_screeninfo *original, unsigned page) {
    struct fb_var_screeninfo v = *original; v.yoffset = page * HEIGHT;
    return ioctl(fd, FBIOPUT_VSCREENINFO, &v);
}
static void *capture(void *unused) {
    (void)unused;
    while (atomic_load(&running)) {
        VIDEO_FRAME_INFO_S frame = {0};
        if (FY_MPI_VO_GetWbcFrame(0, &frame, 500)) { atomic_store(&capture_error, 1); break; }
        unsigned width = frame.stVFrame.u32Width, height = frame.stVFrame.u32Height;
        unsigned stride = frame.stVFrame.u32Stride[0], address = frame.stVFrame.u32PhyAddr[0];
        unsigned n = atomic_load(&capture_count);
        int ok = width == WIDTH && height == HEIGHT && stride >= WIDTH && stride <= 8192 &&
                 frame.stVFrame.enPixelFormat == PIXEL_FORMAT_YUV_SEMIPLANAR_420 && address && n < MAX_FRAMES;
        struct capture c = {frame.stVFrame.u64pts, now_ns(), 255, 0, 0};
        if (ok) {
            unsigned offset = address & 4095u; size_t length = (size_t)stride * HEIGHT + offset;
            const uint8_t *map = mmap(NULL, length, PROT_READ, MAP_SHARED, memfd, address & ~4095u);
            if (map == MAP_FAILED) ok = 0;
            else {
                /* Every row; five separated columns. Uniform test frames make
                 * horizontal tears and old-page poison observable at low overhead.
                 */
                const unsigned columns[] = {0, 256, 512, 768, 1023};
                for (unsigned y = 0; y < HEIGHT; ++y) {
                    for (unsigned x = 0; x < 5; ++x) {
                        unsigned value = map[offset + (size_t)y * stride + columns[x]];
                        if (value < c.low) c.low = value;
                        if (value > c.high) c.high = value;
                        if (value > 1 && value < 254) ++c.middle;
                    }
                }
                munmap((void *)map, length);
            }
        }
        if (FY_MPI_VO_ReleaseWbcFrame(0, &frame)) ok = 0;
        if (!ok) { atomic_store(&capture_error, 1); break; }
        captures[n] = c;
        atomic_store(&capture_count, n + 1);
    }
    return NULL;
}
int main(int argc, char **argv) {
    int early = argc == 3 && !strcmp(argv[2], "--early-reuse");
    if (argc != 2 && !early) return 2;
    char *end; unsigned long requested = strtoul(argv[1], &end, 10);
    if (*end || requested < 1 || requested > 128) return 2;
    const char *owner = getenv("NX5_STOPPED_OWNER");
    if (!owner) return 2;
    char path[96], line[256]; snprintf(path, sizeof path, "/proc/%s/status", owner);
    FILE *s = fopen(path, "r"); int stopped = 0;
    if (s) { while (fgets(line, sizeof line, s)) if (!strncmp(line, "State:", 6) && strchr(line, 'T')) stopped = 1; fclose(s); }
    if (!stopped) return 2;
    int fd = open("/dev/fb0", O_RDWR);
    struct fb_fix_screeninfo f = {0}; struct fb_var_screeninfo v = {0};
    if (fd < 0 || ioctl(fd, FBIOGET_FSCREENINFO, &f) || ioctl(fd, FBIOGET_VSCREENINFO, &v)) return 3;
    if (strncmp(f.id, "fyfb", 4) || v.xres != WIDTH || v.yres != HEIGHT ||
        f.line_length != WIDTH * 4 || v.bits_per_pixel != 32 || f.smem_len < PAGE_BYTES * 2 ||
        (v.yoffset != 0 && v.yoffset != HEIGHT)) return 3;
    memfd = open("/dev/mem", O_RDONLY | O_SYNC);
    if (memfd < 0) return 3;
    volatile const uint32_t *regs = mmap(NULL, 4096, PROT_READ, MAP_SHARED, memfd, 0x34101000u);
    uint8_t *map = mmap(NULL, PAGE_BYTES * 2, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    uint8_t *backup = malloc(PAGE_BYTES * 2);
    if (map == MAP_FAILED || regs == MAP_FAILED || !backup || (regs[0x90 / 4] & 1)) return 3;
    memcpy(backup, map, PAGE_BYTES * 2);
    unsigned original = v.yoffset / HEIGHT, front = 1 - original;
    fill(map + front * PAGE_BYTES, 0xffffffffu);
    if (present(fd, &v, front)) { preserve_backup(backup, PAGE_BYTES * 2); return 4; }
    delay_us(100000);
    int failed = 0, enabled = 0, started = 0, uncertain = 0;
    pthread_t worker;
    VO_WBC_SOURCE_S source = {VO_WBC_SOURCE_DEV, 0};
    VO_WBC_ATTR_S attr = {{WIDTH, HEIGHT}, PIXEL_FORMAT_YUV_SEMIPLANAR_420, 60};
    if (call("set_source", FY_MPI_VO_SetWbcSource(0, &source)) ||
        call("set_attr", FY_MPI_VO_SetWbcAttr(0, &attr)) ||
        call("set_depth", FY_MPI_VO_SetWbcDepth(0, 2)) ||
        call("enable", FY_MPI_VO_EnableWbc(0))) { failed = 1; goto restore; }
    enabled = 1;
    for (unsigned i = 0; i < 3; ++i) {
        VIDEO_FRAME_INFO_S frame = {0};
        if (FY_MPI_VO_GetWbcFrame(0, &frame, 500)) { failed = 1; goto restore; }
        if (FY_MPI_VO_ReleaseWbcFrame(0, &frame)) { failed = 1; goto restore; }
    }
    printf("config cfg=%08x work=%08x update=%08x wbc=%08x pipeline=%08x\n",
           regs[0], regs[0xc / 4], regs[0x8 / 4], regs[0x90 / 4], regs[0x8c / 4]);
    printf("mode early_reuse=%d requested_flips=%lu\n", early, requested);
    if ((regs[0x90 / 4] & 7) != 7 || (regs[0xc / 4] & 0x700)) { failed = 1; goto restore; }
    atomic_store(&running, 1);
    if (pthread_create(&worker, NULL, capture, NULL)) { failed = 1; goto restore; }
    started = 1;
    uint64_t ready_deadline = now_ns() + 2000000000ull;
    while (atomic_load(&capture_count) < 3 && !atomic_load(&capture_error) && now_ns() < ready_deadline) delay_us(1000);
    if (atomic_load(&capture_count) < 3) { failed = 1; goto restore; }
    for (unsigned i = 0; i < requested && !atomic_load(&capture_error); ++i) {
        unsigned next = 1 - front;
        fill(map + next * PAGE_BYTES, (i & 1) ? 0xffffffffu : 0xff000000u);
        /* Negative control: deliberately corrupt the still-visible page. */
        if (early) fill(map + front * PAGE_BYTES, 0xff808080u);
        uint64_t begin = now_ns();
        int rc = present(fd, &v, next);
        uint64_t returned = now_ns();
        if (rc) { failed = 1; uncertain = 1; break; }
        /* The precise condition under test: no extra vsync, polling, query,
         * or sleep between blocking completion and old-page overwrite.
         */
        uint64_t poison_start = now_ns();
        fill(map + front * PAGE_BYTES, 0xff808080u);
        uint64_t poison_end = now_ns();
        front = next;
        printf("reuse step=%u requested=%u start=%" PRIu64 " returned=%" PRIu64
               " poison_start=%" PRIu64 " poison_end=%" PRIu64 " update=%08x addr=%08x\n",
               i, front, begin, returned, poison_start, poison_end, regs[0x8 / 4], regs[0x134 / 4]);
        fflush(stdout);
        delay_us(2000u + (i * 7919u) % 17000u);
    }
    delay_us(100000);
    FILE *vo = fopen("/proc/umap/vo", "r");
    if (vo) { while (fgets(line, sizeof line, vo)) printf("vo %s", line); fclose(vo); }
restore:
    if (started) { atomic_store(&running, 0); pthread_join(worker, NULL); }
    unsigned count = atomic_load(&capture_count), white = 0, black = 0, bad = 0;
    for (unsigned i = 0; i < count; ++i) {
        struct capture c = captures[i];
        int uniform = c.high <= 1 || c.low >= 254;
        white += c.low >= 254; black += c.high <= 1; bad += !uniform;
        printf("capture index=%u pts=%" PRIu64 " ns=%" PRIu64 " min=%u max=%u middle=%u valid=%d\n",
               i, c.pts, c.ns, c.low, c.high, c.middle, uniform);
    }
    if (atomic_load(&capture_error) || !white || !black || bad) failed = 1;
    printf("result frames=%u white=%u black=%u mixed_or_poison=%u capture_error=%d failed=%d\n",
           count, white, black, bad, atomic_load(&capture_error), failed);
    if (enabled && call("disable", FY_MPI_VO_DisableWbc(0))) failed = 1;
    if (call("close", FY_MPI_VO_CloseFd())) failed = 1;
    if (uncertain) { preserve_backup(backup, PAGE_BYTES * 2); return 4; }
    /* Restore via an inactive page, with conservative settling during recovery. */
    unsigned back = 1 - front;
    memcpy(map + back * PAGE_BYTES, backup + back * PAGE_BYTES, PAGE_BYTES);
    __sync_synchronize();
    if (present(fd, &v, back)) { preserve_backup(backup, PAGE_BYTES * 2); return 5; }
    delay_us(100000);
    memcpy(map + front * PAGE_BYTES, backup + front * PAGE_BYTES, PAGE_BYTES);
    __sync_synchronize();
    if (back != original && present(fd, &v, original)) { preserve_backup(backup, PAGE_BYTES * 2); return 5; }
    delay_us(100000);
    int same = !memcmp(map, backup, PAGE_BYTES * 2);
    printf("retire_restore pixels_equal=%d original=%u\n", same, original);
    free(backup); munmap(map, PAGE_BYTES * 2); munmap((void *)regs, 4096); close(memfd); close(fd);
    return failed || !same ? 1 : 0;
}
