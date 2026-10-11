/* Ordinary Linux wait timing; no SDK, framebuffer or hardware-register access. */
#define _GNU_SOURCE
#include <errno.h>
#include <inttypes.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/timerfd.h>
#include <time.h>
#include <unistd.h>

static uint64_t now_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000 + t.tv_nsec;
}
int main(int argc, char **argv) {
    printf("clock_ticks=%ld\n", sysconf(_SC_CLK_TCK));
    int fd = timerfd_create(CLOCK_MONOTONIC, TFD_CLOEXEC);
    if (fd < 0) { perror("timerfd_create"); return 1; }
    const unsigned durations[] = {1000, 3334, 5000, 8000, 13000, 16667, 33334};
    for (unsigned mode = argc > 1 ? (unsigned)atoi(argv[1]) : 0; mode < 5; ++mode) {
        for (unsigned n = 0; n < sizeof durations / sizeof durations[0]; ++n) {
            unsigned us = durations[n];
            for (unsigned sample = 0; sample < 12; ++sample) {
                struct timespec delay = {0, us * 1000};
                uint64_t begin = now_ns();
                int rc;
                switch (mode) {
                    case 0: rc = poll(NULL, 0, (us + 999) / 1000); break;
                    case 1: rc = ppoll(NULL, 0, &delay, NULL); break;
                    case 2: rc = nanosleep(&delay, NULL); break;
                    case 3: {
                        struct itimerspec timer = {.it_value = delay};
                        rc = timerfd_settime(fd, 0, &timer, NULL);
                        uint64_t count;
                        if (rc == 0) rc = read(fd, &count, sizeof count) == sizeof count ? 0 : -1;
                        break;
                    }
                    default: {
                        uint64_t deadline = begin + (uint64_t)us * 1000, current;
                        rc = 0;
                        while ((current = now_ns()) < deadline && rc == 0) {
                            uint64_t remain = deadline-current;
                            struct timespec early = {0, remain > 10000000 ? remain-10000000 : 0};
                            rc = ppoll(NULL, 0, &early, NULL);
                        }
                    }
                }
                uint64_t end = now_ns();
                if (rc < 0) { perror("wait"); return 1; }
                printf("mode=%u requested_us=%u sample=%u elapsed_us=%.3f\n", mode, us, sample, (end-begin)/1000.);
            }
        }
    }
    close(fd);
    return 0;
}
