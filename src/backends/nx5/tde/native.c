/* SDK ABI stays in C, compiled with the board's matching external headers.
 * DMA only touches private MMZ scratch. CPU pixels commit after successful wait
 * and cache synchronization. This module never initializes or shuts down VO/SYS.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <mpi_tde.h>
#include <mpi_sys.h>
#ifdef __ARM_NEON
#include <arm_neon.h>
#endif

/* CPU conversion only: independent of SDK state, including software fallback. */
int bridge_neon_available(void) {
#ifdef __ARM_NEON
    return 1;
#else
    return 0;
#endif
}

void bridge_rgba_to_bgra(uint8_t *destination, const uint8_t *source, size_t count) {
#ifdef __ARM_NEON
    while (count >= 16) {
        uint8x16x4_t rgba = vld4q_u8(source);
        uint8x16x4_t bgra;
        bgra.val[0] = rgba.val[2]; bgra.val[1] = rgba.val[1];
        bgra.val[2] = rgba.val[0]; bgra.val[3] = vdupq_n_u8(255);
        vst4q_u8(destination, bgra);
        source += 64; destination += 64; count -= 16;
    }
#endif
    for (size_t x = 0; x < count; ++x) {
        destination[x*4] = source[x*4+2];
        destination[x*4+1] = source[x*4+1];
        destination[x*4+2] = source[x*4];
        destination[x*4+3] = 255;
    }
}

#define API_FUNCTIONS(X) \
 X(FY_TDE2_Open) X(FY_TDE2_Close) X(FY_TDE2_BeginJob) X(FY_TDE2_EndJob) \
 X(FY_TDE2_CancelJob) X(FY_TDE2_QuickCopy) X(FY_TDE2_Bitblit) \
 X(FY_MPI_SYS_MmzAlloc_Cached) X(FY_MPI_SYS_MmzFree) X(FY_MPI_SYS_MmzFlushCache)
#define FIELD(name) __typeof__(&name) name;
typedef struct { API_FUNCTIONS(FIELD) } Api;
#undef FIELD

typedef struct { int32_t sx, sy, dx, dy; uint32_t width, height; } BridgeRect;
typedef struct { uint32_t phy; void *ptr; uint32_t width, height; int opaque; } Buffer;
typedef struct {
    Api api;
    void *library;
    Buffer target, source[2];
    uint32_t width, height, bytes;
    int opened, quarantined;
} Engine;
static int in_use;

void bridge_tde_destroy(Engine *e) {
    if (!e || e->quarantined) return; /* Retain MMZ, fd and dlopen ref on unknown DMA. */
    for (unsigned i = 0; i < 2; ++i)
        if (e->source[i].ptr) e->api.FY_MPI_SYS_MmzFree(e->source[i].phy, e->source[i].ptr);
    if (e->target.ptr) e->api.FY_MPI_SYS_MmzFree(e->target.phy, e->target.ptr);
    if (e->opened) e->api.FY_TDE2_Close();
    if (e->library) dlclose(e->library);
    free(e);
    __sync_lock_release(&in_use);
}

Engine *bridge_tde_create(uint32_t width, uint32_t height, int *error) {
    *error = -1;
    if (!width || !height || width > 1024 || height > 600 || (width & 3u) || (height & 1u)) return NULL;
    if (!__sync_bool_compare_and_swap(&in_use, 0, 1)) { *error = -2; return NULL; }
    Engine *e = calloc(1, sizeof(*e));
    if (!e) { __sync_lock_release(&in_use); return NULL; }
    /* NOLOAD is essential: this SDK has a SYS/VO-resetting constructor. */
    e->library = dlopen("libVideoDec.so", RTLD_NOW | RTLD_NOLOAD);
    if (!e->library) goto fail;
#define RESOLVE(name) do { *(void **)(&e->api.name) = dlsym(e->library, #name); if (!e->api.name) goto fail; } while (0);
    API_FUNCTIONS(RESOLVE)
#undef RESOLVE
    *error = e->api.FY_TDE2_Open();
    if (*error) goto fail;
    e->opened = 1; e->width = width; e->height = height; e->bytes = width * height * 4;
    *error = e->api.FY_MPI_SYS_MmzAlloc_Cached(&e->target.phy, &e->target.ptr, "bridge_tde_target", NULL, e->bytes);
    if (*error || !e->target.ptr) goto fail;
    for (unsigned i = 0; i < 2; ++i) {
        *error = e->api.FY_MPI_SYS_MmzAlloc_Cached(&e->source[i].phy, &e->source[i].ptr, "bridge_tde_source", NULL, e->bytes);
        if (*error || !e->source[i].ptr) goto fail;
    }
    *error = 0;
    return e;
fail:
    if (!*error) *error = -1;
    bridge_tde_destroy(e);
    return NULL;
}

static TDE2_SURFACE_S surface(uint32_t phy, uint32_t width, uint32_t height) {
    TDE2_SURFACE_S s = {0};
    s.u32PhyAddr = phy; s.u32Width = width; s.u32Height = height;
    s.u32Stride = width; /* This SDK uses pixels, not bytes. */
    /* Both operands use RGBA byte order. Copy and component-wise blending are
     * invariant under the matching R/B permutation of this ARGB descriptor. */
    s.enColorFmt = TDE2_COLOR_FMT_ARGB8888;
    s.bAlphaMax255 = FY_TRUE;
    return s;
}

/* 0 copied, 1 blended, 2 unsupported (untouched), -1 safe error, -2 unknown DMA.
 * Before any failure, output CPU pixels remain untouched. */
