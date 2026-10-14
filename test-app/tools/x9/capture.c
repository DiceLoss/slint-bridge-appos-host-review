/* Read one stable X9 front page; never write pixels or load a graphics SDK. */
#define _XOPEN_SOURCE 700
#include <errno.h>
#include <fcntl.h>
#include <linux/fb.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    int fb = open("/dev/fb0", O_RDONLY | O_CLOEXEC);
    struct fb_var_screeninfo before, after;
    struct fb_fix_screeninfo fixed;
    if (fb < 0 || ioctl(fb, FBIOGET_VSCREENINFO, &before) ||
        ioctl(fb, FBIOGET_FSCREENINFO, &fixed)) { perror("framebuffer"); return 3; }
    if (before.xres != 800 || before.yres != 1280 || before.bits_per_pixel != 32 ||
        fixed.line_length != 3200 || (before.yoffset != 0 && before.yoffset != 1280)) return 4;
    size_t bytes = 3200 * 1280;
    unsigned char *pixels = malloc(bytes);
    if (!pixels) return 5;
    ssize_t count = pread(fb, pixels, bytes, before.yoffset * 3200);
    if (count != (ssize_t)bytes || ioctl(fb, FBIOGET_VSCREENINFO, &after) ||
        before.yoffset != after.yoffset) { fprintf(stderr, "short read or page changed\n"); return 6; }
    int out = open(argv[1], O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0644);
    if (out < 0 || write(out, pixels, bytes) != (ssize_t)bytes || close(out)) { perror("output"); return 7; }
    printf("{\"width\":800,\"height\":1280,\"stride\":3200,\"yoffset\":%u,\"bytes\":%zu}\n", before.yoffset, bytes);
    free(pixels);
    close(fb);
    return 0;
}
