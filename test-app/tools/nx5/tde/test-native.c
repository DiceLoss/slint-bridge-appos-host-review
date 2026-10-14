/* Fault injection at the SDK boundary; never talks to hardware. */
#include <assert.h>
#include <stdio.h>
#include "../../../../src/backends/nx5/tde/native.c"
static int operation;
static int fault, cancels, ends, frees, closes, flushes;
static int free_fault;
static void *unreleased;
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
static FY_S32 fill(TDE_HANDLE job, TDE2_SURFACE_S *d, TDE2_RECT_S *dr, FY_U32 color) {
    assert(color == 0x7f6b3917u);
    return copy(job, d, dr, d, dr);
}
static FY_S32 blend(TDE_HANDLE job, TDE2_SURFACE_S *bg, TDE2_RECT_S *br,
    TDE2_SURFACE_S *fg, TDE2_RECT_S *fr, TDE2_SURFACE_S *d, TDE2_RECT_S *dr, TDE2_OPT_S *opt) {
    (void)bg; (void)br;
    assert(opt->stBlendOpt.bPixelAlphaEnable && !opt->stBlendOpt.bGlobalAlphaEnable);
    assert(!opt->stBlendOpt.bSrc2AlphaPremulti);
    for (unsigned i = 0; i < 64; ++i) assert(((uint8_t *)current->source[0].ptr)[i] == 127);
    return copy(job, fg, fr, d, dr);
}
static FY_S32 flush(FY_U32 p,FY_VOID *v,FY_U32 n) {
    (void)p;(void)v;assert(n); ++flushes;
    return fault==5 && flushes==(operation == 2 ? 2 : 3) ? -105 : 0;
}
static FY_S32 free_mmz(FY_U32 p,FY_VOID *v) {
    (void)p; ++frees;
    if (free_fault && frees == 1) {
        if (free_fault == 1) unreleased = v; else free(v);
        return -106;
    }
    free(v); return 0;
}
static void close_tde(void) { ++closes; }
#include "test-odd-blend.c"
static void test_span_blending(void) {
    for (unsigned alpha = 0; alpha < 256; ++alpha) {
        uint8_t color[4] = {alpha, alpha / 2, 0, alpha};
        for (unsigned opaque = 0; opaque < 2; ++opaque) {
            uint8_t actual[256 * 4], expected[256 * 4];
            for (unsigned value = 0; value < 256; ++value) {
                uint8_t *p = actual + value * 4;
                p[0] = value; p[1] = value / 2; p[2] = value / 3;
                p[3] = opaque ? 255 : value;
                for (unsigned c = 0; c < 3; ++c)
                    expected[value * 4 + c] = p[c] * (255 - alpha) / 255 + color[c];
                expected[value * 4 + 3] = p[3] + alpha - p[3] * alpha / 255;
            }
            bridge_blend_rgba(actual, 256, color);
            assert(!memcmp(actual, expected, sizeof actual));
        }
    }
    for (unsigned count = 0; count <= 65; ++count) {
        for (unsigned shift = 0; shift < 4; ++shift) {
            uint8_t actual[65 * 4 + 8], expected[65 * 4 + 8];
            uint8_t color[4] = {67, 13, 111, 127};
            memset(actual, 0xa5, sizeof actual);
            memcpy(expected, actual, sizeof actual);
            for (unsigned i = 0; i < count; ++i) {
                uint8_t *p = expected + shift + i * 4;
                for (unsigned c = 0; c < 3; ++c)
                    p[c] = p[c] * (255 - color[3]) / 255 + color[c];
                p[3] = p[3] + color[3] - p[3] * color[3] / 255;
            }
            bridge_blend_rgba(actual + shift, count, color);
            assert(!memcmp(actual, expected, sizeof actual));
        }
    }
    puts("PASS span-blend exact Slint RGBA, all alpha pairs, lengths=0..65 alignments=0..3 and guards");
}
static void test_prepare_rgba(void) {
    for (unsigned premul=0;premul<2;++premul) {
        for (unsigned alpha=0;alpha<256;++alpha) {
            uint8_t source[256*4], actual[256*4], expected[256*4];
            for (unsigned a=0;a<256;++a) {
                for (unsigned c=0;c<3;++c) source[a*4+c]=(a*37+c*83)&255;
                source[a*4+3]=a;
                unsigned effective=a*alpha/255;
                for (unsigned c=0;c<3;++c)
                    expected[a*4+c]=source[a*4+c]*(premul ? alpha : effective)/255;
                expected[a*4+3]=effective;
            }
            bridge_prepare_rgba(actual,source,256,premul,alpha);
            assert(!memcmp(actual,expected,sizeof actual));
        }
    }
    for (unsigned n=0;n<=65;++n) {
        for (unsigned shift=0;shift<4;++shift) {
            uint8_t source[65*4+8], actual[65*4+8], expected[65*4+8];
            memset(source,173,sizeof source);memset(actual,0xa5,sizeof actual);
            memcpy(expected,actual,sizeof actual);
            for (unsigned i=0;i<n;++i) {
                unsigned a=173*127/255;
                for (unsigned c=0;c<3;++c) expected[shift+i*4+c]=173*a/255;
                expected[shift+i*4+3]=a;
            }
            bridge_prepare_rgba(actual+shift,source+shift,n,0,127);
            assert(!memcmp(actual,expected,sizeof actual));
        }
    }
    puts("PASS source preparation straight/premultiplied all alpha pairs, lengths=0..65, alignments=0..3 and guards");
}
static void test_sdk_loading(void) {
    int error;
    /* A visible SDK must not be loaded by TDE before the display owns it. */
    assert(!bridge_tde_create(4, 4, &error) && error == -1);
    void *owner = dlopen("libVideoDec.so", RTLD_NOW | RTLD_LOCAL);
    if (!owner) { fprintf(stderr, "%s\n", dlerror()); abort(); }
    int *constructors = dlsym(owner, "sdk_constructors");
    assert(constructors && *constructors == 1);
    /* The stub's Open result proves the production loader reused that SDK. */
    assert(!bridge_tde_create(4, 4, &error) && error == -177);
    assert(!bridge_tde_create(1280, 720, &error) && error == -177);
    assert(!bridge_tde_create(480, 800, &error) && error == -177);
    assert(!bridge_tde_create(UINT32_MAX - 3u, 2, &error) && error == -1);
    assert(!bridge_tde_create(800, 481, &error) && error == -1);
    assert(*constructors == 1);
    assert(!dlclose(owner));
    assert(!dlopen("libVideoDec.so", RTLD_NOW | RTLD_NOLOAD));
    puts("PASS SDK search path, unversioned alias, dynamic dimensions, NOLOAD and reference cleanup");
}
int main(int argc, char **argv) {
    if (argc == 2 && !strcmp(argv[1], "--sdk-loading")) {
        test_sdk_loading();
        return 0;
    }
    test_span_blending();
    test_odd_blend();
    test_prepare_rgba();
    uint32_t reciprocals[256] = {0};
    for (unsigned alpha = 1; alpha < 256; ++alpha)
        reciprocals[alpha] = ((255u << 16) + alpha - 1) / alpha;
    for (unsigned alpha_max = 128; alpha_max <= 255; alpha_max += 127) {
    for (unsigned mixed = 0; mixed < 2; ++mixed) {
    for (unsigned alpha = 0; alpha < 256; ++alpha) {
        uint8_t source[256*4], original[256*4], actual[256*4];
        for (unsigned channel = 0; channel < 256; ++channel) {
            memset(source+channel*4, channel, 3);
            if (mixed) {
                source[channel*4+1] = 255-channel;
                source[channel*4+2] = (channel*37)&255;
            }
            source[channel*4+3] = mixed ? (alpha+channel*73)&255 : alpha;
        }
        memcpy(original,source,sizeof source);
        bridge_rgba_to_bgra(actual,source,256,reciprocals,alpha_max);
        assert(!memcmp(source,original,sizeof source));
        for (unsigned channel = 0; channel < 256; ++channel) {
            unsigned a = source[channel*4+3];
            for (unsigned c=0;c<3;++c) {
                unsigned value = source[channel*4+2-c];
                unsigned expected = a ? (value*255u+a/2)/a : 0;
                if (expected > 255) expected = 255;
                assert(actual[channel*4+c]==expected);
            }
            assert(actual[channel*4+3]==(alpha_max == 128 ? (a+1)/2 : a));
        }
    }
    }
    for (unsigned count = 0; count <= 65; ++count) {
        for (unsigned shift = 0; shift < 4; ++shift) {
            for (unsigned pattern=0;pattern<5;++pattern) {
                uint8_t source[65*4+8], original[65*4+8], actual[65*4+8], expected[65*4+8];
                for (unsigned i=0;i<sizeof source;++i) source[i]=(i*37u+11u)&255u;
                for (unsigned i=0;i<count;++i) {
                    if (pattern==1) source[shift+i*4+3]=255;
                    if (pattern==2) source[shift+i*4+3]=0;
                    if (pattern==3) source[shift+i*4+3]=i<32?255:i<48?127:0;
                    if (pattern==4) source[shift+i*4+3]=i<16?127:i<48?255:0;
                }
                memcpy(original,source,sizeof source);
                memset(actual,0xa5,sizeof actual);memcpy(expected,actual,sizeof expected);
                for (unsigned i=0;i<count;++i) {
                    unsigned alpha=source[shift+i*4+3];
                    for (unsigned c=0;c<3;++c) {
                        unsigned value=source[shift+i*4+2-c];
                        unsigned straight=alpha ? (value*255u+alpha/2)/alpha : 0;
                        expected[shift+i*4+c]=straight > 255 ? 255 : straight;
                    }
                    expected[shift+i*4+3]=alpha_max == 128 ? (alpha+1)/2 : alpha;
                }
                bridge_rgba_to_bgra(actual+shift,source+shift,count,reciprocals,alpha_max);
                assert(!memcmp(actual,expected,sizeof actual));
                assert(!memcmp(source,original,sizeof source));
            }
        }
    }
    }
    puts("PASS pixel-conversion alpha_max=128/255 uniform/mixed-lane channel/alpha pairs, lengths=0..65 alignments=0..3 guards/source unchanged");
    for (operation=0;operation<3;++operation) {
    for(fault=0;fault<=5;++fault) {
        cancels=ends=frees=closes=flushes=0;
        Engine *e=calloc(1,sizeof(*e)); current=e;
        e->api.FY_TDE2_BeginJob=begin; e->api.FY_TDE2_CancelJob=cancel;
        e->api.FY_TDE2_EndJob=end_job; e->api.FY_TDE2_QuickCopy=copy;
        e->api.FY_TDE2_QuickFill=fill; e->api.FY_TDE2_Bitblit=blend;
        e->api.FY_MPI_SYS_MmzFlushCache=flush; e->api.FY_MPI_SYS_MmzFree=free_mmz;
        e->api.FY_TDE2_Close=close_tde; e->opened=1;
        e->width=4;e->height=4;e->bytes=64;
        e->target.ptr=calloc(1,64); e->source[0].ptr=calloc(1,64);
        uint8_t *pixels=bridge_tde_pixels(e), source[64];memset(pixels,0xa5,64);memset(source,255,64);
        if (operation == 1) for (unsigned i=3;i<64;i+=4) pixels[i]=255;
        uint8_t before[64];memcpy(before,pixels,64);
        BridgeRect rect={0,0,0,0,4,4}; int error;
        int result=operation == 2 ? bridge_tde_fill(e,0x7f6b3917u,&rect,1,&error) :
            bridge_tde_draw(e,source,4,4,16,0,operation == 1 ? 127 : 255,0,1,&rect,1,&error);
        if(!fault) { assert(result==(operation == 1));for(unsigned i=0;i<64;++i) assert(pixels[i]==(operation == 1 && i%4 == 3 ? 255 : 0x37)); }
        else {
            assert(result<0);
            if (fault == 1) assert(!memcmp(pixels,before,64));
            /* Every other fake job wrote part of the shared canvas. Recovery
             * must repaint the frame, not blend again over this partial result. */
            else for (unsigned i=0;i<64;++i) assert(pixels[i]==0x37);
        }
        assert(cancels==(fault==2 || fault==3));
        assert(ends==(fault==0 || fault==4 || fault==5));
        if(fault==3 || fault==4) {
            assert(result==-2 && e->quarantined);
            bridge_tde_destroy(e); assert(!frees && !closes);
            /* Test-only: the fake job has no DMA; release deliberately retained objects. */
            e->quarantined=0;
        } else assert(!e->quarantined);
        bridge_tde_destroy(e); assert(frees==2 && closes==1);
        printf("PASS operation=%d fault=%d result=%d sdk=%d\n",operation,fault,result,error);
    }
    }
    for (free_fault=1;free_fault<=2;++free_fault) {
        frees=closes=0; in_use=1;
        Engine *e=calloc(1,sizeof(*e));
        e->api.FY_MPI_SYS_MmzFree=free_mmz; e->api.FY_TDE2_Close=close_tde; e->opened=1;
        e->target.ptr=malloc(64); e->source[0].ptr=malloc(64); e->source[1].ptr=malloc(64);
        bridge_tde_destroy(e);
        assert(frees==3 && closes==1);
        for (unsigned retry=0;retry<3;++retry) {
            int error;
            assert(!bridge_tde_create(4,4,&error) && error==-2);
            assert(frees==3 && closes==1); /* No repeated release or new SDK use. */
        }
        printf("PASS MMZ release fault=%d blocks recreation without retrying addresses\n",free_fault);
        /* Test-only: no DMA/driver allocation; clean up the simulated leak. */
        free(unreleased); unreleased=NULL; in_use=0;
    }
    return 0;
}
