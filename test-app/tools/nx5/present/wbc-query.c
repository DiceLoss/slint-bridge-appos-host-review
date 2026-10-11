/* Read-only SDK queries: do not enable, bind, or reconfigure a WBC channel. */
#include <stdio.h>
#include "mpi_vo.h"

int main(void) {
    VO_WBC_SOURCE_S source = {0};
    VO_WBC_ATTR_S attr = {0};
    VO_WBC_MODE_E mode = VO_WBC_MODE_BUTT;
    FY_U32 depth = 0;
    int rc = FY_MPI_VO_GetWbcSource(0, &source);
    printf("source rc=%08x type=%u id=%u\n", rc, source.enSourceType, source.u32SourceId);
    rc = FY_MPI_VO_GetWbcAttr(0, &attr);
    printf("attr rc=%08x width=%u height=%u format=%u fps=%u\n", rc,
           attr.stTargetSize.u32Width, attr.stTargetSize.u32Height,
           attr.enPixelFormat, attr.u32FrameRate);
    rc = FY_MPI_VO_GetWbcMode(0, &mode);
    printf("mode rc=%08x value=%u\n", rc, mode);
    rc = FY_MPI_VO_GetWbcDepth(0, &depth);
    printf("depth rc=%08x value=%u\n", rc, depth);
    return 0;
}
