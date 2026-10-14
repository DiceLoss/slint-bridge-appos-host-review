/* SDK ABI stays in C, compiled with the board's matching external headers.
 * The cached MMZ canvas is shared by CPU and TDE. Jobs are synchronous and
 * transfer cache ownership before/after DMA. This module never owns VO/SYS.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
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

#ifdef __ARM_NEON
/* Exact floor(x/255) for 0 <= x <= 255*255. */
static uint8x8_t divide_255(uint16x8_t value) {
    value = vaddq_u16(value, vaddq_u16(vdupq_n_u16(1), vshrq_n_u16(value, 8)));
    return vshrn_n_u16(value, 8);
}
#endif

/* Premultiplied source-over, matching Slint's RGB floor and alpha rounding.
 * CPU-only: independent of TDE, MMZ, and DMA completion. Unaligned is allowed. */
void bridge_blend_rgba(uint8_t *pixels, size_t count, const uint8_t *color) {
    unsigned alpha = color[3], inverse = 255 - alpha;
#ifdef __ARM_NEON
    uint8x8_t a = vdup_n_u8(alpha), inv = vdup_n_u8(inverse);
    uint8x8_t r = vdup_n_u8(color[0]), g = vdup_n_u8(color[1]), b = vdup_n_u8(color[2]);
    while (count >= 8) {
        uint8x8x4_t p = vld4_u8(pixels);
        p.val[0] = vadd_u8(divide_255(vmull_u8(p.val[0], inv)), r);
        p.val[1] = vadd_u8(divide_255(vmull_u8(p.val[1], inv)), g);
        p.val[2] = vadd_u8(divide_255(vmull_u8(p.val[2], inv)), b);
        p.val[3] = vsub_u8(vadd_u8(p.val[3], a), divide_255(vmull_u8(p.val[3], a)));
        vst4_u8(pixels, p);
        pixels += 32; count -= 8;
    }
#endif
    for (size_t i = 0; i < count; ++i, pixels += 4) {
        for (unsigned c = 0; c < 3; ++c)
            pixels[c] = pixels[c] * inverse / 255 + color[c];
        pixels[3] = pixels[3] + alpha - pixels[3] * alpha / 255;
    }
}

/* Prepare exactly the pixel Slint would blend. Straight RGBA combines alpha
 * before premultiplication; premultiplied RGBA scales every channel directly. */
void bridge_prepare_rgba(uint8_t *out, const uint8_t *source, size_t count,
    int premultiplied, uint8_t alpha) {
#ifdef __ARM_NEON
    while (count >= 8) {
        uint8x8x4_t p = vld4_u8(source);
        uint8x8_t a = vdup_n_u8(alpha);
        p.val[3] = divide_255(vmull_u8(p.val[3], a));
        uint8x8_t factor = premultiplied ? a : p.val[3];
        for (unsigned c = 0; c < 3; ++c) p.val[c] = divide_255(vmull_u8(p.val[c], factor));
        vst4_u8(out, p);
        source += 32; out += 32; count -= 8;
    }
#endif
    for (size_t i = 0; i < count; ++i, source += 4, out += 4) {
        unsigned a = source[3] * alpha / 255;
        for (unsigned c = 0; c < 3; ++c)
            out[c] = source[c] * (premultiplied ? alpha : a) / 255;
        out[3] = a;
    }
}

static uint8_t straight_channel(uint8_t value, uint32_t factor) {
    uint32_t scaled = ((uint32_t)value * factor + 32768) >> 16;
    return scaled > 255 ? 255 : (uint8_t)scaled;
}

#ifdef __ARM_NEON
/* Exact Q16 scaling across eight pixels; final narrowing clamps to u8.
 * Products plus rounding fit u32 even for noncanonical RGB > alpha. */
