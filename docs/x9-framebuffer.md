# X9 / Lombo N7 纯软件 framebuffer 接入调查

外部源码引用使用采集时的仓库名、相对路径和行号，版本由正文注明；这些调查材料不随本仓库分发，也不是当前构建前提。

本文保留 2026-10-14 接入阶段的硬件依据和验证。后续性能见 [X9 性能记录](x9-performance.md#x9-performance) 和 [Slint 对照](slint-optimization.md#x9-texture-neon)，实际部署与恢复按 [设备说明](../test-app/devices/x9/README.md) 重新核对。

## 驱动完成契约与接入范围

本节记录 2026-10-14 对新设备显示完成语义的只读源码调查。目标不是 NX5：主任务现场只读识别为 LomboTech-N7、双 Cortex-A7、Linux `3.10.100 #452`（2022-12-12），`fb0` 绑定 `lombo-drm`，800×1280、32 位像素、stride 3200、虚拟高度 2560。该设备的 `/proc/kallsyms` 存在 `drm_fb_helper_pan_display`、`lombo_crtc_mode_set_base`、`lombo_crtc_page_flip`、`lombo_crtc_handle_vblank_events`、`lombo_dc_page_flip_worker`、`lombo_crtc_commit_sw`；这些名字说明存在相关实现，不能证明某个 ioctl 的完成含义或实际调用链。身份与布局应在每次实机操作前重新读取。

没有取得匹配固件的厂商源码，因此先核对运行内核已知代码区的反汇编，再对标准 DRM legacy flip 完成接口做目标机验证。**PAN 返回本身不是完成条件**；本次接入只接受该固件，使用同一 framebuffer 的匹配 flip 事件。当前实现及失败处理见 [X9 后端](../src/backends/x9/mod.rs) 和 [平台说明](software-platform.md#准入边界)。其他 X9 用户态项目的注释、固定延时、页号读回或一次 VSYNC 均不作为替代完成证据。

### 通用 Linux 接口能证明什么

Linux 上游 v3.10 的 `drm_fb_helper_pan_display()` 给模式设置结构写入 `xoffset/yoffset`，调用 `drm_mode_set_config_internal()`，成功后更新 `info->var`，随后返回。该 helper 没有独立的提交序号、等待扫描地址锁存或旧页退休的逻辑。等待如果存在，必须由其下游板级实现提供；不能从函数名或 DRM 包装层推断。[上游 `drm_fb_helper.c`，762–795 行](https://raw.githubusercontent.com/torvalds/linux/v3.10/drivers/gpu/drm/drm_fb_helper.c)

同版本 `fb_pan_display()` 校验偏移后调用驱动 `fb_pan_display`，再更新 `info->var.xoffset/yoffset`；`FBIOGET_VSCREENINFO` 读取的是这个内存结构。因而页号读回可用于发现错误偏移，不能单独证明硬件已经停止访问旧页。以上是上游通用路径的源码事实，不是目标厂商内核的等同证明。[上游 `fbmem.c`，827–858、1029–1034 行](https://raw.githubusercontent.com/torvalds/linux/v3.10/drivers/video/fbmem.c)

标准 fbdev 没有与这一笔 PAN 绑定的完成事件队列。v3.10 的 `fb_fops` 不提供事件 `poll`/`read`，其 `read` 用来读取像素；`FBIO_WAITFORVSYNC`、`FBIOGET_VBLANK` 未由通用 ioctl 分支实现，而是转给驱动自己的 `fb_ioctl`。`FB_ACTIVATE_VBL` 只是请求下个 vblank 激活的标志；该 DRM fbdev helper 没有读取该标志，因此也不能据此增加通用完成保证。[同一上游 `fbmem.c`，1132–1140、1395–1413 行](https://raw.githubusercontent.com/torvalds/linux/v3.10/drivers/video/fbmem.c)、X9 工具链 fb.h（`X9-sysroot/usr/include/linux/fb.h:27`）。

纯软件绘制不要求使用 G2D，但也不免除扫描输出的完成同步。DRM 的 legacy page flip 另有与请求绑定的完成事件：提交 `DRM_MODE_PAGE_FLIP_EVENT` 后，完成时返回携带对应 `user_data` 的 `DRM_EVENT_FLIP_COMPLETE`。这与任意一次 `DRM_EVENT_VBLANK` 不同，不能在 fbdev PAN 后等待一个无关联 DRM vblank 就声称完成。X9 工具链自带 drm_mode.h:463（`X9-sysroot/usr/include/drm/drm_mode.h:463`） 和 drm.h:800（`X9-sysroot/usr/include/drm/drm.h:800`） 描述了该 UAPI；其 `linux/version.h` 是 4.0 头文件版本，不能替代 3.10.100 厂商实现的核验。Linux 官方文档也区分 vblank 与 page-flip completion 事件：[DRM 文件操作与事件](https://www.kernel.org/doc/html/v6.14/gpu/drm-internals.html#open-close-file-operations-and-ioctls)。

官方 legacy `page_flip` 契约明确要求：驱动在 flip 完成时发送随请求传入的事件，应用收到该事件后才复用旧 buffer；开启 `DRM_MODE_PAGE_FLIP_ASYNC` 会取消等待 vblank、可能撕裂，因此不能采用。该文档明确讨论 legacy 接口，提供的是接口规范；仍须与目标实现及实机观测结合，不能套用较新 atomic helper 的内部行为。[Linux KMS 的 CRTC functions，`page_flip`](https://www.kernel.org/doc/html/v6.14/gpu/drm-kms.html#crtc-functions-reference)

若实现此路径，至少应检查事件类型为 `DRM_EVENT_FLIP_COMPLETE`、事件长度合法、`user_data` 与本笔请求相符；普通 vblank、旧请求事件、错误事件、超时或读端断开均不能释放旧页。在旧内核 ABI 中事件末尾是 reserved，不能依赖较新内核的 `crtc_id` 字段；使用独占 fd 与请求 cookie 绑定，仍保持最多一笔在途。CPU mmap 写入也不会自动受到早期 UAPI 注释所说的 GPU `execbuffer` 阻塞保护。

若考虑 CPU 绘制到 DRM dumb buffers 并用带序号的 page-flip 事件提交，仍需核对该板驱动、布局、缓存一致性、事件/超时/退出行为，并实现对应板级后端；它不是给现有 fbdev PAN 追加一个通用等待。是否采用此接口取决于任务要求及后续验证，本次实现没有分配 DRM dumb buffer；渲染映射已有 `/dev/fb0` 两页，DRM 仅用于同一 FB ID 的完成事件。

### 本地既有代码的范围

已检索 `/home/dpower/data` 下可见工程源文件与文件名，重点包括 `GitLab/LGUI3`、`GitLab/slint_x9`、`DPower_OS`、`GitHub`、`Code` 及 X9 工具链；也查看了 `slint_x9.tar.gz` 和 `slint_x9_0430.tar.gz` 的归档目录。没有找到 `lombo_crtc*`、Lombo DRM framebuffer/显示控制器内核实现或匹配 BSP 的驱动源码。未将所有大体积固件/压缩档全部解包，此结论仅表示已检查范围中未找到。

| 既有源码 | 实际做法 | 对本次完成契约的意义 |
| --- | --- | --- |
| 旧 X9 framebuffer.rs:179（`slint_x9/framebuffer.rs:179`） | PAN 后调用一次 `FBIO_WAITFORVSYNC`，再换可写页 | 用户态使用范例；缺少驱动等待与锁存证明，不能原样作为板级完成依据 |
| 旧 X9 FramebufferManager:631（`slint_x9/src/framebuffer/mod.rs:631`） | PAN 后立即同步另一缓冲页 | 依赖 PAN 隐含阻塞假设；自身没有验证该假设 |
| LGUI3 通用显示代码:530（`LGUI3/c_library/src/lgui/lgui_disp/lgui_disp_fb.c:530`） | 按平台宏使用 PUT/PAN；部分分支另等 VSYNC | 面向多个平台的应用层逻辑，不是 Lombo 内核契约 |
| DpowerOS framebuffer:239（`DpowerOS/src/framebuffer/mod.rs:239`） | PAN 后切换页身份；VSYNC 不支持时允许忽略 | 现有项目策略与本仓库的严格退休要求不同，不能据此放宽当前要求 |

网上按精确符号和 Lombo/N7 BSP 关键词检索，未取得可核验的官方/厂商驱动实现。其他设备的启动日志或同芯片名称不构成该固件完成证据。

### 目标内核机器码的补充核对

在确认没有厂商源码后，主任务只读取得目标内核已知代码区的字节，并按同次 `/proc/kallsyms` 标注反汇编。本节未读取任意进程数据或显示 MMIO。原始本地证据在被忽略的 `evidence/x9-framebuffer/2026-10-14/`；仅检出仓库不会获得这些文件，重核本节需另行取得同批档案。

| 实际目标函数 | 观察到的行为 | 可以得出的结论 |
| --- | --- | --- |
| `drm_fb_helper_pan_display`，`c027798c` | 写 modeset x/y，并在 `drm_mode_set_config_internal()` 周围设置/清除厂商扩展标志；随后更新 `info->var`、解锁、返回 | 本函数本身没有提交完成等待。见 [helper 反汇编](../evidence/x9-framebuffer/2026-10-14/drm-helper-full-annotated.txt) |
| `drm_crtc_helper_set_config` 的普通偏移更新成功路径 | `c027a784/a790` 写 CRTC x/y，`c027a7c0` 调 `mode_set_base`；成功直接到 `c027a8f8` 释放临时快照，`c027a910` 返回 0 | 该成功路径没有 `mode_set_base` 之后的扫描等待；完整调用链仍需同时核对中间 wrapper `drm_mode_set_config_internal` |
| `lombo_crtc_mode_set_base`，`c0296f40` | 加保护→`lombo_dc_set_mode_base`→解除保护→返回 | 返回值只保留首次加保护结果；未传播后两个调用的返回值。[Lombo 反汇编](../evidence/x9-framebuffer/2026-10-14/lombo-annotated.txt) |
| `lombo_tcon_protect_dbreg`，`c02af2ec` | 加保护只在 `tcon+0x1f0` 计数为负时重试，否则递增；解除保护递减计数并立即返回 | 是与 IRQ 寄存器更新互斥的保护计数，不是等待本次新画面锁存。[TCON 反汇编](../evidence/x9-framebuffer/2026-10-14/tcon-annotated.txt) |
| `lombo_crtc_page_flip`，`c0296fcc` | 外层加保护→调用 mode_set_base→加入本笔事件→解除外层保护 | 保护避免 IRQ 在图像更新与事件入队之间提交；没有相同 FB 指针即跳过事件的分支 |
| `lombo_tcon_irq_handler`，`c02b0524` | 保护计数允许且当前扫描行早于截止值时执行 `lombo_dc_commit`，然后处理 flip 事件；过晚则跳过这次提交和事件 | 事件与一次允许执行的寄存器提交关联，比普通 VSYNC 通知更具体 |
| `lombo_dc_commit`→`csp_dc_load_dbr` | 后者只设置控制寄存器位 `0x300`；随后 IRQ 发事件 | 观察路径未调用独立的 `csp_dc_is_load_dbr_finish`。这不单独证明事件错误，但机器码本身没有给出该控制位与实际锁存/预取结束之间的硬件时间定义 |

反汇编中地址 `c057c6c0` 同时标有 `__sched_text_end` 等符号别名，实际是 `_raw_spin_lock_irqsave`，不能把它误读成一次调度等待。

在不分配第三张物理页的约束下，可进一步核验“PAN 设置 yoffset，随后对同一 DRM FB ID 提交带 cookie 的 page-flip 并等匹配事件”的路径：普通成功路径会先更新 CRTC x/y，目标 page-flip 会读取这些偏移并再次提交相同 framebuffer，同时入队对应事件。两次调用之间即使发生 IRQ，也必须始终保持两页不写，直到该事件确认。本路径在目标 Lombo callback 中未见同 FB ID 拒绝条件；后续 `flip-02` 探针已验证 ioctl 核心接受同一 FB ID 的提交、匹配事件及偏移恢复，见下方实机记录。

### 实机完成事件验证

2026-10-14 使用该设备 `/dev/fb0` 和 `/dev/dri/card0`：CRTC 16、DRM FB 26，未分配新物理页。先备份 8,192,000 字节双页内容及偏移，再用 `flip-02` 探针交替 PAN 到 0 / 1280，对同一 FB 发送带独立 cookie 的 page-flip。120 次交替及最后一次恢复均收到长度 32、类型 FLIP_COMPLETE、cookie 相符的事件，退出 0；探针不写 framebuffer 像素。

PAN 约 70 µs 返回，匹配完成约 15.7 ms，直接否定了 PAN 同步完成的假设。该驱动所有事件 sequence 均为 0，末尾是旧 ABI 的 reserved；实现依据独占 fd 与 64 位 cookie，不要求这些字段递增或等于 CRTC。初次探针因 GETRESOURCES 的连接器/encoder 数量非零却无数组而在 PAN 前退出，修正后的完整结果单独保存，未改写失败记录。

完成依据是 legacy DRM 的旧页复用契约、目标驱动中“受保护的地址更新与事件入队→安全扫描窗口内提交→发送完成事件”的调用关系，以及上述实机逐笔验证。未获得显示控制器手册或独立扫描输出采样；没有把反汇编中的寄存器 load 位、CPU fence 或 MCP 截图当作所有物理帧无撕裂的证明，也没有宣称验证其他固件。现有 fbdev 映射及其缓存属性由该内核提供，不调用 G2D 或厂商 SDK。

### 初始化顺序回归

板上 `pan-state` 探针把前台切到第二页并确认事件后退出。初版捕获 DRM yoffset=1280，随后实际 fbdev 和 CRTC 均变为 0，初始化检查失败且未写 UI 像素。初步检查怀疑 framebuffer 打开顺序；后续 ioctl 故障注入明确了根因：完成闭包只捕获可复制的 console 状态字段，控制台资源本身提前析构。日志出现 `KD_GRAPHICS → KD_TEXT → FLIP_EIO`，文字模式恢复导致偏移重置。

最终显式持有整个控制台对象，初始化与提交共用恢复许可状态；在 framebuffer 打开后核验当前页并等待初始匹配事件。对照注入把首次 flip 在入队前返回 EIO：修复后只有 `KD_GRAPHICS → FLIP_EIO`，退出 1，控制台保持 graphics；没有 UI 像素写入。该探针模拟 ioctl 失败，不能称为真实 DMA 卡死或硬件丢失事件。

恢复工具先取得会话锁、fbdev 锁和 DRM master，核对固件/布局，再等待一笔真实匹配完成事件后恢复 KD_TEXT。应用运行时工具被独占锁拒绝（退出 3）；应用退出及故障注入后恢复成功。工具不会写应用像素。正常退出仍通过 Slint `quit_event_loop()` 等待在途提交，随后恢复原控制台模式。

### 输入、旋转与验证口径

`gt9xxnew_ts` 位于 `/dev/input/event1`，支持无 slot Type-A MT，X 范围 0–800、Y 范围 0–1280、tracking ID 0–255；没有 legacy ABS_X/Y。使用能力位选择协议，不能依赖旧驱动对不支持轴返回的零范围。库将物理坐标归一化并逆旋转；用户确认顺时针 90°，逻辑界面为 1280×800。

主机回归覆盖首指选择、坐标与 ID 顺序、双指顺序变化、主指抬起后不转移点击、部分包和 SYN_DROPPED。MCP 交互绕过 evdev；真实输入与实际页验证另列，不将软件截图替代这些检查。

本地原始档案位于 `evidence/x9-framebuffer/2026-10-14/`，不随 Git 分发。包括设备身份、布局、内核已知代码区和反汇编、首次失败与修正探针、构建、主机回归、设备启动及 MCP 日志。运行前原始双页备份 SHA256 为 `a7237a7ea0e9e7bb318d1d7c46840b8d40a94b3c2b8ff1a129eede02039a88c7`；重核需另行取得同批档案。

## Slint 旋转路径修复

本次在同源 Slint 中记录两项独立本地提交，当前依赖 HEAD 为 `ea7d0aec488caf3acaf61a2ab275a23cae201088`：

- `d753c7b`：原生截图按 1280×800 分配缓冲，却仍套用 Rotate90 的绘制 stride，首次 MCP 截图使程序断言退出。主机 0.68 秒稳定复现；截图临时使用窗口逻辑方向并恢复原旋转及 repaint 模式后，四方向截图和后续面板绘制一致。
- `ea7d0ae`：旋转绘制先交换目标宽高，再将其作为 SVG 栅格化尺寸，导致二次缩放。设备 125% SVG 的 64,000 像素区域与逻辑截图差异 3,558 点；主机相同用例精确复现同样计数。改用旋转前目标尺寸，四方向 PNG/SVG 区域均逐像素一致。

回归在 [rotation_snapshot.rs](../test-app/tests/rotation_snapshot.rs)。这些修改没有改变公开配置、PNG 原图或 SVG 内容；NX5 的非旋转路径同时经过主机 feature 矩阵及 ARM 交叉构建，本轮没有重新部署 NX5。

## 最终设备与验证结果

接入验收时的常驻路径 `/tmp/slint-bridge-x9-20261014/showcase-06`，SHA256 `568791f1c3e64c23154ae537af46f882023c346a58be6e15205d24732f1cb289`，13,697,892 字节；观测 PID 2029。命令为 `--touch=/dev/input/event1 --rotation=90`，无退出定时器、trace 或故障注入环境。进程持有 fbdev 双描述符、DRM 同一会话的两个描述符、触摸设备和 graphics 控制台；maps 中没有 VideoDec、EGL、GLES、GBM 或注入库。完整环境见 [设备快照](../test-app/devices/x9/current-target.json)，PID 不可直接复用。

| 验证 | 结果与范围 |
| --- | --- |
| 主机 feature 矩阵 | fbdev / nx5 / nx5-tde workspace 测试通过；缺失或冲突 feature 正确拒绝；fbdev 普通依赖无桌面/GPU 后端 |
| 格式、Clippy 与文档 | `cargo fmt --all --check`、两套板级 MCP 的严格 Clippy `--all-targets -D warnings`、`cargo doc --workspace --features nx5-tde --no-deps` 通过 |
| 两套 ARM 构建 | X9 hard-float `fbdev,slint/mcp` 与 NX5 `nx5-tde,slint/mcp` release 交叉构建通过；只部署 X9 |
| 双页完成探针 | 120 次交替及一次恢复逐笔匹配完成事件；不写像素，不分配第三页 |
| 初始化错误与恢复 | 初始 flip EIO 注入退出 1 且不恢复文字模式；活动所有者期间恢复工具退出 3；停止及故障后真实事件同步恢复成功 |
| 正常退出 | `showcase-05` 的同一最终后端运行 20 秒后退出 0，KD mode=0；常驻 05/06 运行期间 KD mode=1。06 仅新增 SVG 修复，当前保留运行，未为测试再次停止 |
| 原生 MCP | `showcase-05` 上八页完整交互、19 张场景截图、18 个设备卡片双向全程滚动与边界/误触检查通过，另存结束状态截图；06 上定向复核原生截图、evdev、Controls 与图片缩放，后端及交互代码相同 |
| Type-A evdev | 原始 `(421,87)` 转成逻辑 `(86.932,378.526)`，导航到 Interaction lab，按下/释放/退出三条事件均 Accepted，无丢失/未知事件；06 另验证 Controls 点触。属于 evdev 注入，不冒充实体手指操作 |
| 最终物理页 | Controls 前台 yoffset=0、PNG/SVG 125% 前台 yoffset=1280，分别读取 1,024,000 像素；与同旋转、同字体的独立 CPU 参考逐字节一致，两帧均 **0 个差异像素** |

原生 MCP 的缩放检查现在按实际视口逐轴判断裁剪；横向列表按 18 张卡片实际间距计算精确滚动范围，避免把 1024×600 的“四屏以上”硬套到 1280×800。原有 1024×600 主机 MCP 八页 / 19 图通过；其本轮记录另按新精确范围断言重放为 3610 px，重放没有被记为一次新的设备操作。

物理页对照使用 [x9-reference](../test-app/examples/x9-reference.rs) 的纯软件完整绘制，字体隔离步骤见 [工具说明](../test-app/tools/x9/README.md#独立-cpu-参考)。初次主机参考含真实 Bold 字体，与设备仅有常规字体、合成粗体的环境不同，Controls 差 7,450 个文字像素；匹配字体后整帧一致。直接与未旋转 MCP 截图比较还会出现圆角/曲线边缘的方向性栅格差异，不能把该对照误判为物理写页失败，或声称未经同条件比对的“整帧零差异”。原始失败对照与修正后参考均保留。

设备没有本轮可用的 WBC/独立扫描输出采样，因此两帧零差异证明真实页内容、通道、alpha 和旋转匹配软件参考，不证明所有时刻的 LCD 扫描无撕裂，也不扩展到其他内核或面板。实体手指操作仍需单独确认。日常截图及临时产物占用经过内存检查；原始大像素文件归档并核对 SHA256 后移除板端副本，保留当前程序、上一版恢复程序、字体、初始双页备份及恢复工具。
