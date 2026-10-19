/* Read-only F1 inspection: no mmap, PAN, VSYNC wait, SDK load or input grab.
 * See docs/f1-display.md for firmware scope and invocation prerequisites.
 */
#include <fcntl.h>
#include <linux/fb.h>
#include <linux/input.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <unistd.h>

static int inspect_fb(void)
{
    int fd = open("/dev/fb0", O_RDONLY | O_CLOEXEC);
    struct fb_fix_screeninfo fix = {0};
    struct fb_var_screeninfo var = {0};
    if (fd < 0) {
        perror("open fb0");
        return 1;
    }
    if (ioctl(fd, FBIOGET_FSCREENINFO, &fix) < 0 ||
        ioctl(fd, FBIOGET_VSCREENINFO, &var) < 0) {
        perror("FBIOGET");
        close(fd);
        return 1;
    }
    printf("fb id=%.16s type=%u visual=%u smem_start=%#lx smem_len=%u stride=%u\n",
           fix.id, fix.type, fix.visual, fix.smem_start, fix.smem_len,
           fix.line_length);
    printf("fb pan_step=%u,%u wrap_step=%u accel=%u\n",
           fix.xpanstep, fix.ypanstep, fix.ywrapstep, fix.accel);
    printf("fb size=%ux%u virtual=%ux%u offset=%u,%u bpp=%u rotate=%u\n",
           var.xres, var.yres, var.xres_virtual, var.yres_virtual,
           var.xoffset, var.yoffset, var.bits_per_pixel, var.rotate);
    printf("fb fields=offset,length,msb_right red=%u,%u,%u green=%u,%u,%u "
           "blue=%u,%u,%u alpha=%u,%u,%u\n",
           var.red.offset, var.red.length, var.red.msb_right,
           var.green.offset, var.green.length, var.green.msb_right,
           var.blue.offset, var.blue.length, var.blue.msb_right,
           var.transp.offset, var.transp.length, var.transp.msb_right);
    printf("fb pixclock_ps=%u margins_left_right_upper_lower=%u,%u,%u,%u "
           "sync_len=%u,%u sync=%u vmode=%u nonstd=%u grayscale=%u\n",
           var.pixclock, var.left_margin, var.right_margin,
           var.upper_margin, var.lower_margin, var.hsync_len, var.vsync_len,
           var.sync, var.vmode, var.nonstd, var.grayscale);
    return close(fd) < 0;
}

static int inspect_input(void)
{
    int fd = open("/dev/input/event0", O_RDONLY | O_NONBLOCK | O_CLOEXEC);
    char name[256] = {0};
    unsigned char bits[(ABS_MAX + 8) / 8] = {0};
    if (fd < 0) {
        perror("open event0");
        return 1;
    }
    if (ioctl(fd, EVIOCGNAME(sizeof(name)), name) < 0 ||
        ioctl(fd, EVIOCGBIT(EV_ABS, sizeof(bits)), bits) < 0) {
        perror("EVIOCGNAME/EVIOCGBIT");
        close(fd);
        return 1;
    }
    printf("input name=%s\n", name);
    for (unsigned int axis = 0; axis <= ABS_MAX; ++axis) {
        if (!(bits[axis / 8] & (1u << (axis % 8))))
            continue;
        struct input_absinfo info = {0};
        if (ioctl(fd, EVIOCGABS(axis), &info) < 0) {
            perror("EVIOCGABS");
            close(fd);
            return 1;
        }
        printf("input abs=%u value=%d min=%d max=%d fuzz=%d flat=%d resolution=%d\n",
               axis, info.value, info.minimum, info.maximum,
               info.fuzz, info.flat, info.resolution);
    }
    return close(fd) < 0;
}

int main(void)
{
    int fb_result = inspect_fb();
    int input_result = inspect_input();
    return fb_result || input_result;
}