static uint8x8_t straight_eight(uint8x8_t values, uint32x4_t low, uint32x4_t high) {
    uint16x8_t wide = vmovl_u8(values);
    uint16x4_t lo = vrshrn_n_u32(vmulq_u32(vmovl_u16(vget_low_u16(wide)), low), 16);
    uint16x4_t hi = vrshrn_n_u32(vmulq_u32(vmovl_u16(vget_high_u16(wide)), high), 16);
    return vqmovn_u16(vcombine_u16(lo, hi));
}

/* Keep the opaque loop separate: GCC 5 otherwise spills its constant alpha
 * vector in every block because of the mixed-alpha path's register pressure. */
static __attribute__((noinline)) size_t copy_opaque_run(uint8_t *destination,
    const uint8_t *source, size_t count, uint8_t alpha_max) {
    size_t copied = 0;
    uint8x16x4_t bgra;
    bgra.val[3] = vdupq_n_u8(alpha_max);
    while (count - copied >= 16) {
        uint8x16x4_t rgba = vld4q_u8(source + copied * 4);
        uint8x8_t alpha = vand_u8(vget_low_u8(rgba.val[3]), vget_high_u8(rgba.val[3]));
        if (vget_lane_u64(vreinterpret_u64_u8(alpha), 0) != UINT64_MAX) break;
        bgra.val[0] = rgba.val[2]; bgra.val[1] = rgba.val[1]; bgra.val[2] = rgba.val[0];
        vst4q_u8(destination + copied * 4, bgra);
        copied += 16;
    }
    return copied;
}

static __attribute__((noinline)) size_t clear_transparent_run(uint8_t *destination,
    const uint8_t *source, size_t count) {
    size_t copied = 0;
    uint8x16x4_t zero;
    for (unsigned c = 0; c < 4; ++c) zero.val[c] = vdupq_n_u8(0);
    while (count - copied >= 16) {
        uint8x16x4_t rgba = vld4q_u8(source + copied * 4);
        uint8x8_t alpha = vorr_u8(vget_low_u8(rgba.val[3]), vget_high_u8(rgba.val[3]));
        if (vget_lane_u64(vreinterpret_u64_u8(alpha), 0) != 0) break;
        vst4q_u8(destination + copied * 4, zero);
        copied += 16;
    }
    return copied;
}
#endif

void bridge_rgba_to_bgra(uint8_t *destination, const uint8_t *source, size_t count,
                         const uint32_t *reciprocals, uint8_t alpha_max) {
    while (count) {
        size_t block = count < 16 ? count : 16;
#ifdef __ARM_NEON
        if (count >= 16) {
            uint8x16x4_t rgba = vld4q_u8(source);
            uint8x8_t alpha = vand_u8(vget_low_u8(rgba.val[3]), vget_high_u8(rgba.val[3]));
            if (vget_lane_u64(vreinterpret_u64_u8(alpha), 0) == UINT64_MAX) {
                size_t copied = copy_opaque_run(destination, source, count, alpha_max);
                source += copied * 4; destination += copied * 4; count -= copied;
                continue;
            }
            /* Alpha zero always exports zero RGB, even for noncanonical input. */
            alpha = vorr_u8(vget_low_u8(rgba.val[3]), vget_high_u8(rgba.val[3]));
            if (vget_lane_u64(vreinterpret_u64_u8(alpha), 0) == 0) {
                size_t copied = clear_transparent_run(destination, source, count);
                source += copied * 4; destination += copied * 4; count -= copied;
                continue;
            }
        }
        if (block >= 8 && (alpha_max == 128 || alpha_max == 255)) {
            size_t groups = block / 8;
            for (size_t i = 0; i < groups; ++i) {
                uint8x8x4_t rgba = vld4_u8(source), bgra;
                uint32x4_t low, high;
                uint8x8_t same_alpha = vceq_u8(rgba.val[3], vdup_n_u8(source[3]));
                if (vget_lane_u64(vreinterpret_u64_u8(same_alpha), 0) == UINT64_MAX) {
                    // Flat translucent spans need only one lookup for eight pixels.
                    low = high = vdupq_n_u32(reciprocals[source[3]]);
                } else {
                    low = (uint32x4_t){reciprocals[source[3]], reciprocals[source[7]],
                        reciprocals[source[11]], reciprocals[source[15]]};
                    high = (uint32x4_t){reciprocals[source[19]], reciprocals[source[23]],
                        reciprocals[source[27]], reciprocals[source[31]]};
                }
                bgra.val[0] = straight_eight(rgba.val[2], low, high);
                bgra.val[1] = straight_eight(rgba.val[1], low, high);
                bgra.val[2] = straight_eight(rgba.val[0], low, high);
                bgra.val[3] = alpha_max == 128 ? vrshr_n_u8(rgba.val[3], 1) : rgba.val[3];
                vst4_u8(destination, bgra);
                source += 32; destination += 32;
            }
            count -= groups * 8;
            continue;
        }
#endif
        for (size_t x = 0; x < block; ++x) {
            uint8_t alpha = source[3];
            if (alpha == 255) {
                destination[0] = source[2]; destination[1] = source[1]; destination[2] = source[0];
            } else {
                uint32_t factor = reciprocals[alpha]; /* alpha=0 produces zero RGB. */
                destination[0] = straight_channel(source[2], factor);
                destination[1] = straight_channel(source[1], factor);
                destination[2] = straight_channel(source[0], factor);
            }
            destination[3] = ((unsigned)alpha * alpha_max + 127) / 255;
            source += 4; destination += 4;
        }
        count -= block;
    }
}

