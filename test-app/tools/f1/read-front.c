/* Diagnostic readback only: select a static page through native Slint MCP first.
 * Streams the existing front page in 4 KiB chunks to SD; never writes pixels,
 * requests PAN/VSYNC, or loads an SDK. Offset readback is not a scanout fence.
 */
#define _XOPEN_SOURCE 700
#include <fcntl.h>
#include <linux/fb.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

int main(int argc, char **argv)
{
    if (argc != 2) return 2;
    int fb = open("/dev/fb0", O_RDONLY | O_CLOEXEC);
    struct fb_var_screeninfo before = {0}, after = {0};
    struct fb_fix_screeninfo fixed = {0};
    if (fb < 0 || ioctl(fb, FBIOGET_VSCREENINFO, &before) ||
        ioctl(fb, FBIOGET_FSCREENINFO, &fixed)) { perror("framebuffer"); return 3; }
    if (before.xres != 1024 || before.yres != 600 || before.bits_per_pixel != 32 ||
        fixed.line_length != 4096 || fixed.smem_len != 4915200 ||
        before.red.offset != 16 || before.green.offset != 8 || before.blue.offset != 0 ||
        before.transp.offset != 24 || (before.yoffset != 0 && before.yoffset != 600)) return 4;
    int out = open(argv[1], O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0644);
    if (out < 0) { perror("output"); return 5; }
    unsigned char row[4096], samples[3][4] = {{0}};
    for (unsigned int y = 0; y < 600; y++) {
        off_t offset = (off_t)(before.yoffset + y) * 4096;
        if (pread(fb, row, sizeof(row), offset) != sizeof(row) ||
            write(out, row, sizeof(row)) != sizeof(row)) { perror("copy"); return 6; }
        if (y == 0) { memcpy(samples[0], row, 4); memcpy(samples[1], row + 4000, 4); }
        if (y == 580) memcpy(samples[2], row + 20, 4);
    }
    if (ioctl(fb, FBIOGET_VSCREENINFO, &after) || before.yoffset != after.yoffset) {
        fprintf(stderr, "front page changed: readback is invalid\n"); return 7;
    }
    if (close(out) || close(fb)) { perror("close"); return 8; }
    printf("{\"width\":1024,\"height\":600,\"stride\":4096,\"yoffset\":%u,\"bytes\":2457600}\n", before.yoffset);
    for (unsigned int i = 0; i < 3; i++) printf("sample%u_bgra=%02x%02x%02x%02x\n", i, samples[i][0], samples[i][1], samples[i][2], samples[i][3]);
    return 0;
}
