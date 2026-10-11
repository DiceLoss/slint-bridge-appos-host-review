/* Temporary visible test pattern, with both display pages restored byte-for-byte.
 * Invoke with the UI stopped, from run-device.sh's pattern mode.
 */
#define _POSIX_C_SOURCE 200809L
#include <fcntl.h>
#include <linux/fb.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>
#include "recovery.h"

static void settle(void) { struct timespec t = {0, 100000000}; nanosleep(&t, NULL); }
static int present(int fd, const struct fb_var_screeninfo *original, unsigned offset) {
    struct fb_var_screeninfo v = *original;
    v.yoffset = offset;
    if (ioctl(fd, FBIOPUT_VSCREENINFO, &v) || ioctl(fd, FBIOGET_VSCREENINFO, &v)) return 1;
    settle();
    return v.yoffset != offset;
}
int main(int argc, char **argv) {
    if (argc != 3 || (strcmp(argv[1], "white") && strcmp(argv[1], "split"))) return 2;
    const char *owner = getenv("NX5_STOPPED_OWNER");
    if (!owner) return 2;
    char path[96], line[256]; snprintf(path, sizeof path, "/proc/%s/status", owner);
    FILE *s = fopen(path, "r"); int stopped = 0;
    if (s) { while (fgets(line, sizeof line, s)) if (!strncmp(line, "State:", 6) && strchr(line, 'T')) stopped = 1; fclose(s); }
    if (!stopped) return 2;
    int fd = open("/dev/fb0", O_RDWR);
    struct fb_fix_screeninfo f = {0}; struct fb_var_screeninfo v = {0};
    if (fd < 0 || ioctl(fd, FBIOGET_FSCREENINFO, &f) || ioctl(fd, FBIOGET_VSCREENINFO, &v)) return 3;
    size_t page = 4096u * 600u;
    if (strncmp(f.id, "fyfb", 4) || v.xres != 1024 || v.yres != 600 ||
        f.line_length != 4096 || v.bits_per_pixel != 32 || f.smem_len < page * 2 ||
        (v.yoffset != 0 && v.yoffset != 600)) return 3;
    uint8_t *map = mmap(NULL, page * 2, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    uint8_t *backup = malloc(page * 2);
    if (map == MAP_FAILED || !backup) return 3;
    memcpy(backup, map, page * 2);
    unsigned original = v.yoffset / 600, back = 1 - original;
    uint32_t *pixels = (uint32_t *)(map + back * page);
    for (unsigned y = 0; y < 600; ++y)
        for (unsigned x = 0; x < 1024; ++x)
            pixels[y * 1024 + x] = !strcmp(argv[1], "split") && y >= 300 ? 0xff000000u : 0xffffffffu;
    __sync_synchronize();
    if (present(fd, &v, back * 600)) {
        /* Do not write either potentially active page after an uncertain flip. */
        preserve_backup(backup, page * 2);
        return 4;
    }
    printf("pattern=%s page=%u original=%u\n", argv[1], back, original); fflush(stdout);
    pid_t child = fork();
    int status = -1;
    if (!child) { execl("./wbc-capture", "wbc-capture", argv[2], (char *)NULL); _exit(127); }
    if (child > 0) waitpid(child, &status, 0);
    memcpy(map + original * page, backup + original * page, page);
    __sync_synchronize();
    if (present(fd, &v, original * 600)) { preserve_backup(backup, page * 2); return 5; }
    memcpy(map + back * page, backup + back * page, page);
    __sync_synchronize();
    int same = !memcmp(map, backup, page * 2);
    printf("pattern_restore pixels_equal=%d original=%u\n", same, original);
    free(backup); munmap(map, page * 2); close(fd);
    return !same || status == -1 || !WIFEXITED(status) || WEXITSTATUS(status) ? 1 : 0;
}