#define API_FUNCTIONS(X) \
 X(FY_TDE2_Open) X(FY_TDE2_Close) X(FY_TDE2_BeginJob) X(FY_TDE2_EndJob) \
 X(FY_TDE2_CancelJob) X(FY_TDE2_QuickFill) X(FY_TDE2_QuickCopy) X(FY_TDE2_Bitblit) \
 X(FY_MPI_SYS_MmzAlloc_Cached) X(FY_MPI_SYS_MmzFree) X(FY_MPI_SYS_MmzFlushCache)
#define FIELD(name) __typeof__(&name) name;
typedef struct { API_FUNCTIONS(FIELD) } Api;
#undef FIELD

typedef struct { int32_t sx, sy, dx, dy; uint32_t width, height; } BridgeRect;
typedef struct { uint32_t phy; void *ptr; uint32_t width, height, pitch, rows, pad_x, pad_y; int opaque; } Buffer;
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
    int release_failed = 0;
    Buffer *buffers[] = { &e->source[0], &e->source[1], &e->target };
    for (unsigned i = 0; i < 3; ++i) {
        if (!buffers[i]->ptr) continue;
        int result = e->api.FY_MPI_SYS_MmzFree(buffers[i]->phy, buffers[i]->ptr);
        if (result) {
            release_failed = 1;
            fprintf(stderr, "slint-bridge: MMZ release failed (%d); TDE recreation disabled\n", result);
        }
    }
    if (e->opened) e->api.FY_TDE2_Close();
    if (e->library) dlclose(e->library);
    free(e);
    /* MmzFree can partially unmap/free before failing. Never retry an address
     * or allocate another engine that could accumulate uncertain allocations.
     * DMA is already known complete, so closing TDE/SDK references remains safe. */
    if (!release_failed) __sync_lock_release(&in_use);
}

Engine *bridge_tde_create(uint32_t width, uint32_t height, int *error) {
    *error = -1;
    /* Reserve room for page-aligned cache ranges without 32-bit wraparound. */
    if (!width || !height || (uint64_t)width * height > (UINT32_MAX - 4095u) / 4u ||
        (width & 3u) || (height & 1u)) return NULL;
    uint64_t source_bytes = ((uint64_t)width + 4) * ((uint64_t)height + 2) * 4;
    if (source_bytes > UINT32_MAX - 4095u) return NULL;
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
    memset(e->target.ptr, 0, e->bytes);
    for (unsigned i = 0; i < 2; ++i) {
        *error = e->api.FY_MPI_SYS_MmzAlloc_Cached(&e->source[i].phy, &e->source[i].ptr, "bridge_tde_source", NULL, (uint32_t)source_bytes);
        if (*error || !e->source[i].ptr) goto fail;
    }
    *error = 0;
    return e;
fail:
    if (!*error) *error = -1;
    bridge_tde_destroy(e);
    return NULL;
}

