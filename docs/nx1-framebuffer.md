# NX1 framebuffer 接入与验证（2026-10-20）

本轮从 `main` 建立独立 NX1 适配分支，未采用 F1 内存优化保留分支或
GE 直写原型。现有 Slint 来源、保留画布和双页调度继续使用。
当前接口见 [平台说明](software-platform.md#准入边界)，构建与 `/mnt`
部署见 [设备说明](../test-app/devices/nx1/README.md)。
下面的数据仅对应本次固件和测试，不代表所有 sun8i 设备。

## 设备事实

| 项目 | 实测值 |
| --- | --- |
| 串口 | 主机 `/dev/ttyUSB1` |
| 内核 | `3.4.39 #1 Fri Mar 25 14:53:50 CST 2022`，hostname `X5` |
| CPU | 单核 ARM Cortex-A7，ARMv7、NEON、VFPv4 |
| CPU Serial / MAC | `140079a287202c210290` / `02:92:d0:15:3d:73` |
| 内存 | `/proc/meminfo` 的 MemTotal=43532 KiB，约 42.51 MiB；没有 MemAvailable |
| libc / ELF | glibc 2.21；ARM EABI5，`/lib/ld-linux.so.3`，soft-float ABI |
| fb0 | 1024×600，虚拟 1024×1200，stride 4096，BGRA8888 |
| framebuffer | `0x43100000`，4915200 B，两页各 2457600 B |
| 显示 | `/dev/disp`，唯一启用层 ch2/layer0，pixel alpha，非预乘 |
| 触摸 | `/dev/input/event2`，`gt911_ts`，ABS_X/ABS_Y + BTN_TOUCH |
| 触摸轴 | X=0..1024，Y=0..600；另有 ABS_PRESSURE |
| 测试目录 | `/mnt/slint-nx1-20261020`；上层 `/mnt` 为主机 NFS 导出 |

`/proc/fb` 列出 fb0..fb7，但不意味着八个可用显示输出。
本次只准入已检查的 fb0，不重新分配 framebuffer 或改变其模式。
`/mnt` 的 NFS 源为 `192.168.1.65:/home/dpower/data/Demo/NXF`，
地址和挂载均在设备重启后重新核对。

## 实际内核的完成接口

用户没有对应 BSP 源码。本轮只读设备 `/dev/mtd1`，没有写入 flash，
从 Android boot 镜像中的 gzip 解出运行版本相符的 ARM 内核，
结合设备 `/proc/kallsyms` 反汇编显示路径。
[全志公开 dev_fb.c](https://github.com/allwinner-zh/linux-3.4-sunxi/blob/master/drivers/video/sunxi/disp2/disp/dev_fb.c)
与 [dev_composer.c](https://github.com/allwinner-zh/linux-3.4-sunxi/blob/master/drivers/video/sunxi/disp2/disp/dev_composer.c)
只用于辅助理解；关键结论以本次镜像指令和设备结果为依据。

| 原始文件 | SHA256 |
| --- | --- |
| boot 分区读取，3145728 B | `438da712a0a0641888a6fe8f1556392ce1261069d46c1b6c356d63dedcd9c17f` |
| 解压内核，5445284 B | `952b31d0000514f675a288f8d04001f5306f3c2a5de3dd6eeb494de0b9ead7b0` |

内核加载基址按 `0xc0008000` 解释；以下地址只适用于这些原始文件：

- `sunxi_fb_pan_display`：`0xc015130c`。
  设置图层后在 `0xc0151588` 调用内部等待，
  随后 `0xc015158c` 把成功返回值写成零，未传播等待失败。
- `fb_wait_for_vsync`：`0xc0151068`。
  等待共享显示计数，超时约 50 ms；信号中断也不能构成匹配提交的完成证明。
- `sunxi_fb_ioctl`：标准 `0x40044620` 不进入内部等待，
  落到 `0xc0151904` 的直接返回零路径。不能因为调用返回 0 就声称发生了等待。
- `hwc_commit_ioctl`：`0xc0155560`，输入 ABI 长度 5944 B；
  `dispc_gralloc_queue` 在 `0xc0156648` 固定选择 screen0 的 16 个图层。
- `disp_mgr_set_layer_config` 在获取显示锁后，检查数量是否超过硬件层数；
  本机为 3 通道 × 4 层。16 大于 12 的错误分支经 `0xc015e420`
  返回，跳过正常路径中的解锁。

同画面 HWC 探针只复用现有层内容，没有改写 framebuffer 像素。
它得到首个 fence，但后续等待未完成，内核输出 `NULL hdl!`，
工作线程栈停在 `disp_sys_lock → disp_mgr_set_layer_config → hwc_commit_work`。
程序停止提交并保留资源；经用户同意重启恢复，随后重新核对身份与挂载。
这项失败记录不计作显示验证通过，**不应重复运行此探针**。
正式 NX1 后端不使用 HWC 或其 fence。

在上述限制明确后，用户明确选择 NX1 参考 PAN 约定继续适配。
它允许在现有固件上运行双页应用，但不能补足旧页硬件退休、
错误等待传播或全扫描帧无撕裂的证明。
本轮未修改内核，也未把这个约定推广到 NX5/X9。

## 实现与检查

新增独立 `nx1` feature、`Backend::Nx1`、`src/backends/nx1/` 和工具链预设。
软件 renderer 绘制到通用 RGBA 保留画布，转换并写入后台页；
参考 PAN 在后台提交线程执行，UI 最多提前绘制一帧。
初始化及每次提交检查布局、实际设备和偏移，初始化检查显示所有者。
可见的 ioctl 错误和读回不符停止后续写页；固件吞掉的等待错误仍无法可靠检测。

本轮检查结果：

| 检查 | 结果与边界 |
| --- | --- |
| NX1 主机 workspace 回归 | 通过；含 53 个库内测试及应用公开接口、像素、输入、页面生命周期回归 |
| feature 矩阵 | 8 个有效配置通过；16 个缺省/冲突配置按预期拒绝 |
| clippy | NX5-TDE/MCP、NX1/MCP 主机全 targets，以及 NX1 ARM 全 targets 均通过 |
| 构建 | 用户指定 NX1 工具链构建 basic、display、showcase 成功 |
| API 文档 | `cargo doc --workspace --features nx5-tde --no-deps` 通过 |
| display 实机 | 12 秒限时运行正常退出；预热后约 58.8–59.8 次提交/秒 |
| showcase 原生 MCP | 九页交互、表单/键盘/弹窗、动画暂停、透明开口、缩放与日志检查通过 |
| Devices | 横向 18 张卡片全程、两端夹紧、筛选和稳定选择通过 |
| Rich list | 纵横两轴各 100 个 ID、首末端选择、拖动和边界通过 |
| 实际像素 | 24 张静态物理页采样，包含偏移 0 和 600；透明页 alpha 最小值为 0 |
| 缩放 | 物理页上的 PNG/SVG 50%、125%、200%、100% 尺寸、色块与裁剪检查通过 |
| evdev | event2 完整触摸帧能导航到 Controls、切换开关并恢复；完整释放事件进入 UI |
| 独占 | 已有展示进程时第二个显示进程返回 DisplayNotAdmitted |
| 初始化失败恢复 | 错误 fb1 和不存在的输入路径被拒绝，随后正常运行 3 秒并退出 |
| 正常退出 | 600 秒展示验证进程退出码 0，之后未见 framebuffer/disp 描述符所有者 |

完整 UI 验证后，仅将所有权检查从 fb0 扩展到共享控制器的 framebuffer 节点，
渲染与提交逻辑不变。最终版本再次通过 NX1 主机回归、ARM 构建和 clippy；
实机单独持有 fb1 时，新进程被拒绝，释放后成功启动。
完整 UI 对应 `binaries.sha256`，最终加强准入后的构建对应 `final-binaries.sha256`。
最终展示程序位于 `/mnt/slint-nx1-20261020/final/showcase`，以普通进程启动，
未添加开机服务。本次临时 ADB 调试进程和主机 MCP 转发已清理，
展示程序继续运行；其原生 MCP 仅监听设备 localhost。

原生 MCP 录制未丢失或出现未知事件。
evdev 注入经过内核输入节点和库的解码路径，但不验证物理触摸传感器。
最终程序的手指操作确认窗口未观察到输入，也未收到人工确认，
因此不把实际传感器点按计为通过。
本轮实机使用 rotation=0；其他旋转仅由通用主机回归覆盖。
物理采样在 UI 等待、提交线程空闲时分块读取，PNG 在主机编码；
它不是设备上的 MCP 软件重绘，也不能代替扫描输出采集。
控件、透明页面和富列表的采样图已人工查看。

简单 display 约 59 FPS，与包含图片/透明控件的 showcase 不是同一负载。
showcase 最初 Overview 窗口的提交日志约为 18–22 FPS；
完整交互过程中切页、MCP 查询和 NFS 采样混在一起，不把整段日志作为纯渲染基准。
在验证后段静态 Controls 的一次资源快照中，VmRSS=17136 KiB、
VmHWM=18020 KiB；这是带 MCP 并遍历过完整界面的进程数据，
不是最小 UI 开销。物理采样前记录的 MemFree 最低为 3180 KiB。

## 证据与复现范围

原始材料位于本地 `evidence/nx1/2026-10-20/`，保持 Git 忽略。
使用这些报告值需另行取得该目录；重新构建不能生成同一轮实机记录。

- `boot-readonly.bin`、`kernel.bin`、`kallsyms.txt` 和反汇编片段：
  固件调查；不能改用其他镜像后仍解释为相同地址。
- `fence-probe.log`、`pre-reboot-stacks.txt`：失败的 HWC 尝试与恢复依据。
- `host-tests.log`、`feature-matrix.log`、`cross-build.log`、`clippy-*.log`、
  `cargo-doc.log`：对应主机与交叉构建检查。
- `ui/summary.json`、`ui/events.json`、`ui/*.png`、`ui/rich-full/`：
  原生交互及物理采样；`evdev.json` 独立记录内核输入注入。
- `display-smoke.log`、`ui.log`、`ui.exit`、内存记录及 `binaries.sha256`：
  本轮固件运行和构建身份。

本轮不包含视频层接入、加速器、系统开机服务、flash 刷写，
也不包含掉电/休眠恢复、长时间压力或扫描输出无撕裂认证。

## 后续透明合成修复

2026-10-21 发现：上文物理页 Alpha 最小值为 0 的检查，只证明透明数据已写入 framebuffer，
没有证明 LCD 上的最终透明合成正确。透明背景下调低 Overlay opacity，会暴露非预乘 RGB 的亮色圈。
后续已定位到 DE2 最终输出预乘位被当前内核关闭，修复与新增验证单独记录在
[NX1 透明合成](nx1-transparency.md)，不改写上述历史像素及翻页结果。
