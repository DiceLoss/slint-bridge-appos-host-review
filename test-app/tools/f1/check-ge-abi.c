/* Compile-only comparison of the Rust normal-mode ABI with reference SDK UAPI. */
#include <stddef.h>
#include <video/artinchip_ge.h>
#include <dma-heap.h>

_Static_assert(sizeof(struct mpp_buf) == 64, "mpp_buf size");
_Static_assert(offsetof(struct mpp_buf, fd) == 4, "mpp_buf union");
_Static_assert(offsetof(struct mpp_buf, crop) == 40, "mpp_buf crop");
_Static_assert(offsetof(struct mpp_buf, format) == 56, "mpp_buf format");
_Static_assert(sizeof(struct ge_bitblt) == 168, "bitblt size");
_Static_assert(offsetof(struct ge_bitblt, dst_buf) == 64, "bitblt destination");
_Static_assert(offsetof(struct ge_bitblt, ctrl) == 128, "bitblt control");
_Static_assert(IOC_GE_BITBLT == 0x40a84703, "bitblt ioctl");
_Static_assert(sizeof(struct ge_ctrl) == 40, "ge_ctrl size");
_Static_assert(sizeof(struct ge_fillrect) == 116, "ge_fillrect size");
_Static_assert(offsetof(struct ge_fillrect, dst_buf) == 12, "fill destination");
_Static_assert(offsetof(struct ge_fillrect, ctrl) == 76, "fill control");
_Static_assert(sizeof(struct dma_heap_allocation_data) == 24, "allocation size");
_Static_assert(MPP_FMT_ABGR_8888 == 1, "RGBA memory format");
_Static_assert(MPP_DMA_BUF_FD == 0, "DMA-BUF buffer kind");
_Static_assert(MPP_BUF_IS_PREMULTIPLY == (1 << 8), "premultiplied flag");
_Static_assert(IOC_GE_VERSION == 0x80044700, "version ioctl");
_Static_assert(IOC_GE_MODE == 0x80044701, "mode ioctl");
_Static_assert(IOC_GE_FILLRECT == 0x40744702, "fill ioctl");
_Static_assert(DMA_HEAP_IOCTL_ALLOC == 0xc0184800, "allocation ioctl");