/* Borrowed CPU mapping. The Engine owns it; no access while DMA is unknown. */
void *bridge_tde_pixels(Engine *e) { return e->target.ptr; }

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

/* Return the shared canvas to CPU only after successful wait and cache sync. */
static int finish_job(Engine *e, TDE_HANDLE job,
    const BridgeRect *rects, unsigned count, uint32_t first, uint32_t last,
    int opaque, int *error) {
    *error = e->api.FY_TDE2_EndJob(job, FY_TRUE, FY_TRUE, 100); /* 1 second */
    if (*error) { e->quarantined = 1; return -2; } /* Cancel is invalid after EndJob. */
    *error = e->api.FY_MPI_SYS_MmzFlushCache(e->target.phy + first, (uint8_t *)e->target.ptr + first, last - first);
    if (*error) return -1; /* Retired, but the whole frame must be repainted. */
    uint8_t *pixels = e->target.ptr;
    for (unsigned i = 0; opaque && i < count; ++i) {
        const BridgeRect *r = &rects[i];
        for (uint32_t y = 0; y < r->height; ++y) {
            size_t at = ((size_t)(r->dy + y) * e->width + r->dx) * 4;
            for (uint32_t x = 0; x < r->width; ++x) pixels[at + x * 4 + 3] = 255;
        }
    }
    return 0;
}

/* Fill replaces pixels; blending translucent rectangles is a separate operation. */
int bridge_tde_fill(Engine *e, uint32_t color,
    const BridgeRect *rects, unsigned count, int *error) {
    *error = 0;
    if (e->quarantined) return -2;
    if (!count || count > 16) return 2;
    uint32_t top = e->height, bottom = 0;
    for (unsigned i = 0; i < count; ++i) {
        const BridgeRect *r = &rects[i];
        if (r->dx < 0 || r->dy < 0 || !r->width || !r->height ||
            (uint64_t)r->dx + r->width > e->width ||
            (uint64_t)r->dy + r->height > e->height) return 2;
        if ((uint32_t)r->dy < top) top = r->dy;
        if ((uint32_t)r->dy + r->height > bottom) bottom = r->dy + r->height;
    }
    uint32_t first = (top * e->width * 4) & ~4095u;
    uint32_t last = (bottom * e->width * 4 + 4095u) & ~4095u;
    if (last > e->bytes) last = e->bytes;
    *error = e->api.FY_MPI_SYS_MmzFlushCache(e->target.phy + first,
        (uint8_t *)e->target.ptr + first, last - first);
    if (*error) return -1;
    TDE_HANDLE job = e->api.FY_TDE2_BeginJob();
    if (job < 0) { *error = job; return -1; }
    TDE2_SURFACE_S d = surface(e->target.phy, e->width, e->height);
    for (unsigned i = 0; i < count; ++i) {
        const BridgeRect *r = &rects[i];
        TDE2_RECT_S dr = {r->dx, r->dy, r->width, r->height};
        *error = e->api.FY_TDE2_QuickFill(job, &d, &dr, color);
        if (*error) {
            if (e->api.FY_TDE2_CancelJob(job)) { e->quarantined = 1; return -2; }
            return -1;
        }
    }
    return finish_job(e, job, rects, count, first, last, 0, error);
}

/* At most a one-pixel fringe on each side; source is already premultiplied.
 * Called only after DMA retirement and cache ownership has returned to CPU. */
