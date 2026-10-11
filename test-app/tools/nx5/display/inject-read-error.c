/* Diagnostic LD_PRELOAD shim: inject one evdev read error after a trigger file
 * appears. Never loads the SDK or changes the input device or display driver. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static ssize_t (*next_read)(int, void *, size_t);
static const char *input_path, *trigger_path;
static int injected;

__attribute__((constructor)) static void initialize(void) {
    next_read = dlsym(RTLD_NEXT, "read");
    input_path = getenv("NX5_REGRESSION_INPUT");
    trigger_path = getenv("NX5_REGRESSION_TRIGGER");
}

ssize_t read(int fd, void *buffer, size_t count) {
    if (!next_read) { errno = ENOSYS; return -1; }
    if (input_path && trigger_path && access(trigger_path, F_OK) == 0) {
        char link[64], path[256];
        int length = snprintf(link, sizeof(link), "/proc/self/fd/%d", fd);
        if (length > 0 && (size_t)length < sizeof(link)) {
            ssize_t n = readlink(link, path, sizeof(path) - 1);
            if (n >= 0) {
                path[n] = '\0';
                if (strcmp(path, input_path) == 0 &&
                    __sync_bool_compare_and_swap(&injected, 0, 1)) {
                    static const char message[] = "nx5-regression: injected evdev read EIO\n";
                    ssize_t logged = write(STDERR_FILENO, message, sizeof(message) - 1);
                    (void)logged;
                    errno = EIO;
                    return -1;
                }
            }
        }
    }
    return next_read(fd, buffer, count);
}
