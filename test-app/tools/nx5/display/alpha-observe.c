/* Loaded only by alpha-regression after Slint owns VO. Read physical pages and
 * sample post-compositor Y/UV; never initialize/close VO or change layer alpha. */
#define _POSIX_C_SOURCE 200809L
#include <fcntl.h>
#include <inttypes.h>
#include <linux/fb.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>
#include "mpi_vo.h"

static int checked(const char *operation, int result) {
    if (result) fprintf(stderr, "alpha: %s rc=%08x\n", operation, result);
    return result;
}
static int pixels(int fd, const struct fb_fix_screeninfo *fixed,
                  const struct fb_var_screeninfo *mode, int shift, int visible) {
    const unsigned alphas[] = {0, 1, 63, 127, 128, 191, 254, 255};
    unsigned char *row = malloc(mode->xres * 4);
    if (!row) return -1;
    unsigned bad = 0, first_x = 0, first_y = 0;
    unsigned cell_w = mode->xres / 8, cell_h = mode->yres / 6;
    for (unsigned y = 0; y < mode->yres; ++y) {
        off_t offset = (off_t)(mode->yoffset + y) * fixed->line_length;
        if (pread(fd, row, mode->xres * 4, offset) != (ssize_t)mode->xres * 4) { free(row); return -1; }
        for (unsigned x = 0; x < mode->xres; ++x) {
            unsigned char expected[4] = {0};
            if (visible && x % cell_w >= (unsigned)shift && x % cell_w < (unsigned)shift + cell_w - 16 &&
                y % cell_h >= 8 && y % cell_h < cell_h - 8) {
                unsigned alpha = alphas[x / cell_w], channel = (y / cell_h) / 2;
                int reference = (y / cell_h) & 1;
                expected[3] = reference ? 255 : alpha;
                if (expected[3]) expected[2 - channel] = reference ? alpha : 255;
                expected[3] = (expected[3] + 1u) / 2u;
            }
            if (memcmp(row + x * 4, expected, 4)) {
                if (!bad) { first_x = x; first_y = y; }
                ++bad;
            }
        }
    }
    free(row);
    fprintf(stderr, "alpha: framebuffer page=%u pixels=%u bad=%u first_bad=%u,%u\n",
        mode->yoffset / mode->yres, mode->xres * mode->yres, bad, first_x, first_y);
    return bad ? -1 : 0;
}
static int sample(int mem, const VIDEO_FRAME_INFO_S *frame, unsigned width, unsigned height,
                  int visible, FILE *output, unsigned index) {
    const VIDEO_FRAME_S *f = &frame->stVFrame;
    if (f->u32Width != width || f->u32Height != height ||
        f->enPixelFormat != PIXEL_FORMAT_YUV_SEMIPLANAR_420) return -1;
    const unsigned char *map[2] = {NULL, NULL};
    size_t length[2] = {0};
    int result = -1;
    for (unsigned plane = 0; plane < 2; ++plane) {
        unsigned address = f->u32PhyAddr[plane], stride = f->u32Stride[plane];
        unsigned rows = plane ? height / 2 : height;
        if (!address || stride < width || rows > (SIZE_MAX - (address & 4095u)) / stride) goto done;
        length[plane] = (address & 4095u) + (size_t)stride * rows;
        map[plane] = mmap(NULL, length[plane], PROT_READ, MAP_SHARED, mem, address & ~4095u);
        if (map[plane] == MAP_FAILED) { map[plane] = NULL; goto done; }
    }
    const unsigned char *y_plane = map[0] + (f->u32PhyAddr[0] & 4095u);
    const unsigned char *uv_plane = map[1] + (f->u32PhyAddr[1] & 4095u);
    unsigned bad = 0, maximum = 0;
    for (unsigned channel = 0; channel < 3; ++channel) {
        for (unsigned column = 0; column < 8; ++column) {
            for (int dy = -4; dy <= 4; dy += 2) for (int dx = -4; dx <= 4; dx += 2) {
                unsigned x = column * width / 8 + width / 16 + dx;
                unsigned y = channel * height / 3 + height / 12 + dy;
                unsigned reference_y = y + height / 6;
                unsigned value[3] = {y_plane[(size_t)y * f->u32Stride[0] + x],
                    uv_plane[(size_t)(y / 2) * f->u32Stride[1] + (x & ~1u)],
                    uv_plane[(size_t)(y / 2) * f->u32Stride[1] + (x & ~1u) + 1]};
                unsigned reference[3] = {y_plane[(size_t)reference_y * f->u32Stride[0] + x],
                    uv_plane[(size_t)(reference_y / 2) * f->u32Stride[1] + (x & ~1u)],
                    uv_plane[(size_t)(reference_y / 2) * f->u32Stride[1] + (x & ~1u) + 1]};
                for (unsigned c = 0; c < 3; ++c) {
                    unsigned delta = abs((int)value[c] - (int)reference[c]);
                    if (delta > maximum) maximum = delta;
                    if (delta > 2) ++bad;
                }
                if (!visible && (value[0] > 1 || abs((int)value[1] - 128) > 1 || abs((int)value[2] - 128) > 1)) ++bad;
                /* Opaque primaries must remain visible: all-black capture is not a pass. */
                if (visible && column == 7 && value[0] < (channel == 1 ? 100u : channel == 0 ? 32u : 10u)) ++bad;
                if (!dx && !dy) fprintf(output, "%u,%u,%u,%u,%u,%u,%u,%u,%u\n", index, channel, column,
                    value[0], value[1], value[2], reference[0], reference[1], reference[2]);
            }
        }
    }
    fprintf(stderr, "alpha: wbc sample=%u pts=%" PRIu64 " paired_points=600 max_delta=%u bad=%u\n",
        index, (uint64_t)f->u64pts, maximum, bad);
    result = bad ? -1 : 0;
done:
    for (unsigned i = 0; i < 2; ++i) if (map[i]) munmap((void *)map[i], length[i]);
    return result;
}
int bridge_alpha_check(const char *framebuffer, const char *prefix, int shift, int visible) {
    int result = -1, enabled = 0;
    int fb = open(framebuffer, O_RDONLY | O_CLOEXEC);
    int mem = -1;
    FILE *output = NULL;
    struct fb_fix_screeninfo fixed = {0}; struct fb_var_screeninfo mode = {0}, after = {0};
    if (fb < 0 || ioctl(fb, FBIOGET_FSCREENINFO, &fixed) || ioctl(fb, FBIOGET_VSCREENINFO, &mode)) goto done;
    if (!mode.xres || !mode.yres || mode.xres % 8 || mode.yres % 12 || mode.xres / 8 < 32 || mode.yres / 6 < 32 ||
        mode.bits_per_pixel != 32 || mode.red.offset != 16 || mode.green.offset != 8 || mode.blue.offset != 0 ||
        mode.transp.offset != 24 || mode.red.length != 8 || mode.green.length != 8 || mode.blue.length != 8 || mode.transp.length != 8 ||
        (mode.yoffset != 0 && mode.yoffset != mode.yres) || fixed.line_length < mode.xres * 4 || shift < 0 || shift > 16) goto done;
    if (pixels(fb, &fixed, &mode, shift, visible) || ioctl(fb, FBIOGET_VSCREENINFO, &after) || after.yoffset != mode.yoffset) goto done;
    fyVO_PUB_ATTR_S pub = {0};
    if (checked("get_pub_attr", FY_MPI_VO_GetPubAttr(0, &pub))) goto done;
    fprintf(stderr, "alpha: device_background=%08x\n", pub.u32BgColor);
    if (pub.u32BgColor != 0) { fprintf(stderr, "alpha: this paired-color test requires the existing black background\n"); goto done; }
    mem = open("/dev/mem", O_RDONLY | O_SYNC);
    if (mem < 0) goto done;
    /* This firmware's WBC register reads can be zero during capture. Use
     * the driver's allocation report and exclusive diagnostic ownership. */
    FILE *mmz = fopen("/proc/media-mem", "r");
    if (!mmz) goto done;
    char line[1024]; int occupied = 0;
    while (fgets(line, sizeof line, mmz)) if (strstr(line, "name=\"vou-wbc\"")) occupied = 1;
    if (ferror(mmz)) occupied = 1;
    fclose(mmz);
    if (occupied) { fprintf(stderr, "alpha: WBC already allocated\n"); goto done; }
    char path[1024];
    if (snprintf(path, sizeof path, "%s-wbc.csv", prefix) >= (int)sizeof path) goto done;
    output = fopen(path, "wx");
    if (!output) goto done;
    fputs("sample,channel,alpha_index,Y,C0,C1,reference_Y,reference_C0,reference_C1\n", output);
    VO_WBC_SOURCE_S source = {VO_WBC_SOURCE_DEV, 0};
    VO_WBC_ATTR_S attr = {{mode.xres, mode.yres}, PIXEL_FORMAT_YUV_SEMIPLANAR_420, 60};
    if (checked("source", FY_MPI_VO_SetWbcSource(0, &source)) || checked("attr", FY_MPI_VO_SetWbcAttr(0, &attr)) ||
        checked("depth", FY_MPI_VO_SetWbcDepth(0, 2)) || checked("enable", FY_MPI_VO_EnableWbc(0))) goto done;
    enabled = 1;
    for (unsigned i = 0; i < 8; ++i) {
        VIDEO_FRAME_INFO_S frame = {0};
        if (checked("get", FY_MPI_VO_GetWbcFrame(0, &frame, 500))) goto done;
        int sampled = i < 3 ? 0 : sample(mem, &frame, mode.xres, mode.yres, visible, output, i - 3);
        int released = checked("release", FY_MPI_VO_ReleaseWbcFrame(0, &frame));
        if (sampled || released) goto done;
    }
    result = mode.yoffset / mode.yres;
done:
    if (enabled && checked("disable", FY_MPI_VO_DisableWbc(0))) result = -1;
    if (output && fclose(output)) result = -1;
    if (mem >= 0) close(mem);
    if (fb >= 0) close(fb);
    /* Do not close the SDK's shared VO handle. */
    return result;
}
