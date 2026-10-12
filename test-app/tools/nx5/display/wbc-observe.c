/* Read-only observer of composed pixels. Owns only a previously idle WBC.
 * Run alongside `display --pattern`; normal frames must be uniformly black/white.
 * Coverage is WBC (~30 FPS), not every physical LCD frame.
 */
#ifndef BRIDGE_IN_PROCESS
#error "Use build-observer.sh: loading this SDK in a separate process resets VO"
#endif
#define _POSIX_C_SOURCE 200809L
#include <fcntl.h>
#include <inttypes.h>
#include <linux/fb.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/ioctl.h>
#include <time.h>
#include <unistd.h>
#include "mpi_vo.h"

static uint64_t now_ns(void) {
    struct timespec ts; clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000u + ts.tv_nsec;
}
static int check(const char *name, int rc) {
    if (rc) fprintf(stderr, "%s rc=%08x\n", name, rc);
    return rc;
}
int main(int argc, char **argv) {
    if (argc != 3) { fprintf(stderr, "usage: %s frames (1..1800) framebuffer\n", argv[0]); return 2; }
    char *end; unsigned long limit = strtoul(argv[1], &end, 10);
    if (*end || !limit || limit > 1800) return 2;
    int fb = open(argv[2], O_RDONLY | O_CLOEXEC);
    if (fb < 0) { perror("open framebuffer for WBC"); return 2; }
    struct fb_var_screeninfo mode = {0};
    int queried = ioctl(fb, FBIOGET_VSCREENINFO, &mode);
    close(fb);
    if (queried < 0 || !mode.xres || !mode.yres) {
        fprintf(stderr, "WBC requires a valid framebuffer resolution\n"); return 2;
    }
    int mem = open("/dev/mem", O_RDONLY | O_SYNC);
    if (mem < 0) { perror("/dev/mem"); return 2; }
    const volatile unsigned *regs = mmap(NULL, 4096, PROT_READ, MAP_SHARED, mem, 0x34101000u);
    if (regs == MAP_FAILED) { close(mem); return 2; }
    int enabled = !!(regs[0x90/4] & 1u);
    munmap((void *)regs, 4096);
    if (enabled) { fprintf(stderr, "WBC is already enabled\n"); close(mem); return 2; }
    VO_WBC_SOURCE_S source = {VO_WBC_SOURCE_DEV, 0};
    VO_WBC_ATTR_S attr = {{mode.xres, mode.yres}, PIXEL_FORMAT_YUV_SEMIPLANAR_420, 60};
    int result = 2;
    unsigned black = 0, white = 0, mixed = 0, received = 0;
    uint64_t start = now_ns(), first_pts = 0, last_pts = 0;
    if (check("source", FY_MPI_VO_SetWbcSource(0, &source)) ||
        check("attr", FY_MPI_VO_SetWbcAttr(0, &attr)) ||
        check("depth", FY_MPI_VO_SetWbcDepth(0, 2))) goto close_fd;
    if (check("enable", FY_MPI_VO_EnableWbc(0))) goto close_fd;
    for (unsigned i = 0; i < limit; ++i) {
        VIDEO_FRAME_INFO_S frame = {0};
        if (check("get", FY_MPI_VO_GetWbcFrame(0, &frame, 500))) goto disable;
        unsigned width = frame.stVFrame.u32Width, height = frame.stVFrame.u32Height;
        unsigned stride = frame.stVFrame.u32Stride[0], address = frame.stVFrame.u32PhyAddr[0];
        int valid = width == mode.xres && height == mode.yres && stride >= width && address &&
            frame.stVFrame.enPixelFormat == PIXEL_FORMAT_YUV_SEMIPLANAR_420 &&
            height <= (SIZE_MAX - (address & 4095u)) / stride;
        unsigned low = 255, high = 0;
        if (valid) {
            unsigned offset = address & 4095u;
            size_t length = offset + (size_t)stride * height;
            const unsigned char *map = mmap(NULL, length, PROT_READ, MAP_SHARED, mem, address & ~4095u);
            if (map == MAP_FAILED) valid = 0;
            else {
                /* Sample every row at five columns. Full bytewise reads from
                 * uncached /dev/mem reduce this firmware to ~7 captures/s.
                 * This is deliberately a bounded sample, not all pixels. */
                const unsigned columns[] = {0, (width - 1) / 4, (width - 1) / 2,
                    (unsigned)((uint64_t)(width - 1) * 3 / 4), width - 1};
                for (unsigned y = 0; y < height; ++y) {
                    const unsigned char *row = map + offset + (size_t)y * stride;
                    for (unsigned x = 0; x < 5; ++x) {
                        unsigned value = row[columns[x]];
                        if (value < low) low = value;
                        if (value > high) high = value;
                    }
                }
                munmap((void *)map, length);
            }
        }
        uint64_t pts = frame.stVFrame.u64pts;
        if (check("release", FY_MPI_VO_ReleaseWbcFrame(0, &frame)) || !valid) goto disable;
        if (!i) first_pts = pts;
        last_pts = pts;
        ++received;
        if (high <= 1) ++black;
        else if (low >= 254) ++white;
        else ++mixed;
        printf("frame=%u pts=%" PRIu64 " min=%u max=%u\n", i, pts, low, high);
        fflush(stdout);
    }
    result = mixed || !black || !white ? 1 : 0;
disable:
    if (check("disable", FY_MPI_VO_DisableWbc(0))) result = 2;
close_fd:
#ifndef BRIDGE_IN_PROCESS
    if (check("close", FY_MPI_VO_CloseFd())) result = 2;
#endif
    close(mem);
    printf("summary frames=%u black=%u white=%u mixed=%u elapsed_ms=%.3f first_pts=%" PRIu64 " last_pts=%" PRIu64 " result=%d\n",
        received, black, white, mixed, (now_ns() - start) / 1000000.0, first_pts, last_pts, result);
    return result;
}
