/* A complete single-touch tap through the inspected NX1 GT911 evdev device.
 * Refuses an existing contact; this tests evdev delivery, not the sensor.
 */
#define _DEFAULT_SOURCE
#include <fcntl.h>
#include <linux/input.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

static int events(int fd, struct input_event *data, size_t count) {
    return write(fd, data, count * sizeof(*data)) == (ssize_t)(count * sizeof(*data)) ? 0 : -1;
}

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    char *end;
    long x = strtol(argv[1], &end, 10);
    if (*end || x < 0 || x > 1024) return 2;
    long y = strtol(argv[2], &end, 10);
    if (*end || y < 0 || y > 600) return 2;
    int fd = open("/dev/input/event2", O_RDWR | O_CLOEXEC);
    char name[64] = {0};
    unsigned char keys[96] = {0};
    struct input_absinfo ax = {0}, ay = {0};
    if (fd < 0 || ioctl(fd, EVIOCGNAME(sizeof(name)), name) < 0 ||
        ioctl(fd, EVIOCGKEY(sizeof(keys)), keys) < 0 ||
        ioctl(fd, EVIOCGABS(ABS_X), &ax) < 0 || ioctl(fd, EVIOCGABS(ABS_Y), &ay) < 0) {
        perror("inspect evdev");
        return 3;
    }
    if (strcmp(name, "gt911_ts") || ax.minimum || ay.minimum ||
        ax.maximum != 1024 || ay.maximum != 600) {
        fprintf(stderr, "unverified touchscreen\n");
        close(fd);
        return 4;
    }
    if (keys[BTN_TOUCH / 8] & (1 << (BTN_TOUCH % 8))) {
        fprintf(stderr, "physical contact is already down\n");
        close(fd);
        return 4;
    }
    struct input_event down[] = {
        {.type=EV_ABS, .code=ABS_X, .value=x},
        {.type=EV_ABS, .code=ABS_Y, .value=y},
        {.type=EV_ABS, .code=ABS_PRESSURE, .value=64},
        {.type=EV_KEY, .code=BTN_TOUCH, .value=1},
        {.type=EV_SYN, .code=SYN_REPORT},
    };
    struct input_event up[] = {
        {.type=EV_ABS, .code=ABS_PRESSURE},
        {.type=EV_KEY, .code=BTN_TOUCH},
        {.type=EV_SYN, .code=SYN_REPORT},
    };
    int pressed = events(fd, down, sizeof(down) / sizeof(*down));
    usleep(80000);
    int released = events(fd, up, sizeof(up) / sizeof(*up));
    close(fd);
    if (pressed || released) { perror("write touch frame"); return 5; }
    printf("NX1 single-touch tap raw=(%ld,%ld), released\n", x, y);
    return 0;
}