int bridge_tde_draw(Engine *e, uint8_t *pixels, const uint8_t *data,
    uint32_t width, uint32_t height, uint32_t stride, int premultiplied,
    unsigned slot, int upload, const BridgeRect *rects, unsigned count, int *error) {
    *error = 0;
    if (e->quarantined) return -2;
    if (!width || !height || width > e->width || height > e->height || (width & 3u) ||
        (height & 1u) || stride < width * 4 || slot > 1 || !count || count > 16) return 2;
    Buffer *src = &e->source[slot];
    for (unsigned i = 0; i < count; ++i) {
        const BridgeRect *r = &rects[i];
        if (r->sx < 0 || r->sy < 0 || r->dx < 0 || r->dy < 0 || !r->width || !r->height ||
            (uint64_t)r->sx + r->width > width || (uint64_t)r->sy + r->height > height ||
            (uint64_t)r->dx + r->width > e->width || (uint64_t)r->dy + r->height > e->height) return 2;
    }
    if (upload) {
        src->width = src->height = 0;
        src->opaque = 1;
        for (uint32_t y = 0; y < height; ++y) {
            const uint8_t *row = data + (size_t)y * stride;
            uint8_t *out = (uint8_t *)src->ptr + (size_t)y * width * 4;
            memcpy(out, row, width * 4);
            for (uint32_t x = 0; x < width && src->opaque; ++x)
                if (row[x * 4 + 3] != 255) src->opaque = 0;
            if (!premultiplied)
                for (uint32_t x = 0; x < width; ++x)
                    for (unsigned c = 0; c < 3; ++c)
                        out[x * 4 + c] = (unsigned)row[x * 4 + c] * row[x * 4 + 3] / 255;
        }
        *error = e->api.FY_MPI_SYS_MmzFlushCache(src->phy, src->ptr, width * height * 4);
        if (*error) return -1;
        src->width = width; src->height = height;
    } else if (src->width != width || src->height != height) return 2;
    if (!src->opaque) {
        /* VPPU blend rectangles require even coordinates and extents. Exact
         * odd clipping falls back; never enlarge a draw outside its clip. */
        for (unsigned i = 0; i < count; ++i) {
            const BridgeRect *r = &rects[i];
            if ((r->sx | r->sy | r->dx | r->dy | r->width | r->height) & 1u) return 2;
        }
    }
    uint32_t top = e->height, bottom = 0;
    for (unsigned i = 0; i < count; ++i) {
        const BridgeRect *r = &rects[i];
        if ((uint32_t)r->dy < top) top = r->dy;
        if ((uint32_t)r->dy + r->height > bottom) bottom = r->dy + r->height;
        if (!src->opaque)
            for (uint32_t y = 0; y < r->height; ++y) {
                size_t at = ((size_t)(r->dy + y) * e->width + r->dx) * 4;
                /* This profile blends only onto opaque UI pixels. Transparent
                 * destinations use Slint's exact alpha compositing in software. */
                for (uint32_t x = 0; x < r->width; ++x)
                    if (pixels[at + x * 4 + 3] != 255) return 2;
                memcpy((uint8_t *)e->target.ptr + at, pixels + at, r->width * 4);
            }
    }
    uint32_t first = (top * e->width * 4) & ~4095u;
    uint32_t last = (bottom * e->width * 4 + 4095u) & ~4095u;
    if (last > e->bytes) last = e->bytes;
    *error = e->api.FY_MPI_SYS_MmzFlushCache(e->target.phy + first, (uint8_t *)e->target.ptr + first, last - first);
    if (*error) return -1;
    TDE_HANDLE job = e->api.FY_TDE2_BeginJob();
    if (job < 0) { *error = job; return -1; }
    TDE2_SURFACE_S s = surface(src->phy, width, height), d = surface(e->target.phy, e->width, e->height);
    for (unsigned i = 0; i < count; ++i) {
        const BridgeRect *r = &rects[i];
        TDE2_RECT_S sr = {r->sx, r->sy, r->width, r->height}, dr = {r->dx, r->dy, r->width, r->height};
        if (src->opaque) *error = e->api.FY_TDE2_QuickCopy(job, &s, &sr, &d, &dr);
        else {
            TDE2_OPT_S opt = {0};
            opt.stBlendOpt.bPixelAlphaEnable = FY_TRUE;
            /* Upload already uses Slint's truncating /255 premultiplication. */
            opt.stBlendOpt.bSrc2AlphaPremulti = FY_FALSE;
            opt.bClutReload = FY_TRUE;
            opt.enOutAlphaFrom = TDE2_OUTALPHA_FROM_NORM;
            *error = e->api.FY_TDE2_Bitblit(job, &d, &dr, &s, &sr, &d, &dr, &opt);
        }
        if (*error) {
            if (e->api.FY_TDE2_CancelJob(job)) { e->quarantined = 1; return -2; }
            return -1;
        }
    }
    *error = e->api.FY_TDE2_EndJob(job, FY_TRUE, FY_TRUE, 100); /* 1 second */
    if (*error) { e->quarantined = 1; return -2; } /* Cancel is invalid after EndJob. */
    *error = e->api.FY_MPI_SYS_MmzFlushCache(e->target.phy + first, (uint8_t *)e->target.ptr + first, last - first);
    if (*error) return -1; /* Job is retired; CPU output is still unchanged. */
    for (unsigned i = 0; i < count; ++i) {
        const BridgeRect *r = &rects[i];
        for (uint32_t y = 0; y < r->height; ++y) {
            size_t at = ((size_t)(r->dy + y) * e->width + r->dx) * 4;
            memcpy(pixels + at, (uint8_t *)e->target.ptr + at, r->width * 4);
            if (!src->opaque)
                for (uint32_t x = 0; x < r->width; ++x) pixels[at + x * 4 + 3] = 255;
        }
    }
    return src->opaque ? 0 : 1;
}
