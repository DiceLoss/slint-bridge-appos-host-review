/* Pixel oracle at the production C/SDK boundary, without a device. */
static Engine *odd_engine;
static unsigned odd_jobs, odd_blits;
static int odd_wait_failure;
static FY_S32 odd_flush(FY_U32 p, FY_VOID *v, FY_U32 n) { (void)p; (void)v; assert(n); return 0; }
static TDE_HANDLE odd_begin(void) { ++odd_jobs; return 11; }
static FY_S32 odd_end(TDE_HANDLE j, FY_BOOL s, FY_BOOL b, FY_U32 t) {
    assert(j==11 && s && b && t==100); return odd_wait_failure ? -104 : 0;
}
static FY_S32 odd_blit(TDE_HANDLE j, TDE2_SURFACE_S *bg, TDE2_RECT_S *br,
    TDE2_SURFACE_S *fg, TDE2_RECT_S *fr, TDE2_SURFACE_S *dst, TDE2_RECT_S *dr, TDE2_OPT_S *opt) {
    (void)bg; (void)br; assert(j==11);
    assert(opt->enOutAlphaFrom == TDE2_OUTALPHA_FROM_NORM);
    assert(!((fr->s32Xpos | fr->s32Ypos | dr->s32Xpos | dr->s32Ypos | fr->u32Width | fr->u32Height) & 1));
    assert(fr->u32Width==dr->u32Width && fr->u32Height==dr->u32Height);
    ++odd_blits;
    for (unsigned y=0;y<dr->u32Height;++y) for(unsigned x=0;x<dr->u32Width;++x) {
        uint8_t *d=(uint8_t *)odd_engine->target.ptr+(dr->s32Ypos+y)*dst->u32Stride*4+(dr->s32Xpos+x)*4;
        unsigned slot = fg->u32PhyAddr == 2 ? 2 : 0;
        const uint8_t *s=(uint8_t *)odd_engine->source[slot].ptr+(fr->s32Ypos+y)*fg->u32Stride*4+(fr->s32Xpos+x)*4;
        for(unsigned c=0;c<3;++c) d[c]=s[c]+d[c]*(255-s[3])/255;
        /* The mock preserves the opaque background alpha, as requested. */
    }
    return 0;
}
static void test_odd_blend(void) {
    Engine e={0}; e.width=32;e.height=24;e.bytes=32*24*4;
    e.target.ptr=malloc(e.bytes);e.source[0].ptr=malloc(36*26*4);odd_engine=&e;
    e.api.FY_TDE2_BeginJob=odd_begin;e.api.FY_TDE2_EndJob=odd_end;
    e.api.FY_TDE2_Bitblit=odd_blit;e.api.FY_MPI_SYS_MmzFlushCache=odd_flush;
    uint8_t source[20*12*4], expected[32*24*4];unsigned cases=0;
    for(unsigned premul=0;premul<2;++premul) for(unsigned opacity=127;opacity<=255;opacity+=128)
    for(int x=2;x<=3;++x) for(int y=2;y<=3;++y)
    for(int sx=0;sx<=1;++sx) for(int sy=0;sy<=1;++sy)
    for(unsigned w=15;w<=16;++w) for(unsigned h=7;h<=8;++h) {
        for(unsigned i=0;i<20*12;++i) {
            unsigned a=(i*37+17)%256;source[i*4+3]=a;
            for(unsigned c=0;c<3;++c) source[i*4+c]=premul?(i*(c+3)%256)*a/255:i*(c+3)%256;
        }
        for(unsigned i=0;i<32*24;++i) {
            for(unsigned c=0;c<3;++c) expected[i*4+c]=(i*(c+1)+41)%256;
            expected[i*4+3]=255;
        }
        memcpy(e.target.ptr,expected,sizeof expected);
        BridgeRect r={sx,sy,x,y,w,h};
        for(unsigned py=0;py<h;++py) for(unsigned px=0;px<w;++px) {
            const uint8_t *s=source+((sy+py)*20+sx+px)*4;
            uint8_t *d=expected+((y+py)*32+x+px)*4;unsigned a=s[3]*opacity/255;
            for(unsigned c=0;c<3;++c) d[c]=s[c]*(premul?opacity:a)/255+d[c]*(255-a)/255;
        }
        odd_jobs=odd_blits=0;int error=-1;
        int result=bridge_tde_draw(&e,source,20,12,80,premul,opacity,0,1,&r,1,&error);
        if(result!=1) fprintf(stderr,"odd blend rejected x=%d y=%d sx=%d sy=%d w=%u h=%u result=%d\n",x,y,sx,sy,w,h,result);
        assert(result==1 && !error && odd_jobs==1 && odd_blits==1);
        if (memcmp(e.target.ptr,expected,sizeof expected)) {
            for(unsigned at=0;at<sizeof expected;++at) if(((uint8_t *)e.target.ptr)[at]!=expected[at]) {
                fprintf(stderr,"pixel mismatch case=%u premul=%u opacity=%u xy=%d,%d src=%d,%d wh=%u,%u at=%u actual=%u expected=%u\n",cases,premul,opacity,x,y,sx,sy,w,h,at,((uint8_t *)e.target.ptr)[at],expected[at]);break;
            }
            assert(0);
        }
        ++cases;
    }
    /* Disjoint clipping regions must retain their gap and outside guards. */
    const uint8_t base[4]={31,63,95,255}, mixed[4]={142,158,174,255};
    for(unsigned i=0;i<32*24;++i) memcpy(expected+i*4,base,4);
    memcpy(e.target.ptr,expected,sizeof expected);memset(source,127,sizeof source);
    BridgeRect regions[2]={{0,0,3,3,7,5},{10,4,13,7,9,7}};
    for(unsigned i=0;i<2;++i) for(unsigned y=0;y<regions[i].height;++y)
        for(unsigned x=0;x<regions[i].width;++x)
            memcpy(expected+((regions[i].dy+y)*32+regions[i].dx+x)*4,mixed,4);
    odd_jobs=odd_blits=0;int multi_error;
    assert(bridge_tde_draw(&e,source,20,12,80,1,255,0,1,regions,2,&multi_error)==1);
    assert(!multi_error && odd_jobs==1 && odd_blits==2);
    assert(!memcmp(e.target.ptr,expected,sizeof expected));
    /* The compact third slot admits padded small sources and rejects full-size uploads. */
    e.source[2].phy=2;e.source[2].ptr=malloc(source_capacity(&e,2));
    for(unsigned i=0;i<32*24;++i) memcpy(expected+i*4,base,4);
    memcpy(e.target.ptr,expected,sizeof expected);memset(source,127,sizeof source);
    BridgeRect compact={0,0,3,3,16,8};int compact_error;
    for(unsigned y=3;y<11;++y) for(unsigned x=3;x<19;++x) memcpy(expected+(y*32+x)*4,mixed,4);
    odd_jobs=odd_blits=0;
    assert(bridge_tde_draw(&e,source,20,12,80,1,255,2,1,&compact,1,&compact_error)==1);
    assert(odd_jobs==1 && odd_blits==1 && !memcmp(e.target.ptr,expected,sizeof expected));
    assert(bridge_tde_draw(&e,source,32,24,128,1,255,2,1,&compact,1,&compact_error)==2);
    assert(odd_jobs==1 && !memcmp(e.target.ptr,expected,sizeof expected));
    puts("PASS compact source slot: padded upload, exact pixels, oversized source rejected before access/job");
    /* An unknown completion must not touch the CPU fringes. */
    for(unsigned i=0;i<32*24;++i) memcpy((uint8_t *)e.target.ptr+i*4,base,4);
    memset(source,127,sizeof source);
    BridgeRect r={0,0,3,3,16,8};int error;
    odd_wait_failure=1;
    assert(bridge_tde_draw(&e,source,20,12,80,1,255,0,1,&r,1,&error)==-2);
    assert(e.quarantined && error==-104);
    for(unsigned y=0;y<24;++y) for(unsigned x=0;x<32;++x)
        if(x<4 || x>=18 || y<4 || y>=10)
            for(unsigned c=0;c<4;++c) assert(((uint8_t *)e.target.ptr)[(y*32+x)*4+c]==base[c]);
    odd_wait_failure=0;
    /* The mock has no DMA; real quarantined allocations are never freed. */
    free(e.target.ptr);free(e.source[0].ptr);free(e.source[2].ptr);
    printf("PASS odd blend: %u parity/clip/opacity cases, exact pixels and outside-clip guards\n",cases);
}
