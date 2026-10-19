/* Native decoder request layouts, checked against the pinned F1 UAPI. */
#include <stddef.h>
#include <video/artinchip_ve.h>
_Static_assert(sizeof(struct ve_info) == 4, "VE info ABI");
_Static_assert(sizeof(struct wait_info) == 8, "VE wait ABI");
_Static_assert(offsetof(struct wait_info, reg_status) == 4, "VE status offset");
_Static_assert(sizeof(struct dma_buf_info) == 8, "VE DMA import ABI");
_Static_assert(IOC_VE_GET_CLIENT == 0x5600, "get client request");
_Static_assert(IOC_VE_PUT_CLIENT == 0x5601, "put client request");
_Static_assert(IOC_VE_WAIT == 0xc0085602, "wait request");
_Static_assert(IOC_VE_GET_INFO == 0x80045603, "info request");
_Static_assert(IOC_VE_ADD_DMA_BUF == 0xc0085606, "import request");
_Static_assert(IOC_VE_RM_DMA_BUF == 0x40085607, "release request");
