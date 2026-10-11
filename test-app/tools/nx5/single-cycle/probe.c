/* Matched #233 VOU state observer. Kernel RAM/MMIO are mapped read-only.
 * The only display operation is the existing blocking PAN of two unchanged
 * pages. The original offset is restored before successful exit.
 * Internal driver state is diagnostic evidence, never a retirement fence.
 */
#define _GNU_SOURCE
#define _FILE_OFFSET_BITS 64
#include <dirent.h>
#include <fcntl.h>
#include <inttypes.h>
#include <linux/fb.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

static int mem, kmem;
static uint32_t lld, ufl;
static volatile const uint32_t *regs;
static struct { uint32_t page; volatile const uint32_t *words; } maps[64];
static unsigned map_count;
static atomic_int running;
struct sample {
    uint64_t ns, irq;
    uint32_t update, address, active, pending, queued, flags, consistent;
};
static struct sample samples[8192];
static unsigned sample_count, dropped;

static void die(const char *why) { perror(why); exit(2); }
static uint64_t now_ns(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000u + t.tv_nsec;
}
static void delay_us(unsigned us) {
    uint64_t end = now_ns() + (uint64_t)us * 1000;
    while (now_ns() < end) {}
}
static uint32_t phys32(uint32_t address) {
    if (address < 0x40000000u || address >= 0x46000000u || (address & 3)) {
        fprintf(stderr, "unexpected RAM address %08x\n", address); exit(2);
    }
    uint32_t page = address & ~4095u;
    unsigned i;
    for (i = 0; i < map_count; ++i) if (maps[i].page == page) break;
    if (i == map_count) {
        if (map_count == 64) { fputs("mapping limit\n", stderr); exit(2); }
        void *p = mmap(NULL, 4096, PROT_READ, MAP_SHARED, mem, page);
        if (p == MAP_FAILED) die("map kernel RAM read-only");
        maps[i].page = page; maps[i].words = p; ++map_count;
    }
    return maps[i].words[(address & 4095) / 4];
}
static uint32_t virt32(uint32_t va) {
    if (va < 0xbf000000u || (va & 3)) { fputs("unexpected virtual address\n", stderr); exit(2); }
    uint32_t l1 = phys32(0x40004000u + (va >> 20) * 4), pa;
    if ((l1 & 3) == 2) pa = (l1 & 0xfff00000u) | (va & 0xfffffu);
    else if ((l1 & 3) == 1) {
        uint32_t l2 = phys32((l1 & ~1023u) + ((va >> 12) & 255) * 4);
        if (!(l2 & 2)) { fputs("unmapped kernel page\n", stderr); exit(2); }
        pa = (l2 & ~4095u) | (va & 4095);
    } else { fputs("unexpected ARM page table\n", stderr); exit(2); }
    // Module VAs are below PAGE_OFFSET on this ARM kernel. Translate them,
    // then read the cached linear alias via /dev/kmem. Uncached RAM mappings
    // can miss dirty kernel cache lines and are unsuitable for live state.
    uint32_t value;
    if (pread(kmem, &value, sizeof value, (uint64_t)pa + 0x80000000u) != sizeof value)
        die("read cached kernel state");
    return value;
}
static uint64_t irq_count(void) {
    FILE *f = fopen("/proc/interrupts", "r");
    if (!f) die("interrupts");
    char line[256]; uint64_t total = 0;
    while (fgets(line, sizeof line, f)) {
        unsigned id; unsigned long long a, b;
        if (strstr(line, "vou@34100000") && sscanf(line, "%u: %llu %llu", &id, &a, &b) == 3)
            total += a + b;
    }
    fclose(f); return total;
}
static struct sample snapshot(void) {
    struct sample s = {0};
    s.irq = irq_count();
    uint32_t a = virt32(lld + 0x199c), p = virt32(lld + 0x19a0);
    s.update = regs[2]; s.address = regs[0x134 / 4];
    s.active = a ? virt32(a + 0x18) : 0;
    s.pending = p ? virt32(p + 0x18) : 0;
    s.queued = virt32(ufl + 0xb8);
    s.flags = virt32(lld + 0xedc) & 255;
    s.consistent = a == virt32(lld + 0x199c) && p == virt32(lld + 0x19a0)
        && s.update == regs[2] && s.address == regs[0x134 / 4];
    s.ns = now_ns(); return s;
}
static void *observe(void *unused) {
    (void)unused;
    cpu_set_t cpus; CPU_ZERO(&cpus); CPU_SET(1, &cpus);
    if (pthread_setaffinity_np(pthread_self(), sizeof cpus, &cpus)) die("sample affinity");
    struct sample previous = {0};
    while (atomic_load(&running)) {
        struct sample s = snapshot();
        if (!sample_count || s.irq != previous.irq || s.update != previous.update ||
            s.address != previous.address || s.active != previous.active ||
            s.pending != previous.pending || s.queued != previous.queued ||
            s.flags != previous.flags || s.consistent != previous.consistent) {
            if (sample_count < 8192) samples[sample_count++] = s; else ++dropped;
            previous = s;
        }
        delay_us(100);
    }
    return NULL;
}
static void require_profile(void) {
    static const char expected[] = "0dacc9ce6e893d86fc261c52c29bb11955b28130643b7f8a0c678619768449fb";
    FILE *f = popen("sha256sum /system/ko/vou.ko", "r"); char hash[65];
    if (!f || fscanf(f, "%64s", hash) != 1 || strcmp(hash, expected)) {
        fputs("unrecognized VOU; offsets are firmware-specific\n", stderr); exit(2);
    }
    if (pclose(f)) exit(2);
    const char *pid = getenv("NX5_STOPPED_OWNER");
    if (!pid || !*pid || strspn(pid, "0123456789") != strlen(pid)) exit(2);
    char path[128], state[256]; snprintf(path, sizeof path, "/proc/%s/task", pid);
    DIR *dir = opendir(path); if (!dir) die("owner task directory");
    struct dirent *entry; unsigned tasks = 0;
    while ((entry = readdir(dir))) {
        if (entry->d_name[0] == '.') continue;
        snprintf(path, sizeof path, "/proc/%s/task/%.20s/status", pid, entry->d_name);
        f = fopen(path, "r"); int stopped = 0;
        if (f) { while (fgets(state, sizeof state, f))
            if (!strncmp(state, "State:", 6) && strchr(state, 'T')) stopped = 1;
            fclose(f);
        }
        if (!stopped) { fputs("every display-owner thread must be stopped\n", stderr); exit(2); }
        ++tasks;
    }
    closedir(dir); if (!tasks) exit(2);
}
int main(void) {
    require_profile();
    cpu_set_t cpus; CPU_ZERO(&cpus); CPU_SET(0, &cpus);
    if (sched_setaffinity(0, sizeof cpus, &cpus)) die("submit affinity");
    mem = open("/dev/mem", O_RDONLY | O_SYNC); if (mem < 0) die("mem");
    kmem = open("/dev/kmem", O_RDONLY); if (kmem < 0) die("kmem");
    if ((phys32(0x40007000) & 0xfff00003u) != 0x40000002u) return 2;
    FILE *f = fopen("/sys/module/vou/sections/.bss", "r"); unsigned bss;
    if (!f || fscanf(f, "%x", &bss) != 1) return 2;
    fclose(f);
    lld = virt32(virt32(bss + 0xcdd8) + 0x68) + 0x1c;
    ufl = bss + 0x5878 + 0x6c40;
    regs = mmap(NULL, 4096, PROT_READ, MAP_SHARED, mem, 0x34101000);
    if (regs == MAP_FAILED) die("map DHD read-only");
    int fd = open("/dev/fb0", O_RDWR); if (fd < 0) die("fb0");
    struct fb_fix_screeninfo fix; struct fb_var_screeninfo original, v, actual;
    if (ioctl(fd, FBIOGET_FSCREENINFO, &fix) || ioctl(fd, FBIOGET_VSCREENINFO, &original)) die("layout");
    if (strcmp(fix.id,"fyfb") || original.xres != 1024 || original.yres != 600 ||
        original.yres_virtual != 1200 || original.bits_per_pixel != 32 || fix.line_length != 4096 ||
        (original.yoffset != 0 && original.yoffset != 600)) return 2;
    struct sample initial = snapshot();
    uint32_t shown = fix.smem_start + original.yoffset * fix.line_length;
    if (!initial.consistent || initial.active != shown || initial.pending || initial.queued) {
        fputs("pause and drain the UI before stopping it\n", stderr); return 2;
    }
    printf("layout read_mode=kmem_cached base=%08lx page_bytes=%u original=%u bss=%08x lld=%08x ufl=%08x\n",
        fix.smem_start, fix.line_length * original.yres, original.yoffset, bss, lld, ufl);
    v = original; atomic_store(&running, 1); pthread_t thread;
    if (pthread_create(&thread, NULL, observe, NULL)) die("observer");
    delay_us(100000); int failed = 0;
    for (unsigned i = 0; i < 128; ++i) {
        unsigned pause = i < 64 ? 0 : (i - 64) % 32 * 500;
        delay_us(pause); v.yoffset = 600 - v.yoffset;
        uint64_t start = now_ns(); int rc = ioctl(fd, FBIOPAN_DISPLAY, &v);
        uint64_t end = now_ns(); int qr = ioctl(fd, FBIOGET_VSCREENINFO, &actual);
        printf("flip index=%u pause_us=%u start=%" PRIu64 " end=%" PRIu64 " requested=%08lx rc=%d actual=%u\n",
            i, pause, start, end, fix.smem_start + v.yoffset * fix.line_length, rc, qr ? UINT32_MAX : actual.yoffset);
        if (rc || qr || actual.yoffset != v.yoffset) { failed = 1; break; }
    }
    int restore = ioctl(fd, FBIOPAN_DISPLAY, &original);
    int qr = ioctl(fd, FBIOGET_VSCREENINFO, &actual);
    int restored = !restore && !qr && actual.yoffset == original.yoffset;
    delay_us(100000); atomic_store(&running, 0); pthread_join(thread, NULL);
    for (unsigned i = 0; i < sample_count; ++i) {
        struct sample s = samples[i];
        printf("sample ns=%" PRIu64 " irq=%" PRIu64 " update=%08x address=%08x active=%08x pending=%08x queued=%u flags=%02x consistent=%u\n",
            s.ns,s.irq,s.update,s.address,s.active,s.pending,s.queued,s.flags,s.consistent);
    }
    printf("done failed=%d dropped=%u restored=%d samples=%u\n",failed,dropped,restored,sample_count);
    close(fd); munmap((void *)regs,4096);
    for(unsigned i=0;i<map_count;i++) munmap((void *)maps[i].words,4096);
    close(kmem); close(mem); return failed || dropped || !restored ? 1 : 0;
}
