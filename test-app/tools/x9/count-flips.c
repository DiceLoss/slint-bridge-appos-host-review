/* LD_PRELOAD observation only: retain exact matching DRM completion timestamps. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

#define CAPACITY 32768
struct record { uint32_t magic, pid, count, errors; uint64_t times[CAPACITY]; };
struct flip { uint32_t crtc, fb, flags, reserved; uint64_t cookie; };
static struct record *record;
static int pending_fd = -1;
static uint64_t pending_cookie;
#ifndef PROBE_TEST
static ssize_t (*next_read)(int, void *, size_t);
static int (*next_ioctl)(int, unsigned long, ...);
#endif

static void observe(int fd, const void *buffer, ssize_t length) {
    if (!record || fd != pending_fd || length < 0) return;
    uint32_t type=0, size=0; uint64_t cookie=0;
    if (length == 32) {
        memcpy(&type,buffer,4); memcpy(&size,(const char *)buffer+4,4);
        memcpy(&cookie,(const char *)buffer+8,8);
    }
    if (type!=2 || size!=32 || cookie!=pending_cookie) { record->errors|=1; return; }
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC,&t)) { record->errors|=8; return; }
    uint32_t n=record->count;
    if (n>=CAPACITY) { record->errors|=4; return; }
    record->times[n]=(uint64_t)t.tv_sec*1000000+t.tv_nsec/1000;
    __atomic_store_n(&record->count,n+1,__ATOMIC_RELEASE);
    pending_fd=-1;
}
#ifndef PROBE_TEST
__attribute__((constructor)) static void initialize(void) {
    next_read=dlsym(RTLD_NEXT,"read"); next_ioctl=dlsym(RTLD_NEXT,"ioctl");
    const char *path=getenv("SLINT_BRIDGE_X9_FPS_FILE");
    if (!path) return;
    int fd=open(path,O_RDWR|O_CREAT|O_EXCL|O_CLOEXEC,0644);
    if (!next_read || !next_ioctl || fd<0 || ftruncate(fd,sizeof(*record))) _exit(125);
    void *map=mmap(NULL,sizeof(*record),PROT_READ|PROT_WRITE,MAP_SHARED,fd,0);
    close(fd);
    if (map==MAP_FAILED) _exit(125);
    record=map; record->magic=0x58394650; record->pid=getpid();
}
ssize_t read(int fd, void *buffer, size_t length) {
    ssize_t result=next_read ? next_read(fd,buffer,length) : syscall(SYS_read,fd,buffer,length);
    int saved=errno;
    observe(fd,buffer,result);
    errno=saved; return result;
}
int ioctl(int fd, unsigned long request, ...) {
    va_list args; va_start(args,request);
    uintptr_t arg=request==0x641e ? 0 : request==0x4b3a
        ? (uintptr_t)va_arg(args,int) : (uintptr_t)va_arg(args,void *);
    va_end(args);
    int result=next_ioctl ? next_ioctl(fd,request,arg) : syscall(SYS_ioctl,fd,request,arg);
    int saved=errno;
    if (record && request==0xc01864b0) {
        if (result<0) record->errors|=16;
        else {
            struct flip value; memcpy(&value,(void *)arg,sizeof(value));
            if (pending_fd!=-1) record->errors|=2;
            pending_fd=fd; pending_cookie=value.cookie;
        }
    }
    errno=saved; return result;
}
#else
#include <assert.h>
int main(void) {
    static struct record data; record=&data;
    unsigned char event[32]={2,0,0,0,32,0,0,0};
    uint64_t cookie=0x583900000001ULL; memcpy(event+8,&cookie,8);
    pending_fd=7; pending_cookie=cookie;
    observe(8,event,32); assert(record->count==0 && record->errors==0);
    observe(7,event,-1); assert(record->count==0 && record->errors==0);
    observe(7,event,32); assert(record->count==1 && pending_fd==-1);
    observe(7,event,32); assert(record->count==1);
    pending_fd=7; pending_cookie=cookie+1;
    observe(7,event,32); assert(record->count==1 && record->errors==1);
    record->errors=0; pending_cookie=cookie; event[0]=1;
    observe(7,event,32); assert(record->count==1 && record->errors==1);
    record->errors=0; event[0]=2;
    observe(7,event,31); assert(record->count==1 && record->errors==1);
    record->errors=0; record->count=CAPACITY;
    observe(7,event,32); assert(record->count==CAPACITY && record->errors==4);
    puts("matching completion counter tests passed");
}
#endif
