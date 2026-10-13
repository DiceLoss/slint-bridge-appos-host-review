/* Exclusive NX5 diagnostic: compare CPU conversion straight into framebuffer
 * with exact CPU conversion in cached MMZ followed by a synchronous TDE copy.
 * This does not enable a new production writeback path. */
#include "../../../../src/backends/nx5/tde/native.c"
#include <assert.h>
#include <fcntl.h>
#include <linux/fb.h>
#include <sys/file.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

static double now_us(void) {
    struct timespec t; assert(clock_gettime(CLOCK_MONOTONIC, &t) == 0);
    return t.tv_sec * 1e6 + t.tv_nsec / 1e3;
}
static int ownership(void) {
    int fd = open("/run/slint-bridge-nx5.lock", O_CREAT | O_RDWR, 0600);
    assert(fd >= 0 && flock(fd, LOCK_EX | LOCK_NB) == 0);
    FILE *f = fopen("/proc/umap/vo", "r"); assert(f);
    char line[512]; int table = 0, found = 0;
    while (fgets(line, sizeof(line), f)) {
        if (strstr(line, "-----DEV CONFIG")) { table = 1; found = 1; continue; }
        if (table && strstr(line, "-----VIDEO LAYER STATUS")) break;
        unsigned id; char enabled;
        assert(!table || sscanf(line, " %u %c", &id, &enabled) != 2 || enabled != 'Y');
    }
    fclose(f); assert(found); return fd;
}
static void sdk_check(int code, const char *where) {
    if (code) {
        /* Never free MMZ or close SYS/VO after uncertain DMA completion. */
        fprintf(stderr, "FAILED %s code=%d; no further writes or SDK cleanup\n", where, code);
        _exit(2);
    }
}
static void pan(int fd, struct fb_var_screeninfo *v, unsigned back) {
    struct fb_var_screeninfo current;
    struct fb_fix_screeninfo fixed;
    assert(ioctl(fd, FBIOGET_VSCREENINFO, &current) == 0);
    assert(ioctl(fd, FBIOGET_FSCREENINFO, &fixed) == 0);
    assert(current.xres == v->xres && current.yres == v->yres &&
           current.bits_per_pixel == 32 && fixed.line_length == v->xres * 4 &&
           current.red.offset == 16 && current.green.offset == 8 &&
           current.blue.offset == 0 && current.transp.offset == 24);
    current.yoffset = back * current.yres;
    current.activate = FB_ACTIVATE_VBL;
    /* #233's blocking PAN retirement contract, same as the NX5 backend.
     * No next page write happens before it returns successfully. */
    assert(ioctl(fd, FBIOPAN_DISPLAY, &current) == 0);
    assert(ioctl(fd, FBIOGET_VSCREENINFO, &current) == 0);
    assert(current.xres == v->xres && current.yres == v->yres &&
           current.yoffset == back * v->yres);
    *v = current;
}
static void pattern(uint8_t *out, unsigned width, unsigned height, unsigned kind, unsigned seed) {
    for (unsigned y = 0; y < height; ++y) for (unsigned x = 0; x < width; ++x) {
        unsigned a = kind == 0 ? 255 : kind == 1 ? 0 : kind == 2 ? 127 : (x + y + seed) & 255;
        unsigned at = (y * width + x) * 4;
        out[at] = ((x + seed * 13) & 255) * a / 255;
        out[at + 1] = ((y * 3 + seed * 7) & 255) * a / 255;
        out[at + 2] = ((x * 37 + y * 11 + seed) & 255) * a / 255;
        out[at + 3] = a;
    }
}
static void verify(const uint8_t *source, const uint8_t *output, unsigned count) {
    for (unsigned i = 0; i < count; ++i) {
        unsigned a = source[i * 4 + 3];
        for (unsigned c = 0; c < 3; ++c) {
            unsigned expected = a ? (source[i * 4 + 2 - c] * 255 + a / 2) / a : 0;
            assert(output[i * 4 + c] == expected);
        }
        assert(output[i * 4 + 3] == (a + 1) / 2);
    }
}
static int order_double(const void *a, const void *b) {
    double left = *(const double *)a, right = *(const double *)b;
    return (left > right) - (left < right);
}
int main(int argc, char **argv) {
    if (argc != 2) { fprintf(stderr, "usage: probe-writeback FRAMEBUFFER\n"); return 1; }
    setbuf(stdout, NULL);
    int owner = ownership();
    void *library = dlopen("libVideoDec.so", RTLD_NOW | RTLD_NODELETE); assert(library);
    int (*disable)(void) = dlsym(library, "VO_Disable"); assert(disable);
    int fd = open(argv[1], O_RDWR); assert(fd >= 0 && flock(fd, LOCK_EX | LOCK_NB) == 0);
    struct fb_var_screeninfo v; struct fb_fix_screeninfo f;
    assert(ioctl(fd, FBIOGET_VSCREENINFO, &v) == 0 && ioctl(fd, FBIOGET_FSCREENINFO, &f) == 0);
    /* Keep this physical benchmark explicitly scoped to the verified mode. */
    assert(v.xres == 1024 && v.yres == 600 && v.bits_per_pixel == 32 &&
           v.xoffset == 0 && (v.yoffset == 0 || v.yoffset == v.yres) &&
           v.xres_virtual == v.xres && v.yres_virtual >= v.yres * 2 &&
           v.red.offset == 16 && v.green.offset == 8 && v.blue.offset == 0 &&
           v.transp.offset == 24 && v.red.length == 8 && v.green.length == 8 &&
           v.blue.length == 8 && v.transp.length == 8 &&
           f.line_length == v.xres * 4 && f.smem_len >= f.line_length * v.yres * 2 &&
           f.smem_start > 0 && f.smem_start + f.smem_len <= UINT32_MAX);
    unsigned bytes = f.line_length * v.yres, count = v.xres * v.yres;
    uint8_t *mapped = mmap(NULL, bytes * 2, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    assert(mapped != MAP_FAILED);
    int error; Engine *e = bridge_tde_create(v.xres, v.yres, &error); assert(e);
    uint8_t *source = e->target.ptr, *converted = e->source[0].ptr;
    uint8_t *expected_source = malloc(bytes), *actual = malloc(bytes), *front_pixels = malloc(bytes);
    assert(expected_source && actual && front_pixels);
    uint32_t reciprocals[256] = {0};
    for (unsigned a = 1; a < 256; ++a) reciprocals[a] = ((255u << 16) + a - 1) / a;
    unsigned front = v.yoffset / v.yres, presentations = 0, checked_pixels = 0;
    printf("layout width=%u height=%u stride=%u physical=%lx alpha_max=128\n",v.xres,v.yres,f.line_length,f.smem_start);
    const char *names[] = {"opaque", "clear", "alpha127", "alpha-ramp"};
    for (unsigned kind = 0; kind < 4; ++kind) {
        double conversion[2] = {0}, flush[2] = {0}, dma[2] = {0}, totals[2][30];
        for (unsigned iteration = 0; iteration < 35; ++iteration) {
            pattern(source, v.xres, v.yres, kind, iteration);
            for (unsigned step = 0; step < 2; ++step) {
                unsigned path = (iteration + step) % 2, back = 1 - front;
                if (iteration == 34) {
                    memcpy(expected_source, source, bytes);
                    assert(pread(fd, front_pixels, bytes, (off_t)front * bytes) == (ssize_t)bytes);
                    // Prove even an all-zero output was actually written;
                    // an already-clear page must not hide a missed DMA copy.
                    memset(mapped + back * bytes, 0xa5, bytes);
                    __sync_synchronize();
                }
                double start = now_us(), converted_at, flushed_at, done;
                if (path == 0) {
                    bridge_rgba_to_bgra(mapped + back * bytes, source, count, reciprocals, 128);
                    __sync_synchronize();
                    converted_at = now_us(); flushed_at = converted_at; done = converted_at;
                } else {
                    /* Diagnostic reuse of an otherwise unused source slot;
                     * production would need separate staging/cache ownership. */
                    bridge_rgba_to_bgra(converted, source, count, reciprocals, 128);
                    converted_at = now_us();
                    sdk_check(e->api.FY_MPI_SYS_MmzFlushCache(e->source[0].phy, converted, bytes), "flush converted");
                    flushed_at = now_us();
                    TDE2_SURFACE_S s = surface(e->source[0].phy, v.xres, v.yres);
                    TDE2_SURFACE_S d = surface(f.smem_start + back * bytes, v.xres, v.yres);
                    s.bAlphaMax255 = FY_FALSE; d.bAlphaMax255 = FY_FALSE;
                    TDE2_RECT_S rect = {0, 0, v.xres, v.yres};
                    TDE_HANDLE job = e->api.FY_TDE2_BeginJob(); assert(job >= 0);
                    int result = e->api.FY_TDE2_QuickCopy(job, &s, &rect, &d, &rect);
                    if (result) {
                        sdk_check(e->api.FY_TDE2_CancelJob(job), "cancel");
                        sdk_check(result, "quickcopy");
                    }
                    sdk_check(e->api.FY_TDE2_EndJob(job, 1, 1, 100), "end");
                    done = now_us();
                }
                if (iteration >= 5) {
                    conversion[path] += converted_at - start;
                    flush[path] += flushed_at - converted_at;
                    dma[path] += done - flushed_at;
                    totals[path][iteration - 5] = done - start;
                }
                if (iteration == 34) {
                    assert(pread(fd, actual, bytes, (off_t)back * bytes) == (ssize_t)bytes);
                    verify(source, actual, count); checked_pixels += count;
                    assert(!memcmp(source, expected_source, bytes));
                    assert(pread(fd, actual, bytes, (off_t)front * bytes) == (ssize_t)bytes);
                    assert(!memcmp(front_pixels, actual, bytes));
                }
                pan(fd, &v, back); front = back; ++presentations;
            }
        }
        for (unsigned path = 0; path < 2; ++path) {
            qsort(totals[path], 30, sizeof(double), order_double);
            printf("writeback name=%s path=%s samples=30 convert_us=%.3f flush_us=%.3f dma_us=%.3f total_mean_us=%.3f total_p50_us=%.3f total_p95_us=%.3f pixels_exact=true source_unchanged=true front_unchanged=true\n",
                names[kind], path ? "cpu-mmz+tde-copy" : "cpu-direct-fb",
                conversion[path]/30,flush[path]/30,dma[path]/30,
                (conversion[path]+flush[path]+dma[path])/30,totals[path][15],totals[path][28]);
        }
    }
    printf("verified_pixels=%u presentations=%u\n",checked_pixels,presentations);
    free(front_pixels); free(actual); free(expected_source);
    bridge_tde_destroy(e); assert(disable() == 1); dlclose(library);
    assert(munmap(mapped, bytes * 2) == 0); close(fd); close(owner); return 0;
}
