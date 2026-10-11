/* Fault injection at the SDK boundary; never talks to hardware. */
#include <assert.h>
#include <stdio.h>
#include "../../../../src/backends/nx5/tde/native.c"
static int fault, cancels, ends, frees, closes, flushes;
static Engine *current;
static TDE_HANDLE begin(void) { return fault == 1 ? -101 : 7; }
static FY_S32 cancel(TDE_HANDLE job) { assert(job==7); ++cancels; return fault==3 ? -103 : 0; }
static FY_S32 end_job(TDE_HANDLE job,FY_BOOL sync,FY_BOOL block,FY_U32 timeout) {
    assert(job==7 && sync && block && timeout==100); ++ends;
    return fault==4 ? -104 : 0;
}
static FY_S32 copy(TDE_HANDLE job,TDE2_SURFACE_S *s,TDE2_RECT_S *sr,TDE2_SURFACE_S *d,TDE2_RECT_S *dr) {
    (void)s; (void)sr; (void)d; (void)dr; assert(job==7);
    memset(current->target.ptr, 0x37, current->bytes); /* Simulate a partial scratch write. */
    return fault==2 || fault==3 ? -102 : 0;
}
static FY_S32 flush(FY_U32 p,FY_VOID *v,FY_U32 n) {
    (void)p;(void)v;assert(n); ++flushes;
    return fault==5 && flushes==3 ? -105 : 0;
}
static FY_S32 free_mmz(FY_U32 p,FY_VOID *v) { (void)p; ++frees; free(v); return 0; }
static void close_tde(void) { ++closes; }
int main(void) {
    for (unsigned count = 0; count <= 65; ++count) {
        for (unsigned shift = 0; shift < 4; ++shift) {
            uint8_t source[65*4+8], actual[65*4+8], expected[65*4+8];
            for (unsigned i=0;i<sizeof source;++i) source[i]=(i*37u+11u)&255u;
            memset(actual,0xa5,sizeof actual);memcpy(expected,actual,sizeof expected);
            for (unsigned i=0;i<count;++i) {
                expected[shift+i*4]=source[shift+i*4+2];
                expected[shift+i*4+1]=source[shift+i*4+1];
                expected[shift+i*4+2]=source[shift+i*4];expected[shift+i*4+3]=255;
            }
            bridge_rgba_to_bgra(actual+shift,source+shift,count);
            assert(!memcmp(actual,expected,sizeof actual));
        }
    }
    puts("PASS pixel-conversion lengths=0..65 alignments=0..3 guards=unchanged");
    for(fault=0;fault<=5;++fault) {
        cancels=ends=frees=closes=flushes=0;
        Engine *e=calloc(1,sizeof(*e)); current=e;
        e->api.FY_TDE2_BeginJob=begin; e->api.FY_TDE2_CancelJob=cancel;
        e->api.FY_TDE2_EndJob=end_job; e->api.FY_TDE2_QuickCopy=copy;
        e->api.FY_MPI_SYS_MmzFlushCache=flush; e->api.FY_MPI_SYS_MmzFree=free_mmz;
        e->api.FY_TDE2_Close=close_tde; e->opened=1;
        e->width=4;e->height=4;e->bytes=64;
        e->target.ptr=calloc(1,64); e->source[0].ptr=calloc(1,64);
        uint8_t pixels[64], source[64];memset(pixels,0xa5,64);memset(source,255,64);
        BridgeRect rect={0,0,0,0,4,4}; int error;
        int result=bridge_tde_draw(e,pixels,source,4,4,16,0,0,1,&rect,1,&error);
        if(!fault) { assert(result==0);for(unsigned i=0;i<64;++i) assert(pixels[i]==0x37); }
        else { assert(result<0);for(unsigned i=0;i<64;++i) assert(pixels[i]==0xa5); }
        assert(cancels==(fault==2 || fault==3));
        assert(ends==(fault==0 || fault==4 || fault==5));
        if(fault==3 || fault==4) {
            assert(result==-2 && e->quarantined);
            bridge_tde_destroy(e); assert(!frees && !closes);
            /* Test-only: the fake job has no DMA; release deliberately retained objects. */
            e->quarantined=0;
        } else assert(!e->quarantined);
        bridge_tde_destroy(e); assert(frees==2 && closes==1);
        printf("PASS fault=%d result=%d sdk=%d\n",fault,result,error);
    }
    return 0;
}
