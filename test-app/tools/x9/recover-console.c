/* After the display owner exits, fence scanout before restoring fbcon writes. */
#define _DEFAULT_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <poll.h>
#include <sys/file.h>
#include <sys/ioctl.h>
#include <sys/utsname.h>
#include <linux/fb.h>
#include <linux/kd.h>
#include <drm/drm.h>
#include <unistd.h>

int main(void) {
    struct utsname kernel;
    if (uname(&kernel) || strcmp(kernel.release, "3.10.100") ||
        strcmp(kernel.version, "#452 SMP PREEMPT Mon Dec 12 11:32:50 CST 2022")) return 2;
    int lock = open("/run/slint-bridge-x9.lock", O_RDWR | O_CREAT | O_CLOEXEC, 0600);
    if (lock < 0 || flock(lock, LOCK_EX | LOCK_NB)) { perror("display owner"); return 3; }
    int fb = open("/dev/fb0", O_RDWR | O_CLOEXEC);
    int drm = open("/dev/dri/card0", O_RDWR | O_NONBLOCK | O_CLOEXEC);
    if (fb < 0 || drm < 0 || flock(fb, LOCK_EX | LOCK_NB) || ioctl(drm, DRM_IOCTL_SET_MASTER, 0)) {
        perror("display owner or open"); return 3;
    }
    char name[32] = {0};
    struct drm_version version = {.name_len=sizeof(name), .name=name};
    if (ioctl(drm, DRM_IOCTL_VERSION, &version) || version.name_len != 5 ||
        memcmp(name, "lombo", 5) || version.version_major != 1 ||
        version.version_minor != 0 || version.version_patchlevel != 0) return 4;
    uint32_t id = 0;
    struct drm_mode_card_res resources = {.crtc_id_ptr=(uintptr_t)&id, .count_crtcs=1};
    if (ioctl(drm, DRM_IOCTL_MODE_GETRESOURCES, &resources) || resources.count_crtcs != 1) return 5;
    struct drm_mode_crtc crtc = {.crtc_id=id};
    struct fb_var_screeninfo v;
    if (ioctl(drm, DRM_IOCTL_MODE_GETCRTC, &crtc) || ioctl(fb, FBIOGET_VSCREENINFO, &v) ||
        !crtc.fb_id || crtc.mode_valid != 1 || crtc.mode.hdisplay != 800 || crtc.mode.vdisplay != 1280 ||
        crtc.x != 0 || (crtc.y != 0 && crtc.y != 1280) || v.xres != 800 || v.yres != 1280 ||
        v.yres_virtual != 2560 || v.bits_per_pixel != 32) return 6;
    struct drm_mode_crtc_page_flip flip = {
        .crtc_id=id, .fb_id=crtc.fb_id, .flags=DRM_MODE_PAGE_FLIP_EVENT, .user_data=0x58395245434f5645ULL,
    };
    if (ioctl(drm, DRM_IOCTL_MODE_PAGE_FLIP, &flip)) { perror("flip"); return 7; }
    struct pollfd p = {.fd=drm, .events=POLLIN};
    if (poll(&p, 1, 2000) != 1 || p.revents != POLLIN) { fprintf(stderr,"completion unknown\n"); return 8; }
    unsigned char bytes[4096];
    ssize_t count = read(drm, bytes, sizeof(bytes));
    struct drm_event_vblank event;
    if (count != sizeof(event)) return 9;
    memcpy(&event, bytes, sizeof(event));
    if (event.base.type != DRM_EVENT_FLIP_COMPLETE || event.base.length != sizeof(event) ||
        event.user_data != flip.user_data) return 10;
    int console = open("/dev/tty0", O_RDWR | O_CLOEXEC);
    if (console < 0 || ioctl(console, KDSETMODE, KD_TEXT)) { perror("console"); return 11; }
    puts("matched X9 completion; restored text console; no application pixel writes");
    close(console); close(drm); close(fb); close(lock);
    return 0;
}
