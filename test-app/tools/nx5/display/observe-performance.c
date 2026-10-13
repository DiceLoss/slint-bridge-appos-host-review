/* Read-only resource/evdev observer. No SDK, framebuffer or input injection. */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <linux/input.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <time.h>
#include <unistd.h>

static int64_t us(clockid_t clock) {
    struct timespec t;
    if (clock_gettime(clock, &t)) { perror("clock_gettime"); exit(2); }
    return (int64_t)t.tv_sec * 1000000 + t.tv_nsec / 1000;
}

static long field(const char *path, const char *key) {
    FILE *file = fopen(path, "r");
    if (!file) return -1;
    char line[512];
    long value = -1;
    while (fgets(line, sizeof line, file)) {
        if (!strncmp(line, key, strlen(key))) {
            if (sscanf(line + strlen(key), "%ld", &value) != 1) value = -1;
            break;
        }
    }
    fclose(file);
    return value;
}

static long cpu_ticks(int pid) {
    char path[64], line[2048];
    snprintf(path, sizeof path, "/proc/%d/stat", pid);
    FILE *file = fopen(path, "r");
    if (!file) return -1;
    char *read = fgets(line, sizeof line, file);
    fclose(file);
    char *cursor = read ? strrchr(line, ')') : NULL;
    if (!cursor) return -1;
    long ticks = 0;
    char *state = NULL;
    char *token = strtok_r(cursor + 1, " ", &state);
    for (int column = 3; column <= 15; column++) {
        if (!token) return -1;
        if (column == 14 || column == 15) ticks += strtol(token, NULL, 10);
        token = strtok_r(NULL, " ", &state);
    }
    return ticks;
}

static int sample(int pid, const char *phase_path) {
    char phase[64] = "unknown", path[64];
    FILE *file = fopen(phase_path, "r");
    if (file) { if (fscanf(file, "%63s", phase) != 1) strcpy(phase, "unknown"); fclose(file); }
    snprintf(path, sizeof path, "/proc/%d/status", pid);
    long rss = field(path, "VmRSS:"), hwm = field(path, "VmHWM:");
    long available = field("/proc/meminfo", "MemAvailable:"), ticks = cpu_ticks(pid);
    if (rss < 0 || hwm < 0 || available < 0 || ticks < 0) return -1;
    printf("sample mono_us=%" PRId64 " realtime_us=%" PRId64
           " phase=%s rss_kib=%ld hwm_kib=%ld available_kib=%ld cpu_ticks=%ld\n",
           us(CLOCK_MONOTONIC), us(CLOCK_REALTIME), phase, rss, hwm, available, ticks);
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 5) {
        fprintf(stderr, "usage: %s EVDEV PID SECONDS PHASE_FILE\n", argv[0]);
        return 2;
    }
    int pid = atoi(argv[2]), seconds = atoi(argv[3]);
    if (pid <= 0 || seconds < 1 || seconds > 1800) return 2;
    int fd = open(argv[1], O_RDONLY | O_NONBLOCK | O_CLOEXEC);
    if (fd < 0) { perror("open evdev"); return 2; }
    int clock = CLOCK_REALTIME;
    if (ioctl(fd, EVIOCSCLOCKID, &clock)) { perror("EVIOCSCLOCKID"); return 2; }
    char name[128] = {0};
    if (ioctl(fd, EVIOCGNAME(sizeof name), name) < 0) { perror("EVIOCGNAME"); return 2; }
    setvbuf(stdout, NULL, _IOLBF, 0);
    printf("ready name=%s clock=realtime pid=%d ticks_per_second=%ld\n", name, pid, sysconf(_SC_CLK_TCK));
    int64_t end = us(CLOCK_MONOTONIC) + seconds * INT64_C(1000000);
    int64_t next = 0;
    unsigned count = 0;
    while (us(CLOCK_MONOTONIC) < end) {
        if (us(CLOCK_MONOTONIC) >= next) {
            if (sample(pid, argv[4])) { fprintf(stderr, "resource sample failed\n"); return 2; }
            next = us(CLOCK_MONOTONIC) + 1000000;
        }
        struct pollfd p = {fd, POLLIN, 0};
        int ready = poll(&p, 1, 100);
        if (ready < 0) { if (errno == EINTR) continue; perror("poll"); return 2; }
        if (!ready) continue;
        if (p.revents & (POLLERR | POLLHUP | POLLNVAL)) return 2;
        struct input_event events[32];
        ssize_t n = read(fd, events, sizeof events);
        if (n < 0 && (errno == EAGAIN || errno == EINTR)) continue;
        if (n <= 0 || n % sizeof events[0]) { fprintf(stderr, "incomplete evdev read\n"); return 2; }
        int64_t received = us(CLOCK_REALTIME);
        for (size_t i = 0; i < (size_t)n / sizeof events[0]; i++) {
            struct input_event *e = &events[i];
            printf("event kernel_us=%" PRId64 " read_us=%" PRId64 " type=%u code=%u value=%d\n",
                (int64_t)e->time.tv_sec * 1000000 + e->time.tv_usec,
                received, e->type, e->code, e->value);
            count++;
        }
    }
    printf("finished events=%u\n", count);
    close(fd);
    return 0;
}
