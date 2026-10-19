# F1 显示驱动与翻页完成调查

调查日期：2026-10-19。目标设备：`fd_f1_r88r30_spinor-20c31b4578`。
本文按只读调查、实现准备、离屏验证分阶段保留证据口径。原应用始终保持运行，
尚未进行显示接管、翻页提交、物理页复用或扫描输出验证。

## 当前结论

后续已完成 [离屏 G2D 与缓存同步正向实测](#f1-offscreen-ge)，并通过
[当前内核镜像分析](f1-kernel-analysis.md) 直接核验普通 PAN 立即返回、
共享 VBLANK 等待和未使用 CFG_DONE 的完成路径。显示准入仍关闭。
下文最初调查及实现准备阶段的“尚未验证”仅代表该阶段，不覆盖后续结果。

设备参数已足够用于规划 F1 后端：1024×600、两页容量、BGRA8888 字节布局、
单点绝对坐标输入。**尚不具备准入所需的翻页完成证据**：官方参考驱动的 PAN
只更新配置，VSYNC 等待没有提交归属；当前固件也还未与源码对应。
不能把旧 F1 工程的 `PAN → WAITFORVSYNC` 直接搬入本库作为完成契约。

配置时序计算为 52.7278 Hz；DE 中断采样约 53.7 次/秒。二者口径不同，
实际屏幕刷新率尚未最终确认。以下区分本机观测、参考源码和待验证项。

## 本机只读观测

### 采集范围与原应用状态

本次通过显式 ADB 序列号读取 sysfs、procfs、设备树及内核只读分区，
并运行 [只读查询探针](../test-app/tools/f1/inspect-device.c)。探针仅调用
FBIOGET/EVIOCG 查询，只有 libc 依赖；未加载厂商 SDK、映射 framebuffer、
写显示寄存器、执行 PAN/WAITFORVSYNC、抓取或注入输入事件。
设备上唯一主动创建的内容是独立 `/tmp/slint-f1-inspect-WZiH8R/` 目录及
11,872 字节探针，运行后已删除。未停止、重启或替换原应用。

| 身份项 | 观测 |
| --- | --- |
| 系统 | ArtInChip Luban，RISC-V 64 位，Linux 5.10.44 #1，2026-05-22 构建 |
| boot ID | `dac34b8d-d45f-4fb9-a5fc-ba8902eda615` |
| 显示所有者 | `/system/app0`，PID 166，`/proc/PID/stat` starttime 1199 |
| 配套服务 | `/system/control_service`，PID 165，starttime 1199 |
| app0 资源 | fd 3 `/dev/ge`，fd 4 `/dev/fb0`，fd 5 `/dev/input/event0` |

`before.txt`、`after.txt` 以及清理后的 `final.txt` 中，两进程的 PID/starttime
和 boot ID 一致；前后 fd 所有者及 DE 图层/时序配置一致。PID 仅为本次快照，
后续操作必须重新发现。当前内核没有可读的 `/proc/kallsyms` 或 `/proc/config.gz`；
未挂载 debugfs、修改驱动日志级别或启用追踪。

### 显示布局与像素格式

以下来自 `probe.txt` 中的实际 ioctl 返回，不能用设备树默认值替代。

| 参数 | 实测值 |
| --- | --- |
| framebuffer | `/dev/fb0`，`aicfb0`，packed pixels (`type=0`)、true color (`visual=2`) |
| 可见/虚拟尺寸 | 1024×600 / 1024×1200 |
| 位深与 stride | 32 bpp，4096 字节/行 |
| 像素位域 | R offset 16 / G 8 / B 0 / A 24，各 8 位，`msb_right=0` |
| 内存字节顺序 | 小端 B、G、R、A，即 BGRA8888；32 位像素值为 `0xAARRGGBB` |
| 物理内存 | 起始 `0x42440000`，长度 4,915,200 字节 |
| 每页容量/第二页起始 | 2,457,600 字节 / `0x42698000`（由 stride×600 计算） |
| PAN 步长 | xpanstep=0，ypanstep=1，ywrapstep=0 |
| 旋转与其他格式标记 | rotate=0，nonstd=0，grayscale=0 |
| 查询时页偏移 | xoffset=0、yoffset=600；其他 sysfs 采样也见 yoffset=0 |
| 标准时序字段 | pixclock、四个 margin、hsync_len、vsync_len 均为 0 |

DE debug 输出显示视频层关闭，UI rectangle 0 开启、1024×600、stride 4096，
UI format=0，alpha enabled、mode=0、global alpha=255；显示 dithering 开启，
RGB 深度各 6 位。这里只记录寄存器解码输出，**未测试真实透明合成量程、
预乘规则或面板物理颜色**，也不能把 dithering 深度当作 framebuffer 位深。

### 时序与刷新率

`timing_config/`、DE `debug/display` 和实时设备树给出相同配置：

| 项目 | 有效区 | 前肩 | 后肩 | 同步宽度 | 总计 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 水平（像素） | 1024 | 160 | 140 | 20 | 1344 |
| 垂直（行） | 600 | 20 | 12 | 3 | 635 |

请求像素时钟为 45,000,000 Hz，因此配置刷新率为
`45000000 / (1344 × 635) = 52.727784 Hz`，周期约 18.97 ms。
标准 FBIOGET 的 pixclock=0，不能用该字段做除法或默认为 60 Hz。

`after.txt` 的 `/proc/uptime` 为 843.51→853.55 秒，DE IRQ 为
45,203→45,742，即约 53.685 次/秒。两类文件顺序读取，存在采样边界误差；
这不是专用帧计数，也不是物理刷新率测量。参考源码的 sysfs 时钟值没有回读
实际 clk rate，DE IRQ 又可包含 underflow，详见下文。`final.txt` 的 dmesg
筛选未发现 underflow 日志，但不能由此证明每次 IRQ 都对应一帧。
本轮未获得实际像素时钟或扫描端测量，保留差异，不将任一值宣称为已验证实际刷新率。

### 触摸轴与方向

实际 `EVIOCGABS` 结果如下，轴 ID 0/1/24 分别是 X/Y/压力：

| 轴 | minimum | maximum | fuzz / flat / resolution |
| --- | ---: | ---: | --- |
| ABS_X | 0 | 1024 | 0 / 0 / 0 |
| ABS_Y | 0 | 600 | 0 / 0 / 0 |
| ABS_PRESSURE | 0 | 255 | 0 / 0 / 0 |

设备名 `gsl1686`，event0，ABS 位图 `0x1000003`，具有 BTN_TOUCH，未声明
ABS_MT 轴。上限是 1024/600，不是 1023/599；归一化及末端像素钳制应遵循
本库既有输入规则，不直接拿原始坐标作为像素索引。

实时设备树 `/soc/i2c@19222000/ctp_gsl1686` 还配置了
`ctp_revert_x_flag=0`、`ctp_revert_y_flag=1`、`ctp_exchange_x_y_flag=1`，
`ctp_abs_max_x/y=1024/600`、`ctp_convert_reslution=0`。这些是驱动配置，
不能据此在后端再次交换/翻转 evdev 坐标。本轮未做四角实触或事件录制，
物理触摸方向与显示方向的对应仍待验证。

### 复核与证据

- [查询工具说明](../test-app/tools/f1/README.md) 包含构建、运行和清理命令。
  使用 `/opt/F1` T-HEAD GCC 10.2.0 编译，`-Wall -Wextra -Werror` 通过，
  设备探针输出 `probe_exit=0`。本次不涉及 Rust 行为或构建配置，未运行 Rust 测试矩阵。
- 原始档案位于 `evidence/f1-display-20261019/`，被 Git 忽略；检出仓库后
  须另行取得该档案。`probe.txt` 是 ioctl 结果，`before.txt` / `after.txt` /
  `final.txt` 是状态记录，`checks.json` 是进程身份与时序计算核对结果。
- `device.dtb` 为设备 `/sys/firmware/fdt` 的二进制副本，SHA256
  `21a04db8982c885a77519900a5edcd8b64f7f9e30116c5404ab4b6840b4aad53`。
  `kernel.mtd` 来自 `/dev/mtd4ro`；两者使用 `adb pull` 只读获取。
  此设备的 `exec-out` 不可用，base64 工具也不存在，不使用旧 adbd 的文本 shell
  直接搬运二进制。失败尝试单独保留为输出，不作为有效设备树或内核证据。
- `unpack-kernel.py` 将 FIT 中的内核和内嵌 DTB 解包到 `unpacked/`；
  参考源码与厂商手册在 `source/`，哈希及来源见下一节。

## 官方源码调查

### 来源与固件对应边界

本节依据厂商官方 [ArtInChip Luban](https://github.com/artinchip/luban) 的固定提交 `13fa021285a307ca345a6b05e0da371b635f18eb`（提交说明 `v1.2.9`，2025-06-23），以及 [D21x 用户手册](https://aicdoc.artinchip.com/out/downloads/D21x_User_Manual.pdf) Version 2.0.4，修订日期 2026-07-17。不是以其他芯片的 DE 手册替代 D211。

官方源码内核 [Makefile:2–5](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/Makefile#L2) 也是 `5.10.44`，但这个版本号不能证明它与设备上的厂商固件是同一份驱动。

本轮从设备只读取得的 `evidence/f1-display-20261019/kernel.mtd` 是 FIT 容器，`/images/kernel` 为 LZMA 压缩的 RISC-V 内核，数据长度 2,597,708 字节，解压 Image 长度 5,848,932 字节。Image 中的版本字符串为：

```text
Linux version 5.10.44 (root@38ba493018ec)
(riscv64-unknown-linux-gnu-gcc (T-HEAD RISCV Tools V2.0.1 B20210512) 10.2.0,
GNU ld (GNU Binutils) 2.32) #1 Fri May 22 02:04:02 UTC 2026
```

与本机此前观测的固件构建时间一致。取得的是原始内核 Image，没有配套 `vmlinux`、`System.map`、构建提交和厂商补丁清单；本轮未完成函数反汇编对应。因此，下述代码结论明确限于官方参考版本，不能宣称已验证当前固件的实现。

| 对象 | SHA256 |
| --- | --- |
| 设备 `kernel.mtd` | `28573f4b908e29ef7cecc8b0eea7dec76f0fcd86138eb9034e3ee9d397d2820d` |
| FIT 内核 LZMA 数据 | `97359be9007fbd7363c0632e5ff285ce051b7959d21fb0e166fa892dd6f2aa36` |
| 解压内核 Image | `754e2d6f05664522b16c6d60f5f6dc48d3e2eee1bf788938cf994a6823c86573` |
| D21x 用户手册 PDF | `ac3b4eb899a0233325a919a340a8a44cbde7964e78a2fb8109da67c96419f213` |
| `disp/aic_fb.c` | `3cb273b393408acf5de40479780dae2756f64bb7a87b8adca1e578ee3133dae2` |
| `disp/aic_de.c` | `bd2c87caaaee545cb34866ae29db08b017723626ec6a4ef3504c1561b05ec881` |
| `disp/aic_fb_ioctl.c` | `116b38225edc3c0ad763e4545e5c161de1865f97261cd581a6e73fabc6892224` |
| `disp/hw/de_hw.c` | `d32406df96e18b950894da508db2c324b64cbc56e8ae655e9be76f95a9e549ba` |
| `disp/hw/de_hw.h` | `855b3e5e60166e268add3a23a0b767121352515696a4820cbeaa09b6ce62c072` |
| `include/uapi/video/artinchip_fb.h` | `6c59e0925bab8cd30ca1d48dc31ac68de3281d3fec94a05138342b13c20fdb98` |

表中 `disp/` 位于 `source/linux-5.10/drivers/video/artinchip/`。设备原始证据在被忽略的 `evidence/` 下，复核须先取得本轮原始档案；官方源文件及手册已归档于 `evidence/f1-display-20261019/source/`。可运行 `python3 evidence/f1-display-20261019/unpack-kernel.py` 重建 `unpacked/` 下的 FIT 数据及内核 Image。官方源文件也可按固定提交重新获取。PDF URL 会随厂商更新，应核对版本和哈希。

### 打开与关闭 framebuffer

参考版本的 [`aic_fb_open()` / `aic_fb_release()`，aic_fb.c:207–217](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_fb.c#L207) 中，open 直接成功，release 则调用 `de->release_dmabuf()`。因此不能仅凭 `O_RDONLY` 就断言关闭设备没有驱动副作用。

继续追踪 [`aic_de_release_dmabuf()`，aic_de.c:1615–1641](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_de.c#L1615)：只移除 `pos->pid == current->tgid` 的 DMA-BUF 映射。独立查询进程没有导入 DMA-BUF 时，参考版本不会清理原应用的映射；这不等于已证明当前厂商固件没有额外改动。

### 从 PAN 到寄存器的路径

1. [`aic_fb_pan_display()`，aic_fb.c:292–338](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_fb.c#L292) 根据 x/y offset 计算物理地址，取得 UI layer 的缓存配置，设置地址后调用 `update_layer_config()`。普通物理地址分支随后直接返回 `0`，没有等待 latch、VBLANK 或 DMA 读取完成，并且没有传递 `update_layer_config()` 的错误。只有 `var.reserved[0]` 非零的 DMA-BUF 分支直接返回更新结果。
2. offset 的行步长使用 `ALIGN_8B(bits_per_pixel / 8 * var->xres)`，不是 `fix.line_length`。新增后端需验证两者一致，不能仅靠虚拟高度假设任意 stride 都可 PAN；当前 1024×32bpp 的 4096 字节 stride 恰好一致。旋转/HSBC 分支还会引入 GE 和额外缓冲，不能与普通双页路径混同。
3. [`aic_de_update_layer_config()`，aic_de.c:1323–1334](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_de.c#L1323) 执行 `DE_CONFIG_UPDATE=0`、更新 layer、`DE_CONFIG_UPDATE=1`，然后返回。底层 [`de_config_update_enable()`，de_hw.c:148–151](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/hw/de_hw.c#L148) 只是写寄存器，没有等待该位清零。
4. [`aic_de_get_layer_config()`，aic_de.c:455–477](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_de.c#L455) 复制 `comp->layers[]` 软件缓存；UI 配置时即更新缓存，见 [aic_de.c:1287–1301](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_de.c#L1287)。它不是正在扫描的 shadow 地址查询。读取 layer 或 fb yoffset 不能作为翻页完成确认。

### VSYNC 接口实际等待什么

[`AICFB_WAIT_FOR_VSYNC`，artinchip_fb.h:184–186](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/include/uapi/video/artinchip_fb.h#L184) 定义为 `_IOW('F', 0x20, unsigned int)`，在本目标 ABI 上与 `FBIO_WAITFORVSYNC` 一样为 `0x40044620`。[`aicfb_ioctl_wait_for_vsync()`，aic_fb_ioctl.c:15–18](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_fb_ioctl.c#L15) 忽略用户参数内容，直接调用 DE 等待函数。

[`aic_de_wait_for_vsync()`，aic_de.c:1488–1509](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_de.c#L1488) 的流程是：加自旋锁清空共享 `vsync_flag`，再等待其非零，超时 100 ms；信号中断返回负值，超时返回 `-ETIMEDOUT`，被唤醒后返回 `0`。

[`aic_de_handler()`，aic_de.c:101–125](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_de.c#L101) 读取并清除 timing IRQ 状态，只有 `V_BLANK_FLAG` 会将该共享标志置 1 并唤醒等待者。启动时使能 VBLANK 和 underflow，见 [aic_de.c:411–412](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_de.c#L411)。没有提交 ID、目标页地址、每次更新的完成序号，也没有检查 `CONFIG_UPDATE` 或 `CFG_DONE`。

由此能确认的仅是“等待期间处理过一次 VBLANK 中断”。由代码推论，多个等待者还会共用并重新清空同一布尔状态。未建立目标更新与这一中断之间的因果关联前，不能把 `PAN` 成功后一次 `WAITFORVSYNC` 成功解释为旧页可写。驱动缓存地址读回、固定延时或额外等待几次 VBLANK 也不补足这个证据缺口。本轮没有在原应用运行时调用这些同步或提交接口。

### 硬件提供的线索及尚未建立的完成契约

D21x 用户手册给出以下硬件描述（章号和页码按手册印刷页码）：

| 手册位置 | 说明 | 本次可得结论 |
| --- | --- | --- |
| 10.1.3.1，p.311 | Preload 为软件可访问配置，Shadow 为硬件使用且软件不可见的配置；多数 DE 寄存器带 Shadow | 普通 UI 地址寄存器读回不证明当前扫描地址 |
| 10.1.5.3，p.321，`DE_CONFIG_UPDATE` +0x008 | 写 1 允许 Preload 更新到 Shadow；硬件完成更新后自动清零，软件也可主动清零 | 自清零可作为后续研究的 latch 证据，但仅读到 0 无法辨别是谁清零、对应哪次提交 |
| 10.1.5.63–64，p.347–348，`TIMING_INT` +0x1D4 / 状态 +0x1D8 | bit 3 是 `CFG_DONE_INT` / `CFG_DONE_STATUS`；bit 0 是 VBLANK | 配置完成与 VBLANK 是不同状态，参考 Linux 驱动没有暴露带提交归属的 CFG_DONE 等待 |
| 10.1.5.65，p.348，`TIMING_LINE_SET` +0x1DC | 包含预取行数 | 还需要确认 latch 与旧页最后一次总线读取之间的关系，不能把输出行位置直接当作 DMA 退休证明 |

上述内容来源为 [D21x 用户手册](https://aicdoc.artinchip.com/out/downloads/D21x_User_Manual.pdf)。目前未得到“本固件下，特定提交的 CFG_DONE / CONFIG_UPDATE 自清零发生后，DE 不再读取旧页”的板级契约，也未验证 IRQ 状态清除顺序、提交并发与超时后的状态恢复。手册这些寄存器描述本身不足以证明旧页退休。

后续合适的方向是核对当前固件对应源码，确认硬件版本与手册适用性，再评估是否需由驱动将提交序号、配置生效和旧页退休绑定为一个可等待结果。当前不能直接宣布必须修改内核，也不能宣布现有接口已经满足准入。

### 时钟字段与 IRQ 统计的解释范围

参考版本 [`timing_config_attr()`，aic_fb.c:991–1001](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_fb.c#L991) 的读取函数返回 `panel->vm` 字段；[`display_show()`，aic_de.c:1692–1769](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_de.c#L1692) 输出 `comp->vm.pixelclock`。[`aic_de_pixclk_enable()`，aic_de.c:1663–1676](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_de.c#L1663) 将该值交给 `clk_set_rate()`，未以 `clk_get_rate()` 回读实际时钟。因此这两个 sysfs 字段表达配置请求值，不能证明实际 PLL/分频精确得到该频率。

同时，DE 驱动在一个 IRQ 上处理 VBLANK 和 underflow；`/proc/interrupts` 的 DE 次数并非专用 VBLANK 计数。用它除以时间只能得到 DE IRQ 频率近似值，underflow、中断合并、采样边界和调度都可能影响推导。当前硬件输出刷新率要结合实际时钟或专用帧事件/物理输出测量才能最终确认；请求时序推导值与 DE IRQ 频率不一致时，应保留两者的口径，不任选一个写作已验证刷新率。

### 格式与历史 F1 工程的参考价值

参考 [`aicfb_format_lists[]`，aic_fb.c:77–98](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/disp/aic_fb.c#L77)：`a8r8g8b8` 对应 ARGB8888，字段为 R16:8 / G8:8 / B0:8 / A24:8。在小端内存中是 B、G、R、A 字节顺序。它与厂商枚举名称 `MPP_FMT_BGRA_8888` 不是同一概念；后者对应位字段 B24 / G16 / R8 / A0。因此后端必须核对实际 fb bitfields 和设备树格式，不能只比较“BGRA”字符串。

本机旧工程 `/home/dpower/data/OMNI_STACK_GEN/os-saas/F1-Slint-App/src/framebuffer/mod.rs:188–221` 是 `PAN → WAITFORVSYNC → 交换软件 buffer 索引`。它可以帮助定位历史 ABI 用法，但代码注释中的“确保切换完成”不是驱动契约，更不是本次设备验证；本轮未修改或运行该工程。

### 新增后端前仍须满足的条件

- 将当前固件与驱动源版本、构建配置及厂商改动对应；核对当前 open/release、PAN、VBLANK 和 CFG_DONE 的实现。
- 在明确接管与恢复流程后，验证至多一笔在途提交下的目标页生效、旧页不再被读取，以及 timeout / underflow 后停止写两页的处理；当前只读阶段不做这些测试。
- 针对实际 stride、通道字段、旋转/HSBC、cache/coherent 映射建立准入条件；GE 完成与显示扫描完成应分别处理。
- 保留通用运行时的双页保护，把 ArtInChip 专有 ioctl、更新确认和设备核验放在 F1 板级后端。参考源码中缺失的完成确认不能通过单缓冲或等待固定时间规避。

## 2026-10-19：实现准备阶段

本阶段参考用户指定的 `/home/dpower/data/GitLab/slint-f1` 工作树，HEAD 记录为
`3f48f3bd2c54e936ae99e54da5f9983693963751`。用户确认没有当前固件对应源码。
新增 `f1` / `f1-g2d` 构建及板级边界；**显示准入仍关闭，不代表可上屏运行**。
当前实现与命令集中见 [平台说明](software-platform.md#构建依赖) 和
[F1 集成状态](../test-app/devices/f1/README.md)，不改变前文只读调查的验证口径。

参考工程 `src/platform/mod.rs:123–182` 用 G2D 把前台页复制到后台页，
随后在后台页进行软件渲染。本库保留画布及相邻帧脏区修复已经承担历史像素
维护，本次没有复制这一整页同步，也没有让 GE 访问两张物理显示页。
参考 `src/framebuffer/mod.rs` 的 `fb_fix_screeninfo` 物理地址字段为 `c_uint`，
本库继续使用 Linux LP64 所要求的 `c_ulong`，不照搬该结构。

扩展后的只读探针查询 `IOC_GE_VERSION` / `IOC_GE_MODE`：设备返回
`version=0x100, mode=0`，即 normal mode。`evidence/f1-implementation-20261019/`
保存本阶段 `before.txt`、`probe.txt` 和 `after.txt`；boot ID 与两原应用的
PID/starttime 保持一致，app0 继续持有 GE/fb/evdev，临时探针目录已移除。
这些证据只证明参数查询和进程状态，不证明 GE 作业正确。

参考工程的 `libmpp_ge.so` 区分 normal/cmdq 运行模式。进一步检查同一官方固定
提交 `13fa021285a307ca345a6b05e0da371b635f18eb` 的 GE 内核实现：

- [`ge_normal.c:295–318`](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/ge/ge_normal.c#L295)
  在绘制作业内等待硬件中断，检查完成位；超时重置并返回 `-ETIMEDOUT`，
  错误状态返回失败。此模式不支持单独 `IOC_GE_SYNC`。
- [`ge_cmdq.c:278–298`](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip/ge/ge_cmdq.c#L278)
  按 client 的 batch_num 等待。等待超时后循环重试，而 IRQ 的硬件错误分支
  重置后仍减少 batch_num，没有将该作业错误保存为 sync 返回值。
  因此不能把该模式下的 SDK sync 成功等同于结果有效。本次实现拒绝 cmdq 模式。
- 当前设备驱动与这份参考源码仍未完整对应；normal 模式查询本身不是完成契约验证。
  两个参考源文件归档于本阶段 evidence 目录，不以源码审查替代实机故障验证。

`f1-g2d` 直接使用 normal-mode UAPI，只实现独立 RGBA 画布上的大块不透明填充，
没有引入厂商 decoder/VO 会话。UAPI 依据为参考工程的三个原样头文件，保存在
`third-party/f1/include/`，摘要在 `third-party/SHA256SUMS`。
`check-ge-abi.c` 对照这些头文件校验结构尺寸、偏移、枚举及 ioctl；
DMA 缓存 flags=7/1 与参考 `libmpp_decoder.so` 的 `dmabuf_sync` 反汇编一致。
此核对只确认调用 ABI，不确认当前设备缓存行为。

主机测试模拟 GE 填充并比较 Slint 场景的完整像素，覆盖软件透明/圆角/渐变
回退及四种旋转；注入未知完成错误后验证不重试、CPU 回调使用隔离行、像素访问被拒绝。
查询工具、主机/交叉 ABI 编译检查、RISC-V 编译与主机回归属于本阶段验证范围。
**未运行真实 GE 填充、DMA 缓存/硬件错误注入、显示接管、PAN 或物理扫描验证。**

### SD 卡最小程序验证

用户指定测试固件放在 `/tmp/sdcard`，避免占用 RAM `/tmp`。本阶段重新确认
`/dev/mmcblk0` 以 exFAT 挂载到该路径，剩余约 7.2 GiB，部署前
`MemAvailable=22408 KiB`。`f1-g2d` 的 release `basic` 经 strip 后为
769,392 字节，放在当次独立目录：

```text
/tmp/sdcard/slint-bridge-f1-FZYRGj/basic
/tmp/sdcard/slint-bridge-f1-FZYRGj/basic.log
SHA256 cfeba80ae3f7db3b70bc33d999d2fea3bf732d6cae84ea12858af6af58a12b2c
```

上传后回读，字节与摘要一致。设备执行成功到达库入口，按预期报告
`DisplayNotAdmitted { backend: F1, ... }` 并以 1 退出；这验证 ELF、T-HEAD
加载器和拒绝准入路径，**不是显示或 G2D 绘图通过**。原应用 PID/starttime、
boot ID、资源归属保持一致。上述固件及日志留在 SD 卡供复核，未启动常驻进程。
原始记录为本阶段 evidence 下的 `sd-preflight.txt`、`basic-run.txt`、
`basic-readback` 和 `sd-postflight.txt`。

本阶段主机 feature 矩阵（含 F1/G2D 与板级互斥）、fmt、NX5 规定 clippy、
F1 主机及 RISC-V clippy、Rust API 文档构建通过。F1 `basic` / `showcase`
release 交叉构建及 NX5/X9 `basic` 交叉构建通过；后两者未部署到设备。
GE UAPI 在主机和 RISC-V 编译器下的结构/请求校验通过。

<a id="f1-offscreen-ge"></a>
## 2026-10-19：离屏 G2D 与缓存同步实测

本阶段在原应用持续运行时验证独立 DMA 画布，未打开 framebuffer、执行 PAN、
加载厂商 SDK 或改动显示寄存器。测试直接调用当前库的 `Native::new/fill`，
不允许软件回退。源码为
[`hardware.rs`](../src/backends/f1/g2d/native/hardware.rs)，构建及运行步骤见
[工具说明](../test-app/tools/f1/README.md#离屏-g2d-与缓存验证)。

部署前重新确认设备序列号、同一 boot ID、SD 挂载和原应用资源归属。
`MemAvailable` 为 22348 KiB。测试二进制 strip 后为 4,673,912 字节，
放在 `/tmp/sdcard/slint-f1-offscreen-rourxq/g2d-tests`，回读字节一致，SHA256：
`83ed4048c2552dc633f883b2ca55bcbf61cde14b8f0a776468358cbf61b1a6be`。
日志和退出码分别保存在同目录 `run.log`、`exit-code.txt`。

测试依次分配三个尺寸，每次仅保留一份画布。每个尺寸 16 轮，每轮 CPU
写入变化的预乘 RGBA 图案，GE 按轮次执行整幅、内部奇数边界、四角单像素、
交叉细条填充；原生路径使用 cache flush（flags=7）、normal-mode
`IOC_GE_FILLRECT`、cache invalidate（flags=1）。每轮读回全缓冲，逐像素
核对覆盖区的填充色，以及未覆盖区的原 CPU 图案和 alpha。

| 画布 | 单份 DMA 字节数 | 轮次 | 结果 | 校验和 | 用例阶段耗时 |
| --- | ---: | ---: | --- | --- | ---: |
| 64×32 | 8192 | 16 | 全像素一致 | `88e7c72a870d52e9` | 15 ms |
| 256×128 | 131072 | 16 | 全像素一致 | `086749d98c7a18a5` | 213 ms |
| 1024×600 | 2457600 | 16 | 全像素一致 | `ce8ed8f0fb08896d` | 4819 ms |

共核对 **10,387,456 个像素**，整个用例 5.16 秒，退出码 **0**。
最大采样 `VmHWM=5516 KiB`，包括测试进程及当前映射；不是 UI 应用内存预算。
表中耗时还包括 CPU 图案生成、全量读取、比较和校验和，不能作为 GE 性能或帧率。
此结果证明本固件上所测 normal-mode 不透明填充、格式/裁剪与这套同步调用
序列的正向数据一致性，不能单独证明某个缓存 ioctl 不可省略，也不覆盖透明混合。

结束后，app0（PID 166）与 control_service（PID 165）的 starttime 仍均为
1199，boot ID、原有 GE/fb/evdev 描述符归属和 DE 图层/时序配置保持一致。
MPP 池报告的 used size 前后均为 0；MemAvailable 为 22364 KiB。
没有发现 GE timeout/error、underflow 或 OOM 日志，测试进程已正常退出。
这些是本轮快照，不将全局内存数值或池计数解释为所有分配路径的无泄漏证明。

原始档案在被忽略的 `evidence/f1-offscreen-20261019/`，包含 `before.txt`、
`after.txt`、`run.log`、`exit-code.txt`、`checks.json`、测试 ELF 及编译记录；
检出仓库后需另行取得。固件及日志留在 SD 卡。首次后台 shell 启动没有产生
日志或留下进程，经核对后改为保持连接的前台执行；只有上述前台执行算作实测。

本轮未进行真实硬件超时/复位故障注入、显示接管、物理页复用或扫描输出测试。
若以后的离屏作业出现未知完成，ignored 用例会输出 `F1_GE_QUARANTINED`
并保持进程/资源，防止进程退出关闭已经隔离的 FD；本次未触发该分支。
**离屏 G2D 通过不开放显示准入。**
