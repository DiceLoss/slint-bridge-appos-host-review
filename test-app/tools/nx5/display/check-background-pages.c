/* Read-only checker loaded by background-holes --verify after presentation
 * drains. No SDK, VO handle, DMA operation, or framebuffer writes. */
#define _POSIX_C_SOURCE 200809L
#include <fcntl.h>
#include <linux/fb.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <unistd.h>

static unsigned compare(const unsigned char *actual, const unsigned char *rgba,
                        unsigned width, unsigned y, unsigned *maximum) {
    unsigned bad = 0;
    for (unsigned x = 0; x < width; ++x) {
        const unsigned char *reference = rgba + ((size_t)y * width + x) * 4;
        const unsigned char *p = actual + x * 4;
        int mismatch = p[3] != (reference[3] + 1u) / 2u;
        for (unsigned c = 0; c < 3; ++c) {
            unsigned delta = abs((int)p[2 - c] - reference[c]);
            if (delta > *maximum) *maximum = delta;
            if (delta > 1) mismatch = 1;
        }
        bad += mismatch;
    }
    return bad;
}

int bridge_background_check(const char *path, const unsigned char *current,
                            const unsigned char *previous, unsigned width, unsigned height) {
    int result = -1;
    int fd = open(path, O_RDONLY | O_CLOEXEC);
    unsigned char *row = NULL;
    struct fb_fix_screeninfo fixed = {0};
    struct fb_var_screeninfo mode = {0}, after = {0};
    if (fd < 0 || ioctl(fd, FBIOGET_FSCREENINFO, &fixed) || ioctl(fd, FBIOGET_VSCREENINFO, &mode)) goto done;
    if (!width || !height || width != mode.xres || height != mode.yres ||
        mode.bits_per_pixel != 32 || mode.xoffset || mode.yres_virtual != height * 2 ||
        mode.red.offset != 16 || mode.green.offset != 8 || mode.blue.offset != 0 || mode.transp.offset != 24 ||
        mode.red.length != 8 || mode.green.length != 8 || mode.blue.length != 8 || mode.transp.length != 8 ||
        (mode.yoffset != 0 && mode.yoffset != height) || fixed.line_length < width * 4 ||
        (uint64_t)fixed.line_length * height * 2 > fixed.smem_len) goto done;
    row = malloc((size_t)width * 4);
    if (!row) goto done;
    unsigned front = mode.yoffset / height;
    for (unsigned page = 0; page < 2; ++page) {
        if (page != front && !previous) continue;
        unsigned bad_current = 0, bad_previous = 0, max_current = 0, max_previous = 0;
        for (unsigned y = 0; y < height; ++y) {
            off_t offset = (off_t)(page * height + y) * fixed.line_length;
            if (pread(fd, row, width * 4, offset) != (ssize_t)width * 4) goto done;
            bad_current += compare(row, current, width, y, &max_current);
            if (previous) bad_previous += compare(row, previous, width, y, &max_previous);
        }
        int valid = page == front ? !bad_current : (!bad_previous || !bad_current);
        fprintf(stderr, "background-pages: page=%u front=%u pixels=%u current_bad=%u previous_bad=%u current_rgb_max=%u previous_rgb_max=%u valid=%d\n",
                page, front, width * height, bad_current, bad_previous, max_current, max_previous, valid);
        if (!valid) goto done;
    }
    if (ioctl(fd, FBIOGET_VSCREENINFO, &after) || after.yoffset != mode.yoffset) goto done;
    result = front;
done:
    free(row);
    if (fd >= 0) close(fd);
    return result;
}
