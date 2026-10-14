/* Send a complete Type-A tap through evdev, including the all-up frame. */
#define _DEFAULT_SOURCE
#include <fcntl.h>
#include <linux/input.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <unistd.h>

static int send_events(int fd, struct input_event *events, size_t count) {
    return write(fd, events, count * sizeof(*events)) == (ssize_t)(count * sizeof(*events)) ? 0 : -1;
}
int main(int argc, char **argv) {
    if (argc != 3) return 2;
    char *end;
    long x = strtol(argv[1], &end, 10);
    if (*end || x < 0 || x > 800) return 2;
    long y = strtol(argv[2], &end, 10);
    if (*end || y < 0 || y > 1280) return 2;
    int fd = open("/dev/input/event1", O_RDWR | O_CLOEXEC);
    unsigned char keys[96] = {0};
    if (fd < 0 || ioctl(fd, EVIOCGKEY(sizeof(keys)), keys) < 0) { perror("evdev"); return 3; }
    if (keys[BTN_TOUCH / 8] & (1 << (BTN_TOUCH % 8))) {
        fprintf(stderr, "physical contact is already down\n"); return 4;
    }
    struct input_event down[] = {
        {.type=EV_KEY, .code=BTN_TOUCH, .value=1},
        {.type=EV_ABS, .code=ABS_MT_TRACKING_ID, .value=7},
        {.type=EV_ABS, .code=ABS_MT_POSITION_X, .value=x},
        {.type=EV_ABS, .code=ABS_MT_POSITION_Y, .value=y},
        {.type=EV_SYN, .code=SYN_MT_REPORT}, {.type=EV_SYN, .code=SYN_REPORT},
    };
    struct input_event up[] = {
        {.type=EV_KEY, .code=BTN_TOUCH},
        {.type=EV_SYN, .code=SYN_MT_REPORT}, {.type=EV_SYN, .code=SYN_REPORT},
    };
    int pressed = send_events(fd, down, sizeof(down) / sizeof(*down));
    usleep(80000);
    int released = send_events(fd, up, sizeof(up) / sizeof(*up));
    close(fd);
    if (pressed || released) { perror("write touch frame"); return 5; }
    printf("Type-A tap raw=(%ld,%ld) released\n", x, y);
    return 0;
}
