#include <errno.h>
#include <fcntl.h>
#include <linux/fb.h>
#include <linux/input.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <unistd.h>

int main(void) {
    struct fb_fix_screeninfo f = {0};
    struct fb_var_screeninfo v = {0};
    int fd = open("/dev/fb0", O_RDONLY);
    if (fd < 0 || ioctl(fd, FBIOGET_FSCREENINFO, &f) ||
        ioctl(fd, FBIOGET_VSCREENINFO, &v)) {
        perror("framebuffer");
        return 1;
    }
    printf("fixed size=%zu id=%.*s physical=%lx bytes=%u type=%u visual=%u "
           "stride=%u pan=%u,%u wrap=%u accel=%u\n",
           sizeof(f), 16, f.id, f.smem_start, f.smem_len, f.type, f.visual,
           f.line_length, f.xpanstep, f.ypanstep, f.ywrapstep, f.accel);
    printf("variable size=%zu visible=%ux%u virtual=%ux%u offset=%u,%u bpp=%u "
           "rgba=%u:%u,%u:%u,%u:%u,%u:%u nonstd=%u rotate=%u vmode=%u\n",
           sizeof(v), v.xres, v.yres, v.xres_virtual, v.yres_virtual,
           v.xoffset, v.yoffset, v.bits_per_pixel, v.red.offset, v.red.length,
           v.green.offset, v.green.length, v.blue.offset, v.blue.length,
           v.transp.offset, v.transp.length, v.nonstd, v.rotate, v.vmode);
    close(fd);
    fd = open("/dev/input/event2", O_RDONLY);
    if (fd < 0) { perror("input"); return 1; }
    for (int axis = 0; axis <= ABS_MAX; ++axis) {
        struct input_absinfo a = {0};
        if (ioctl(fd, EVIOCGABS(axis), &a) == 0 && a.maximum != a.minimum)
            printf("axis=%d min=%d max=%d value=%d\n",
                   axis, a.minimum, a.maximum, a.value);
    }
    close(fd);
    return 0;
}
