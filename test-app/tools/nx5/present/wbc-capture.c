/* Capture the composed device output with the SDK's previously unused WBC.
 * This does not touch the display pages, graphic layer, or MMIO registers.
 */
#define _POSIX_C_SOURCE 200809L
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
#include "mpi_vo.h"

static int check(const char *operation, int rc) {
    printf("%s rc=%08x\n", operation, rc); fflush(stdout); return rc;
}
int main(int argc, char **argv) {
    if (argc != 2) { fprintf(stderr, "usage: %s output.pgm\n", argv[0]); return 2; }
    VO_WBC_ATTR_S previous = {0};
    if (check("get_attr", FY_MPI_VO_GetWbcAttr(0, &previous))) return 3;
    if ((previous.stTargetSize.u32Width || previous.stTargetSize.u32Height) &&
        !(previous.stTargetSize.u32Width == 1024 && previous.stTargetSize.u32Height == 600 &&
          previous.enPixelFormat == PIXEL_FORMAT_YUV_SEMIPLANAR_420 && previous.u32FrameRate == 60)) {
        fprintf(stderr, "WBC already configured; refusing to take ownership\n"); return 3;
    }
    int registers_fd = open("/dev/mem", O_RDONLY | O_SYNC);
    if (registers_fd < 0) return 3;
    volatile const unsigned *registers = mmap(NULL, 4096, PROT_READ, MAP_SHARED,
                                             registers_fd, 0x34101000u);
    if (registers == MAP_FAILED) return 3;
    if (registers[0x90 / 4] & 1) {
        fprintf(stderr, "WBC is enabled; refusing to take ownership\n"); return 3;
    }
    VO_WBC_SOURCE_S source = {VO_WBC_SOURCE_DEV, 0};
    VO_WBC_ATTR_S attr = {{1024, 600}, PIXEL_FORMAT_YUV_SEMIPLANAR_420, 60};
    if (check("set_source", FY_MPI_VO_SetWbcSource(0, &source))) return 3;
    if (check("set_attr", FY_MPI_VO_SetWbcAttr(0, &attr))) return 3;
    if (check("set_depth", FY_MPI_VO_SetWbcDepth(0, 2))) return 3;
    if (check("enable", FY_MPI_VO_EnableWbc(0))) return 3;
    int result = 1;
    VIDEO_FRAME_INFO_S frame = {0};
    for (unsigned i = 0; i < 3; ++i) {
        if (check("warmup_get", FY_MPI_VO_GetWbcFrame(0, &frame, 500))) goto cleanup;
        if (check("warmup_release", FY_MPI_VO_ReleaseWbcFrame(0, &frame))) goto cleanup;
    }
    if (!check("get_frame", FY_MPI_VO_GetWbcFrame(0, &frame, 500))) {
        printf("registers cfg=%08x wbc=%08x work=%08x addr=%08x\n",
               registers[0], registers[0x90 / 4], registers[0xc / 4], registers[0x134 / 4]);
        printf("frame width=%u height=%u stride=%u format=%u phys=%08x pts=%llu\n",
               frame.stVFrame.u32Width, frame.stVFrame.u32Height,
               frame.stVFrame.u32Stride[0], frame.stVFrame.enPixelFormat,
               frame.stVFrame.u32PhyAddr[0], (unsigned long long)frame.stVFrame.u64pts);
        unsigned width = frame.stVFrame.u32Width, height = frame.stVFrame.u32Height;
        unsigned stride = frame.stVFrame.u32Stride[0], address = frame.stVFrame.u32PhyAddr[0];
        if (width == 1024 && height == 600 && stride >= width && stride <= 8192 &&
            frame.stVFrame.enPixelFormat == PIXEL_FORMAT_YUV_SEMIPLANAR_420 && address) {
            int memfd = open("/dev/mem", O_RDONLY | O_SYNC);
            if (memfd >= 0) {
                unsigned offset = address & 4095u;
                size_t length = (size_t)stride * height + offset;
                const unsigned char *map = mmap(NULL, length, PROT_READ, MAP_SHARED,
                                               memfd, address & ~4095u);
                if (map != MAP_FAILED) {
                    FILE *f = fopen(argv[1], "wb");
                    if (f) {
                        fprintf(f, "P5\n%u %u\n255\n", width, height);
                        result = 0;
                        for (unsigned y = 0; y < height; ++y)
                            if (fwrite(map + offset + (size_t)y * stride, 1, width, f) != width) result = 1;
                        if (fclose(f)) result = 1;
                    }
                    munmap((void *)map, length);
                } else perror("mmap capture");
                close(memfd);
            } else perror("open /dev/mem");
        }
        if (check("release_frame", FY_MPI_VO_ReleaseWbcFrame(0, &frame))) result = 1;
    }
cleanup:
    if (check("disable", FY_MPI_VO_DisableWbc(0))) result = 1;
#ifndef BRIDGE_IN_PROCESS
    if (check("close", FY_MPI_VO_CloseFd())) result = 1;
#endif
    munmap((void *)registers, 4096); close(registers_fd);
    return result;
}
