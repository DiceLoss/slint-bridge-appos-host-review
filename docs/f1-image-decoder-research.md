# F1 图片硬件解码调查

调查日期：2026-10-19。对象为本次 F1 / D211、RISC-V64、Linux 5.10.44 #1。
本报告记录静态资料和实际设备库的只读核对，**未运行 VE 解码、未证明硬解性能或异常恢复**。
显示与 GE 的既有验证范围见 [F1 显示调查](f1-display.md) 和 [F1 性能记录](f1-optimization.md)。
这是接入前调查的范围；随后完成的原生实现、像素与性能验证另见 [本轮实测](f1-optimization.md#f1-scalar-png)。

## 结论与接入顺序

F1 有独立于 CPU 指令集的 VE PNG/JPEG 解码器。当前 showcase 的背景和五张 PNG 都是
8-bit RGB/RGBA、非隔行，适合先验证 PNG；SVG 继续走 Slint 已有加载与缓存。
硬解改善图片加载和首帧准备，不能据此推断已缓存图片的稳态渲染 FPS 会提高。
推荐先做只处理上述 PNG 子集的 F1 原生加载适配器，成功后生成普通 Slint `Image`，
将解码耗时与渲染耗时分别记录。不要在每次 `draw_texture()` 时重新解码。

**不能直接调用现有 `libmpp_decoder.so` 并把 `decode()==0` 当作成功。** 实际设备库和
官方参考版本都会忽略 PNG HAL 的失败返回，将失败帧送入 render list；见下文。
原生适配器可避开这个问题并省去 MPP packet 缓冲，但仍须明确 VE 所有权、IRQ 归属及
非确定完成时保留 DMA 资源的规则。未知完成状态不可转换为 CPU 回退或正常释放。

## 一手来源与实际库

- [D21x 用户手册](https://aicdoc.artinchip.com/out/downloads/D21x_User_Manual.pdf)，
  本地副本 Version 2.0.4，2026-07-17；SHA256 与 [显示报告](f1-display.md) 相同。
  本文页码均为印刷页码。10.3 节说明 PNG/JPEG 能力，10.3.6 描述寄存器。
- [官方 Luban](https://github.com/artinchip/luban/tree/13fa021285a307ca345a6b05e0da371b635f18eb)，
  固定提交 `13fa021285a307ca345a6b05e0da371b635f18eb`，v1.2.9。
  调查时 GitHub `master` 仍指向这个提交，未发现该分支上的后续 PNG 修复。
- [MPP Decoder 接口说明](https://aicdoc.artinchip.com/topics/sdk/mpp/mpp-decode-luban.html)
  可解释 packet/frame 生命周期，但其示例函数名、错误码与当前源码并非全部一致；
  不能直接将网页或旧工程头文件用作当前固件 ABI。

主任务从 `/system/clib/` 只读取得设备库，临时放在被忽略的 `target/f1-decoder-probe/`。
三份库与本机旧 F1 工程的对应文件逐字节哈希相同：

| 库 | SHA256 |
| --- | --- |
| `libmpp_decoder.so` | `5c91b1530aed15930bcc40d985fcf9a04a952167db6b0f5212017230be27c4c5` |
| `libmpp_base.so` | `3b57258b19b2ff2826af6514df5d06cceca81be0aa4b740e2c38be305bfa6ef8` |
| `libmpp_ve.so` | `9e2e9d244fec648a61e003bd7b0e0b82cd0b168bd24514967937064f56ecf6cf` |

设备 decoder ELF 为 RISC-V64 little-endian、RVC、double-float ABI。其 DWARF 包含
`bsp_V1.2/source/artinchip/aic-mpp/` 源路径，编译标志含 `-march=rv64imafdc_xtheadc`
和 `-Os -O0`；这只能识别构建线索，不能据此指定一个官方 Git 提交。
动态依赖是 `libmpp_base.so`、`libmpp_ve.so`、`libc.so.6`；PNG 不需要 GE/encoder 库。
官方新版本 decoder 的构建依赖另含 `dl`，见 [CMakeLists](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/ve/decoder/CMakeLists.txt#L18)。

本轮只提取 ABI、函数地址和控制流结论，不将 ELF、原始像素或反汇编文件归档到 `evidence/`。
设备库后续清理后，复核需重新取得相同 SHA256 的文件；官方源码可由固定链接重取。

## 支持格式、布局和内存

手册 p.404–405 的 JPEG 能力是 8-bit baseline、一个或三个分量，支持若干 YUV 采样、
缩小和旋转；不把它扩展解释为 progressive JPEG 支持。
官方 [JPEG 实现](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/ve/decoder/jpeg/mjpeg_decoder.c#L277)
按码流采样选 YUV/NV 输出；[单图示例](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/mpp_test/picture_decoder_test.c#L365)
明确不直接做 JPEG YUV→RGB。Slint 所需 RGB/RGBA 因而还要转换，新增输出缓冲和 GE 完成路径。
首轮先不加入 JPEG。

PNG 先限定 bit depth=8、color type=2/6、compression/filter method=0、interlace=0。
手册关于索引色低位深支持存在表述差异，且寄存器章节只定义 8-bit，本轮不依赖它。
不透明 RGB 原图输出 alpha 格式时补 255；有 alpha 的原图输出 RGB 会丢失 alpha，见手册 p.464。
官方 [PNG HAL](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/ve/decoder/png/png_hal.c#L128)
支持多种输出通道排列。选择 `MPP_FMT_ABGR_8888=1` 才是小端内存中的 R,G,B,A 字节；
`MPP_FMT_ARGB_8888=0` 是 B,G,R,A。输出为普通 alpha，不能标记成预乘 alpha。
RGBA 行步长是 `ALIGN_8B(width*4)`，见 [stride/frame 初始化](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/ve/decoder/png/png_decoder.c#L425)。
应使用实际 stride/crop，不将 padding 当图像内容。

当前文件头检查结果如下；这是本机素材检查，尚非硬解成功结果。

| 素材 | 尺寸 | 编码长度 | IHDR |
| --- | --- | ---: | --- |
| 构建后的 `coastal-background.png` | 1024×600 | 934,907 B | 8-bit RGBA，非隔行 |
| `scaling-reference.png` | 256×160 | 2,805 B | 8-bit RGBA，非隔行 |
| `rich-row-blue/amber.png` | 800×112 | 14,081 / 14,049 B | 8-bit RGBA，非隔行 |
| `rich-card-blue/amber.png` | 244×328 | 19,489 / 18,792 B | 8-bit RGBA，非隔行 |

原始 1638×960 背景为 2,184,761 B，运行时应继续使用构建出的 1024×600 资源。
官方 PNG 分配一个输出 frame；其 `extra_frame_num` 不参与 PNG frame 数计算。
默认 MPP 的物理缓冲约为：packet 容量 + 编码文件长度的 IDAT 暂存 + 输出 stride×height
+ 32 KiB LZ77 + width×channels + 可选 1 KiB palette。
实际 DMA 分配还按页取整，见 [PNG 分配](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/ve/decoder/png/png_decoder.c#L37)
和 [packet 分配](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/ve/decoder/packet_manager.c#L53)。
1024×600 的 MPP RGBA 路径约需 4.17 MiB DMA；再复制到 Slint heap 会增加 2.344 MiB 峰值，
尚不含编码资源页和库自身。不要直接采用旧封装的固定大码流缓存。

原生限定 RGB/RGBA 路径可以直接将各 IDAT 合并进一个 DMA 缓冲，省掉 MPP packet 副本；
palette/filter-line 缓冲只用于索引色的官方路径，本子集不需要它。
因此基础 DMA 预算约为 IDAT 长度 + 32 KiB + stride×height，背景约 3.27 MiB 上界，
再加 Slint 复制约 2.344 MiB。以上为源码推导，不是 RSS/PSS/CMA 实测。

## SDK ABI 与不可忽略的失败路径

设备 DWARF 核验的公开布局为：

| 类型 | 大小 | 字段偏移（字节） |
| --- | ---: | --- |
| `decode_config` | 16 | pix_fmt 0；bitstream_buffer_size 4；packet_count 8；extra_frame_num 12 |
| `mpp_packet` | 32 | data 0；size 8；pts 16；flag 24 |
| `mpp_buf` | 64 | buf_type 0；fd/phy union 4；stride 16；size 28；crop_en 36；crop 40；format 56；flags 60 |
| `mpp_frame` | 80 | buf 0；pts 64；id 72；flags 76 |

错误/等待状态为 `0` 成功、`1..4` 分别缺 render frame/empty frame/ready packet/empty packet，
`-1/-2/-3` 是不支持/空指针/frame manager 未建。控制命令外部分配器=0、旋转=1、缩放=2、
crop=3、输出位置=4。旧 F1 工程头文件仍采用 `0x9000000*` 错误和外部分配器=4，不能混用。
这些值也可与 [当前官方类型头](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/include/mpp_dec_type.h#L116) 对照。

实际设备 `decode_frame_common()` 在 `0x976a` 调用 `png_hardware_decode@plt`，
随后不检查返回寄存器，`0x97e2` 将帧加入 render list，`0x97fa` 返回 0。
官方 [对应调用](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/ve/decoder/png/png_decoder.c#L456)
也吞掉该返回值。实际 HAL 和官方 [等待逻辑](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/ve/decoder/png/png_hal.c#L74)
均在超时/ERR 后延时并调用 reset，但不检查 reset 成功；BITREQ 返回失败而不 reset；
未匹配任何状态时还会返回 0。PNG `reset()` 本身只是空实现。

`IOC_VE_WAIT` 的参考驱动在超时后返回 `-ETIMEDOUT`，没有停止硬件；`IOC_VE_RESET`
只执行 reset assert/deassert，忽略二者返回且不等待复位状态，见
[驱动 WAIT/RESET](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/ve/aic_ve.c#L459)。
这些行为尚未与当前设备内核逐函数对应。即使上层补查 HAL 返回，仍不能把错误解释成
“DMA 已停止、可释放”。像素比较、固定等待和进程退出均不补足这个契约。

成功的标准 MPP 生命周期应是 create → init → get/填/put packet → decode → get frame →
所有消费者完成 → put frame → destroy。输出 DMA fd 属于 frame manager，不是调用者可任意关闭的 fd。
CPU 读取前还须 invalidate；直接给 GE 时须保留 frame 直到 GE 完成。
[frame manager 销毁](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/ve/decoder/frame_manager.c#L291)
会移除导入并释放所有帧，外部分配器也会被 close，不能先 destroy 再保留裸指针。
[DMA 同步实现](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/base/memory/dma_allocator.c#L108)
中 clean 对应 WRITE|END，invalidate 对应 READ|START；同步成功不代表硬件任务完成。

## 原生 PNG 适配器的最小边界

适配器放在 F1 后端，直接使用已核验的 DMA heap/ioctl；不加载旧 MPP、不做符号 hook，
不改 Slint 私有核心。先单图、单任务、单线程；对外只提供“编码 PNG → 已完成的普通 Image”。
`Config::prepare_background(Image)` 已接收解码结果，无法拦截解码；应另设明确的编码数据加载入口，
或先在测试应用原生 helper 中验证再决定公共接口。稳态渲染仍走原来的通用路径。

1. **提交前解析并限额。** 验证 PNG 签名、IHDR 唯一性/长度、checked 长宽和 stride 乘法、
   完整 chunk 边界、CRC、IEND、连续 IDAT、zlib CMF/FLG、无预置字典和足够的头尾长度。
   不支持的位深/颜色/隔行、APNG、RGB+tRNS、影响解码语义而未实现的关键 chunk 在硬件提交前
   返回“不支持”并走现有软件加载。不要让不支持的透明色被静默变成不透明。
   先针对既有素材限制到 1024×600 和明确的 encoded/output 上限，不能只依赖寄存器位宽。
2. **独占和分配。** 重新核验设备身份、VE fd 所有者、恢复方式；持有进程间锁并确认没有其他 VE 用户。
   `GET_CLIENT` 只是设备仲裁接口，参考驱动的检查和置位未由同一个互斥区保护，且 `PUT_CLIENT`
   没检查调用者身份，不能把它当成强隔离保证。
   打开 `/dev/aic_ve`，`GET_INFO` 核验映射范围，mmap 寄存器；分配输入/LZ77/输出 DMA 并
   `ADD_DMA_BUF` 取得设备地址，逐一检查成功和地址范围。完成后 `RM_DMA_BUF`、munmap、close。
3. **准备输入和缓存。** 直接把各 IDAT 拷到单一 DMA 缓冲，保留完整 zlib 数据但硬件输入 offset=16 bit、
   length=(IDAT 总长−6)×8，与官方只解 deflate 的流程一致。分配长度留对齐/预取余量、尾部清零；
   `INPUT_BS_START` 低 4 位为 0，`END` 低 4 位为 15，LZ77 地址需 1024 对齐。
   官方 [首行 filter workaround](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/ve/decoder/png/png_decoder.c#L333)
   会先清空输出最后一行并 clean，原生实现须保留并验证。硬件前清理输入/CPU 写过的输出 cache；
   读回必须在确定完成后 invalidate。所有 MMIO 使用有序的 32-bit 访问，不能仅加 Rust/C volatile；
   [厂商 RISC-V 访问封装](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/artinchip/aic-mpp/ve/include/ve_top_register.h#L14)
   对写前用 `fence w,o`，读后用 `fence i,r`。
4. **配置和启动。** 仅在独占成功后复位/初始化本模块；明确等待 reset 完成，超时禁止启动。
   配置 color type、8-bit 编码值 0、RGBA 输出排列、stride、地址、长度、LZ77。
   清除历史硬件状态和驱动可能残留的 IRQ 标志，再启用本次中断、启动并提交 FIRST|LAST|VALID。
   参考驱动的 `GET_CLIENT` 不清 IRQ flag，必须验证 drain 的行为；不能收到前一任务中断就发布新图。
5. **完成或隔离。** `WAIT` 返回的 `reg_status` 必须明确包含 FINISH 且没有 ERR/BITREQ/其他错误；
   再核验输出计数、格式和已知图像校验。驱动 IRQ 已清除 W1C 状态，不能在 WAIT 后读寄存器 0
   当作成功证据。只有完成后才可 CPU 读/复制、PUT_CLIENT 和释放临时 DMA。
   WAIT 失败、BITREQ、混合/未知状态或无法确认完成时，把 fd、映射、所有 DMA 和 VE 所有权一起
   quarantine，拒绝后续任务与软件回退；仅保留部分输出还不够。进程退出会触发内核 release，
   因而异常恢复须另行核验，不能用杀死测试进程代替 DMA 停止。

以上步骤中的寄存器最小集合如下；偏移相对 VE mmap 基址，来自官方 HAL 和手册。

| 寄存器 | 偏移 | 用途 |
| --- | --- | --- |
| VE_CLK / VE_RST / VE_INIT / VE_IRQ / PNG_EN | 0x000 / 0x004 / 0x008 / 0x00c / 0x018 | 模块时钟、复位、初始化、中断、PNG 使能 |
| INFLATE_INT / STATUS / START | 0xc00 / 0xc04 / 0xc08 | 中断/FINISH=1、ERR=2、BITREQ=4/启动 |
| CTRL / PNG_SIZE / STRIDE / FORMAT | 0xc10 / 0xc14 / 0xc18 / 0xc1c | 模式、尺寸、行步长、输出排列 |
| BS_START / BS_END / BS_OFFSET / BS_LENGTH | 0xc20 / 0xc24 / 0xc28 / 0xc2c | 压缩输入地址和位长度 |
| OUTPUT_ADDR / MAX_LENGTH / COUNT | 0xc30 / 0xc34 / 0xc38 | 输出地址、容量、计数 |
| WINDOW_ADDR / DATA_VALID | 0xc40 / 0xc48 | 32 KiB LZ77、FIRST|LAST|VALID |
| INFLATE_RESET | 0xc50 | 官方 HAL 使用的 PNG 复位 |

仍有两处必须单独核验：手册 p.421 的 PIC reset 位写为 [7:6]，官方 HAL 用 [9:8]；
驱动用 `PNG_BUSY=0xc54` 做退出检查，但这份手册没有对应公开定义。
因此不凭这个未说明的 busy 位或 reset 固定循环次数宣布异常 DMA 已退休。
手册 p.461 明确区分 FINISH“完成”、ERR“停止任务”和 BITREQ“暂停任务”，支持成功路径判断，
但仍需核对本固件 IRQ 状态传递、清除次序及独占前提。

## 有界 CPU / VE 微基准

- 先跑 256×160 scaling PNG 和 17×13 等小尺寸 RGB/RGBA fixture，再跑实际全屏背景；
  逐步覆盖所有 filter、alpha=0/1/254/255、奇数宽、多个 IDAT。编码非法和不支持类型的拒绝
  在主机解析测试中验证，不为性能测试向未知恢复能力的硬件故意提交坏码流。
- 每组在同一时钟/负载条件下先预热，再记录至少 20 次的中位数/p95；区分编码读取、分配、
  IDAT 合并与 clean、硬解等待、invalidate、Slint 复制和总时间。与相同编码输入的现有
  `Image::load_from_data` 比较，并单列冷启动；不要只比较 VE 寄存器周期计数。
- CPU 参考与硬解逐字节比较有效像素，可用小块行比较避免同时再留一张全屏参考。
  正确性比较只验证结果，不替代完成确认。记录每次完成状态和输出计数；任何非确定完成立即停止。
- 记录 `/proc/self/status` 的 VmHWM、smaps_rollup RSS/PSS、系统 MemAvailable，以及 reserved
  DMA heap 可用量/分配失败与回收；CmaFree 不一定代表实际 reserved heap 余量。
  初始化前/图像返回后/销毁后各采样，检查反复加载是否泄漏。源码预算与实测分别标注。
- 硬解在首帧画布与大缓存分配前执行，可以降低并存峰值；先采用完成后按行复制到普通
  SharedPixelBuffer 的简单入口。零复制需要公开 external-pixel-buffer 和明确的 DMA owner，
  或外部分配器转移所有权；它减少堆复制，却增加生命周期/stride 限制，不作为第一步的前提。
- 测试产物放 `target/` 或 `/tmp/`，仅归档小型计时/状态/哈希摘要。不保存固件、原始像素帧或反汇编。

此报告未更改运行代码、未接管显示、未调用 VE，也未宣称硬解可以安全处理异常返回。