static void blend_cpu_rect(Engine *e, const Buffer *src, BridgeRect r) {
    for (uint32_t y = 0; y < r.height; ++y) {
        uint8_t *d = (uint8_t *)e->target.ptr + ((size_t)(r.dy + y) * e->width + r.dx) * 4;
        const uint8_t *s = (const uint8_t *)src->ptr +
            ((size_t)(r.sy + src->pad_y + y) * src->pitch + r.sx + src->pad_x) * 4;
        for (uint32_t x = 0; x < r.width; ++x, d += 4, s += 4) {
            for (unsigned c = 0; c < 3; ++c) d[c] = s[c] + d[c] * (255 - s[3]) / 255;
            d[3] = 255;
        }
    }
}

/* 0 copied, 1 blended, 2 unsupported (untouched), -1 safe error, -2 unknown DMA.
 * Known failure requires a full software repaint; unknown DMA forbids access. */
int bridge_tde_draw(Engine *e, const uint8_t *data,
    uint32_t width, uint32_t height, uint32_t stride, int premultiplied, uint8_t alpha,
    unsigned slot, int upload, const BridgeRect *rects, unsigned count, int *error) {
    *error = 0;
    if (e->quarantined) return -2;
    if (!width || !height || width > e->width || height > e->height || (width & 3u) ||
        (height & 1u) || stride < width * 4 || slot > 1 || !count || count > 16) return 2;
    Buffer *src = &e->source[slot];
    const uint8_t *pixels = e->target.ptr;
    for (unsigned i = 0; i < count; ++i) {
        const BridgeRect *r = &rects[i];
        if (r->sx < 0 || r->sy < 0 || r->dx < 0 || r->dy < 0 || !r->width || !r->height ||
            (uint64_t)r->sx + r->width > width || (uint64_t)r->sy + r->height > height ||
            (uint64_t)r->dx + r->width > e->width || (uint64_t)r->dy + r->height > e->height) return 2;
    }
    uint32_t pad_x = (uint32_t)(rects[0].dx - rects[0].sx) & 1u;
    uint32_t pad_y = (uint32_t)(rects[0].dy - rects[0].sy) & 1u;
    if (upload) {
        src->width = src->height = 0;
        src->pad_x = pad_x; src->pad_y = pad_y;
        src->pitch = (width + pad_x + 3u) & ~3u;
        src->rows = (height + pad_y + 1u) & ~1u;
        if (pad_y) memset(src->ptr, 0, src->pitch * 4);
        if (src->rows > height + pad_y)
            memset((uint8_t *)src->ptr + (size_t)(height + pad_y) * src->pitch * 4, 0, src->pitch * 4);
        src->opaque = alpha == 255;
        for (uint32_t y = 0; y < height; ++y) {
            const uint8_t *row = data + (size_t)y * stride;
            uint8_t *out = (uint8_t *)src->ptr + (size_t)(y + pad_y) * src->pitch * 4;
            if (pad_x) memset(out, 0, 4);
            bridge_prepare_rgba(out + pad_x * 4, row, width, premultiplied, alpha);
            memset(out + (width + pad_x) * 4, 0, (src->pitch - width - pad_x) * 4);
            for (uint32_t x = 0; x < width && src->opaque; ++x)
                if (row[x * 4 + 3] != 255) src->opaque = 0;
        }
        *error = e->api.FY_MPI_SYS_MmzFlushCache(src->phy, src->ptr, src->pitch * src->rows * 4);
        if (*error) return -1;
        src->width = width; src->height = height;
    } else if (src->width != width || src->height != height) return 2;
    int blend = !src->opaque || alpha != 255;
    BridgeRect gpu[16], interiors[16];
    unsigned gpu_count = 0;
    for (unsigned i = 0; i < count; ++i) {
        const BridgeRect *r = &rects[i];
        BridgeRect inner = *r;
        if (blend) {
            if (src->pad_x != ((uint32_t)(r->dx - r->sx) & 1u) ||
                src->pad_y != ((uint32_t)(r->dy - r->sy) & 1u)) return 2;
            uint32_t left = (uint32_t)(r->dx + 1) & ~1u;
            uint32_t top = (uint32_t)(r->dy + 1) & ~1u;
            uint32_t right = ((uint32_t)r->dx + r->width) & ~1u;
            uint32_t bottom = ((uint32_t)r->dy + r->height) & ~1u;
            inner.sx += left - r->dx; inner.sy += top - r->dy;
            inner.dx = left; inner.dy = top;
            inner.width = right > left ? right - left : 0;
            inner.height = bottom > top ? bottom - top : 0;
        }
        interiors[i] = inner;
        if (inner.width && inner.height) {
            inner.sx += src->pad_x; inner.sy += src->pad_y;
            gpu[gpu_count++] = inner;
        }
    }
    if (!gpu_count) return 2;
    uint32_t top = e->height, bottom = 0;
    for (unsigned i = 0; i < count; ++i) {
        const BridgeRect *r = &rects[i];
        if ((uint32_t)r->dy < top) top = r->dy;
        if ((uint32_t)r->dy + r->height > bottom) bottom = r->dy + r->height;
        if (blend)
            for (uint32_t y = 0; y < r->height; ++y) {
                size_t at = ((size_t)(r->dy + y) * e->width + r->dx) * 4;
                /* This profile blends only onto opaque UI pixels. Transparent
                 * destinations use Slint's exact alpha compositing in software. */
                for (uint32_t x = 0; x < r->width; ++x)
                    if (pixels[at + x * 4 + 3] != 255) return 2;
            }
    }
    uint32_t first = (top * e->width * 4) & ~4095u;
    uint32_t last = (bottom * e->width * 4 + 4095u) & ~4095u;
    if (last > e->bytes) last = e->bytes;
    *error = e->api.FY_MPI_SYS_MmzFlushCache(e->target.phy + first, (uint8_t *)e->target.ptr + first, last - first);
    if (*error) return -1;
    TDE_HANDLE job = e->api.FY_TDE2_BeginJob();
    if (job < 0) { *error = job; return -1; }
    TDE2_SURFACE_S s = surface(src->phy, src->pitch, src->rows), d = surface(e->target.phy, e->width, e->height);
    for (unsigned i = 0; i < gpu_count; ++i) {
        const BridgeRect *r = &gpu[i];
        TDE2_RECT_S sr = {r->sx, r->sy, r->width, r->height}, dr = {r->dx, r->dy, r->width, r->height};
        if (!blend) *error = e->api.FY_TDE2_QuickCopy(job, &s, &sr, &d, &dr);
        else {
            TDE2_OPT_S opt = {0};
            opt.stBlendOpt.bPixelAlphaEnable = FY_TRUE;

            /* Opacity is baked into the cached source. The SDK global-alpha
             * path does not match Slint's premultiplied RGB semantics. */
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
    int result = finish_job(e, job, gpu, gpu_count, first, last, blend, error);
    if (result < 0) return result;
    for (unsigned i = 0; blend && i < count; ++i) {
        const BridgeRect *r = &rects[i], *inner = &interiors[i];
        if (!inner->width || !inner->height) { blend_cpu_rect(e, src, *r); continue; }
        uint32_t left = inner->dx - r->dx, top = inner->dy - r->dy;
        uint32_t right = left + inner->width, bottom = top + inner->height;
        blend_cpu_rect(e, src, (BridgeRect){r->sx,r->sy,r->dx,r->dy,r->width,top});
        blend_cpu_rect(e, src, (BridgeRect){r->sx,r->sy+bottom,r->dx,r->dy+bottom,r->width,r->height-bottom});
        blend_cpu_rect(e, src, (BridgeRect){r->sx,r->sy+top,r->dx,r->dy+top,left,inner->height});
        blend_cpu_rect(e, src, (BridgeRect){r->sx+right,r->sy+top,r->dx+right,r->dy+top,r->width-right,inner->height});
    }
    return blend ? 1 : 0;
}
