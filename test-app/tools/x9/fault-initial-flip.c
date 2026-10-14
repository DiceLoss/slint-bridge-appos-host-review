/* LD_PRELOAD fault probe: reject flip before queueing it and log console mode. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <unistd.h>

int ioctl(int fd, unsigned long request, ...) {
    va_list args;
    va_start(args, request);
    uintptr_t argument = request == 0x641e ? 0 : request == 0x4b3a
        ? (uintptr_t)va_arg(args, int) : (uintptr_t)va_arg(args, void *);
    va_end(args);
    int (*real_ioctl)(int, unsigned long, ...) = dlsym(RTLD_NEXT, "ioctl");
    const char *path = getenv("SLINT_BRIDGE_X9_FAULT_LOG");
    if (path && (request == 0xc01864b0 || request == 0x4b3a)) {
        int log = open(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0644);
        const char *line = request == 0xc01864b0 ? "FLIP_EIO\n" : argument == 0 ? "KD_TEXT\n" : "KD_GRAPHICS\n";
        size_t length = request == 0xc01864b0 ? 9 : argument == 0 ? 8 : 12;
        if (log >= 0) { ssize_t count = write(log, line, length); (void)count; close(log); }
        if (request == 0xc01864b0) { errno = EIO; return -1; }
    }
    return real_ioctl(fd, request, argument);
}
