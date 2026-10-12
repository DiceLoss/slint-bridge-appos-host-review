/* Host loader regression only. No hardware or real SDK is accessed. */
#include <stdlib.h>
int sdk_constructors;
__attribute__((constructor)) static void initialize(void) { ++sdk_constructors; }
int FY_TDE2_Open(void) { return -177; }
/* Open deliberately fails; these symbols must resolve but never be called. */
void FY_TDE2_Close(void) { abort(); }
void FY_TDE2_BeginJob(void) { abort(); }
void FY_TDE2_EndJob(void) { abort(); }
void FY_TDE2_CancelJob(void) { abort(); }
void FY_TDE2_QuickCopy(void) { abort(); }
void FY_TDE2_Bitblit(void) { abort(); }
void FY_MPI_SYS_MmzAlloc_Cached(void) { abort(); }
void FY_MPI_SYS_MmzFree(void) { abort(); }
void FY_MPI_SYS_MmzFlushCache(void) { abort(); }
