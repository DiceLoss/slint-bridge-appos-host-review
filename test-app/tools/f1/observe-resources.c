/* F1 /proc sampler: process identity, memory and shared display/GE IRQ counts.
 * No framebuffer, evdev or SDK access. */
#define _POSIX_C_SOURCE 200809L
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

static int64_t mono_us(void) {
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC, &t)) { perror("clock"); exit(2); }
    return (int64_t)t.tv_sec * 1000000 + t.tv_nsec / 1000;
}
static long field(const char *path, const char *key) {
    FILE *f = fopen(path, "r");
    if (!f) return -1;
    char line[512]; long value = -1;
    while (fgets(line, sizeof(line), f)) {
        if (!strncmp(line, key, strlen(key))) {
            if (sscanf(line + strlen(key), "%ld", &value) != 1) value = -1;
            break;
        }
    }
    fclose(f); return value;
}
static int stat_fields(int pid, uint64_t *ticks, uint64_t *start, uint64_t *major) {
    char path[64], line[4096];
    snprintf(path, sizeof(path), "/proc/%d/stat", pid);
    FILE *f = fopen(path, "r");
    if (!f) return -1;
    char *read = fgets(line, sizeof(line), f);
    fclose(f);
    char *end = read ? strrchr(line, ')') : NULL, *state = NULL;
    if (!end) return -1;
    char *token = strtok_r(end + 1, " ", &state);
    *ticks = 0; *start = 0; *major = 0;
    for (int column = 3; column <= 22; column++) {
        if (!token) return -1;
        if (column == 14 || column == 15) *ticks += strtoull(token, NULL, 10);
        if (column == 22) *start = strtoull(token, NULL, 10);
        if (column == 12) *major = strtoull(token, NULL, 10);
        token = strtok_r(NULL, " ", &state);
    }
    return 0;
}
static long irq_count(const char *name) {
    FILE *f = fopen("/proc/interrupts", "r");
    if (!f) return -1;
    char line[256]; long count = -1;
    while (fgets(line, sizeof(line), f)) {
        if (strstr(line, name)) {
            char *colon = strchr(line, ':');
            if (colon) sscanf(colon + 1, "%ld", &count);
            break;
        }
    }
    fclose(f); return count;
}
int main(int argc, char **argv) {
    if (argc != 4) { fprintf(stderr,"usage: %s PID MAX_SECONDS PHASE_FILE\n",argv[0]); return 2; }
    int pid = atoi(argv[1]), seconds = atoi(argv[2]);
    if (pid <= 0 || seconds <= 0 || seconds > 1800) return 2;
    uint64_t ticks, start, major, identity, own_ticks, own_start, own_major;
    if (stat_fields(pid, &ticks, &identity, &major)) return 3;
    setvbuf(stdout, NULL, _IOLBF, 0);
    printf("meta pid=%d hz=%ld start_ticks=%" PRIu64 " observer_pid=%d\n",pid,sysconf(_SC_CLK_TCK),identity,getpid());
    puts("mono_us,phase,cpu_ticks,rss_kib,hwm_kib,threads,major_faults,free_kib,shmem_kib,cpu_khz,observer_ticks,available_kib,ge_irqs,de_irqs");
    int64_t deadline = mono_us() + (int64_t)seconds * 1000000;
    while (mono_us() < deadline) {
        char phase[64] = "unknown", status[64];
        FILE *f = fopen(argv[3], "r");
        if (f) { if (fscanf(f,"%63s",phase)!=1) strcpy(phase,"unknown"); fclose(f); }
        if (!strcmp(phase,"done")) return 0;
        if (stat_fields(pid,&ticks,&start,&major) || start != identity ||
            stat_fields(getpid(),&own_ticks,&own_start,&own_major)) return 3;
        snprintf(status,sizeof(status),"/proc/%d/status",pid);
        long rss=field(status,"VmRSS:"), hwm=field(status,"VmHWM:"), threads=field(status,"Threads:");
        long free_kib=field("/proc/meminfo","MemFree:"), shmem=field("/proc/meminfo","Shmem:");
        long available=field("/proc/meminfo","MemAvailable:");
        long ge=irq_count("18b00000.ge"), de=irq_count("18a00000.de");
        long freq=field("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq","");
        if (rss<0 || hwm<0 || threads<0 || free_kib<0 || shmem<0 || available<0 || ge<0 || de<0) return 3;
        printf("%" PRId64 ",%s,%" PRIu64 ",%ld,%ld,%ld,%" PRIu64 ",%ld,%ld,%ld,%" PRIu64 ",%ld,%ld,%ld\n",
            mono_us(),phase,ticks,rss,hwm,threads,major,free_kib,shmem,freq,own_ticks,available,ge,de);
        struct timespec pause={1,0};
        while (nanosleep(&pause,&pause)) {}
    }
    return 0;
}
