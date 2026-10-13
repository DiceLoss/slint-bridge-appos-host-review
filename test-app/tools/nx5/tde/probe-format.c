/* Exclusive NX5 MMZ-only conversion probe; never writes framebuffer pages.
 * Source bytes are Slint premultiplied RGBA. Expected output is straight BGRA
 * with the requested alpha range. Rejected and mismatching paths are findings,
 * not successful conversions; stdout retains their SDK codes and pixel errors.
 * Modes: 0 same-format QuickCopy control; 1 source-format QuickCopy;
 * 2 foreground-only Bitblit; 3 background-only Bitblit;
 * 4/5 CONFIG copy factors, foreground alpha, premultiply off/on;
 * 6/7 same with normal output alpha; 8 pixel alpha disabled;
 * 9 CONFIG source-over factors. Tested flags are reported, not presumed honored.
 */
#include "../../../../src/backends/nx5/tde/native.c"
#include <fcntl.h>
#include <sys/file.h>
#include <unistd.h>
#include <time.h>
#include <assert.h>

static void ownership(void) {
    int fd = open("/run/slint-bridge-nx5.lock", O_CREAT|O_RDWR, 0600);
    assert(fd >= 0 && flock(fd, LOCK_EX|LOCK_NB) == 0);
    FILE *f = fopen("/proc/umap/vo", "r"); assert(f);
    char line[512]; int table=0, found=0;
    while (fgets(line,sizeof(line),f)) {
        if (strstr(line,"-----DEV CONFIG")) {table=1;found=1;continue;}
        if (table && strstr(line,"-----VIDEO LAYER STATUS")) break;
        unsigned id; char enabled;
        assert(!table || sscanf(line," %u %c",&id,&enabled)!=2 || enabled!='Y');
    }
    fclose(f); assert(found);
}
static void check_error(int code, const char *where) {
    if (code) { fprintf(stderr,"fatal %s code=%d; no further SDK cleanup\n",where,code); _exit(2); }
}
static unsigned straight(unsigned c,unsigned a) { return a ? (c*255+a/2)/a : 0; }
int main(void) {
    setbuf(stdout,NULL); ownership();
    void *lib=dlopen("libVideoDec.so",RTLD_NOW|RTLD_NODELETE); assert(lib);
    int (*disable)(void)=dlsym(lib,"VO_Disable"); assert(disable);
    int error; Engine *e=bridge_tde_create(256,256,&error); assert(e);
    uint8_t *src=e->target.ptr,*dst=e->source[0].ptr;
    /* Each alpha row contains all input intensity values. Premultiplied,
     * distinct R/G/B channels detect both permutation and missing division. */
    for (unsigned a=0;a<256;++a) for(unsigned v=0;v<256;++v) {
        unsigned at=(a*256+v)*4;
        src[at]=v*a/255;src[at+1]=(255-v)*a/255;
        src[at+2]=((v*37)&255)*a/255;src[at+3]=a;
    }
    TDE2_RECT_S r={0,0,256,256};
    for (unsigned fmt=TDE2_COLOR_FMT_ARGB8888;fmt<=TDE2_COLOR_FMT_RABG8888;++fmt)
    for (unsigned mode=0;mode<10;++mode) for(unsigned maximum=0;maximum<2;++maximum) {
        memset(dst,0xa5,e->bytes); memset(e->source[1].ptr,0,e->bytes);
        check_error(e->api.FY_MPI_SYS_MmzFlushCache(e->target.phy,src,e->bytes),"flush source");
        check_error(e->api.FY_MPI_SYS_MmzFlushCache(e->source[0].phy,dst,e->bytes),"flush destination");
        TDE2_SURFACE_S s=surface(e->target.phy,256,256),d=surface(e->source[0].phy,256,256);
        s.enColorFmt=mode==0?TDE2_COLOR_FMT_ARGB8888:fmt;
        d.bAlphaMax255=maximum;
        TDE2_OPT_S opt={0}; opt.enOutAlphaFrom=TDE2_OUTALPHA_FROM_FOREGROUND;
        opt.u8GlobalAlpha=255;
        if (mode>=4) { opt.enAluCmd=TDE2_ALUCMD_BLEND; opt.stBlendOpt.eBlendCmd=TDE2_BLENDCMD_CONFIG;
            opt.stBlendOpt.eSrc1BlendMode=mode==9?TDE2_BLEND_INVSRC2ALPHA:TDE2_BLEND_ZERO;
            opt.stBlendOpt.eSrc2BlendMode=TDE2_BLEND_ONE;
            opt.stBlendOpt.bPixelAlphaEnable=mode!=8;
            opt.stBlendOpt.bSrc2AlphaPremulti=(mode==5||mode==7);
            if(mode>=6)opt.enOutAlphaFrom=TDE2_OUTALPHA_FROM_NORM;
        }
        TDE_HANDLE job=e->api.FY_TDE2_BeginJob();assert(job>=0);
        int result;
        if (mode<=1) result=e->api.FY_TDE2_QuickCopy(job,&s,&r,&d,&r);
        else if(mode==3) {opt.enOutAlphaFrom=TDE2_OUTALPHA_FROM_BACKGROUND;result=e->api.FY_TDE2_Bitblit(job,&s,&r,NULL,NULL,&d,&r,&opt);}
        else result=e->api.FY_TDE2_Bitblit(job,mode>=4?&d:NULL,mode>=4?&r:NULL,&s,&r,&d,&r,&opt);
        if(result) {check_error(e->api.FY_TDE2_CancelJob(job),"cancel");printf("format=%u mode=%u dst_max=%u rejected=%d\n",fmt,mode,maximum?255:128,result);continue;}
        check_error(e->api.FY_TDE2_EndJob(job,1,1,100),"end");
        check_error(e->api.FY_MPI_SYS_MmzFlushCache(e->source[0].phy,dst,e->bytes),"read destination");
        unsigned bad_rgb=0,bad_alpha=0,max_rgb=0,max_alpha=0,straight_rgb=0,swapped_rgb=0;
        for(unsigned i=0;i<65536;++i) {
            unsigned a=src[i*4+3];
            for(unsigned c=0;c<3;++c) {
                unsigned want=straight(src[i*4+2-c],a),got=dst[i*4+c];
                unsigned diff=got>want?got-want:want-got;bad_rgb+=diff!=0;if(diff>max_rgb)max_rgb=diff;
                swapped_rgb+=got!=src[i*4+2-c];straight_rgb+=got!=src[i*4+c];
            }
            unsigned want=(a*(maximum?255:128)+127)/255,got=dst[i*4+3];
            unsigned diff=got>want?got-want:want-got;bad_alpha+=diff!=0;if(diff>max_alpha)max_alpha=diff;
        }
        unsigned at=(127*256+217)*4;
        printf("format=%u mode=%u dst_max=%u bad_rgb=%u max_rgb=%u bad_alpha=%u max_alpha=%u raw_same_bad=%u raw_swapped_bad=%u sample_src=%u,%u,%u,%u sample_dst=%u,%u,%u,%u\n",fmt,mode,maximum?255:128,bad_rgb,max_rgb,bad_alpha,max_alpha,straight_rgb,swapped_rgb,src[at],src[at+1],src[at+2],src[at+3],dst[at],dst[at+1],dst[at+2],dst[at+3]);
    }
    bridge_tde_destroy(e); assert(disable()==1); dlclose(lib);return 0;
}
