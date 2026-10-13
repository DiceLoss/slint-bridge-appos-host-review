# NX5 最近环境记录

2026-10-13 清理版部署收尾：当前为 `c087071` + Slint `22804ff`，路径 `/tmp/slint-bridge-clean-c087071/showcase`，观测 PID **12364**。无退出定时器，trace、QuickFill 诊断、WBC 和观察器关闭；旧版 `/tmp/slint-bridge-neon-final/showcase`（`4a2eda2`）及启动脚本保留回退。两个 strip 二进制逐字节一致。

六页/十张原生 MCP 截图、227 条自动交互事件和正常退出检查通过；实体输入录到 8 次接触，16 个边沿均与 Slint 配对，265 条事件无丢失。用户确认显示和触摸正常，Controls 滑块最终为 48%。17 个诊断/更早回退文件已归档校验后清理。详见 [本次部署报告](nx5-cleanup-deploy.md)；本轮没有重测 FPS、WBC 或物理页像素。

设备仍为 USB1 / ADB `192.168.1.151:5555`、Linux #233，启动 ID `938c34d4-456a-4b1a-b532-678c0f92efd0`。操作前仍需重新核对实际身份、显示所有者和恢复文件。下方部署状态保留各阶段原始口径，不能用于当前进程操作。

## 2026-10-13 NEON 优化记录（历史）

2026-10-13 NEON 反预乘优化收尾：当前为 `4a2eda2` + Slint `22804ff`，路径 `/tmp/slint-bridge-neon-final/showcase`，观测 PID **6818**。无退出定时器，trace、WBC 和观察器关闭；恢复为 `/tmp/slint-bridge-shared-final/showcase`（`d34f436`）。

真实探针的半透明整屏写页由 52.75 ms 降至 19.34 ms，alpha 渐变由 51.61 ms 降至 22.89 ms；生产 Rust 写页独立测得半透明 19.13 ms。两组最后页读回合计 8601600 像素完全一致，另有 100 帧真实 TDE＋内存双页对照通过。六页/十张 MCP 截图和短列表 29.00 FPS 通过；相邻无 trace CPU 对照为新版 76.35%、旧版 77.14%。首次带 trace 窗口的较高 CPU 值保留，不用于归因。有限进程正常退出并释放资源。完整范围见 [NEON 报告](nx5-neon-unpremultiply.md) 和 [设备快照](../test-app/devices/nx5/current-target.json)。

## 2026-10-13 TDE 写页调查收尾（历史）

2026-10-13 TDE 写页调查收尾：常驻仍为 `d34f436` + Slint `22804ff`，路径 `/tmp/slint-bridge-shared-final/showcase`，重新启动后观测 PID **20127**。本轮只新增诊断工具与报告，生产路径未改；无退出定时器，trace、WBC 和观察器关闭。恢复版本仍为 `fdb0f0e`。

所需单次 TDE 转换未通过；CPU 转换 MMZ 后 TDE 拷贝的 4915200 个实际页像素完全一致，但四种图案均比 CPU/NEON 直接写页更慢。不透明全屏为 11.39 ms 对 6.61 ms，半透明为 66.36 ms 对 52.75 ms。独占探针正常退出并释放 VO/MMZ，随后原生 MCP 确认常驻恢复。完整范围与原始证据见 [TDE 写页调查](nx5-tde-writeback.md)，资源快照见 [current-target.json](../test-app/devices/nx5/current-target.json)。本轮没有新增展示 FPS、WBC 或实体触摸测量。

## 2026-10-13 共享 MMZ 画布收尾（历史）

2026-10-13 共享 MMZ 画布验证收尾：当前程序为 `d34f436` + Slint `22804ff`，路径 `/tmp/slint-bridge-shared-final/showcase`，观测 PID **6670**。无退出定时器，trace、QuickFill 诊断开关、WBC 和观察器关闭。恢复程序为 `/tmp/slint-bridge-fill-fdb0f0e/showcase`。身份、PID 和显示所有者在操作前仍须重新核对。

CPU/TDE 已共用 SDK 的 cached MMZ 映射。真实物理页最终六组全幅共 3686400 像素完全一致；不透明全屏写页约 6.65 ms，旧式暂存对照约 14.69 ms。列表短窗口 900 次有效完成提交均为 29.00 FPS，CPU 约 76.8% 单核。六页 MCP 与十张截图通过，180 秒有限进程正常退出、MMZ 回到基线，无对应进程强制回收。

最终常驻 RSS 14848 KiB、VmHWM 15132 KiB、3 线程，MMZ 12100 KiB / 5 块；归档校验后清理 17 个不用文件，`/tmp` 使用 21096 KiB、剩余 24748 KiB，系统可用内存 45044 KiB。旧报告的 WBC、实体触摸及全场景长期性能成绩未重记为当前版本。完整范围见 [共享画布报告](nx5-shared-canvas.md)，快照见 [current-target.json](../test-app/devices/nx5/current-target.json)。

## 2026-10-13 填充与整体透明度收尾（历史）

2026-10-13 填充与整体透明度验证收尾：当前程序为 `fdb0f0e` + Slint `22804ff`，路径 `/tmp/slint-bridge-fill-fdb0f0e/showcase`，观测 PID **25632**。无退出定时器，trace、QuickFill 诊断开关、WBC 和观察器关闭；普通填充继续使用 CPU。整体透明度已接入符合条件的 TDE 图像混合。恢复程序为 `/tmp/slint-bridge-ring-0e8516d/showcase`。

22 组面积/形状/颜色对照均未发现本面板范围内的 TDE 填充优势；真实 SDK 调用链检查中，TDE 成功后同一操作的软件绘制行数为 0。六页 MCP、短 Motion 窗口（750 次有效完成提交，约 28.96 FPS）和正常退出资源检查通过。此次未重测物理显示页、WBC 或实体触摸时延；完整范围见 [填充/混合报告](nx5-fill-blend.md)，实际快照见 [current-target.json](../test-app/devices/nx5/current-target.json)。

## 2026-10-13 圆环修复收尾（历史）

2026-10-13 圆环用例修复收尾：该轮程序为 `0e8516d`，路径 `/tmp/slint-bridge-ring-0e8516d/showcase`，观测 PID **6967**，使用同一 Slint `22804ff`。Controls 圆环已绑定配置百分比，五档/两种配色的原生 MCP 与主机保留缓冲回归通过；切换前数值及开关已恢复。无退出定时器，trace、观察器和 WBC 关闭。回退为上一版 `/tmp/slint-bridge-image-opt-22804ff/showcase`。详情见 [圆环修复](nx5-controls-ring.md)，实际快照及恢复前提见 [current-target.json](../test-app/devices/nx5/current-target.json)。

本轮未重复全场景性能或物理页测量；下方 29 FPS 与实际页成绩继续保留优化版的原有范围。

## 2026-10-13 图片列表优化收尾（历史）

2026-10-13 图片列表优化收尾：该轮为桥接工程 `acab18a` + Slint `22804ff`，路径 `/tmp/slint-bridge-image-opt-22804ff/showcase`，观测 PID **31200**，无退出定时器；trace、观察器、WBC 和事件录制均关闭。回退保留原图片版 `41f87b2`。实际状态与资源见 [current-target.json](../test-app/devices/nx5/current-target.json)，恢复时必须重新核对 PID 和显示所有者。

最终列表 **29.00 FPS**，其余三场景 28.90–28.98 FPS；列表动态 RSS 17.0 MiB、CPU 中位数 94.6% 单核。真实页三组 1843200 像素完全匹配软件参考；测试发现并修复了 Slint 文字缓存淘汰后的标题漏刷。用户确认“滑动流畅，标题同步正常”。实测范围、临时文件空间不足的失败样本和清理过程见 [优化报告](nx5-image-optimization.md)。

## 2026-10-13 首次图片背景复测（历史）

2026-10-13 图片背景复测收尾：该轮为 `41f87b2`，路径 `/tmp/slint-bridge-image-41f87b2/showcase`，观测 PID **12090**，无退出定时器。trace、输入观察器、事件录制和 WBC 已关闭；纯色 `308afe5` 保留作回退。该轮详细数据见 [图片背景报告](nx5-image-background.md)。

实际 PNG 背景和半透明界面下，动画/透明混合仍为 29.00 FPS，列表约 20.37 FPS、CPU 接近单核 100%，动态 RSS 约 20.5 MiB。实体共同采样区间 16 次接触，处理时延中位数 2.755 ms、最高 55.580 ms；用户确认恢复后的常驻界面响应正常。首次等待实体操作时有限测试到期造成空窗，现已恢复常驻，详情及所有测量边界见 [图片背景报告](nx5-image-background.md)。

## 2026-10-13 纯色展示性能收尾（历史）

2026-10-13 纯色版本性能测试收尾：该轮运行 `308afe5` 展示二进制，当时观测 PID **3782**，日志 `/tmp/slint-bridge-showcase-308afe5/live-after-performance-2.log`；已恢复关闭 trace 的常驻模式，观察器与事件录制均结束，`f963558` 回退保留。该轮详细数据见 [纯色性能报告](nx5-showcase-performance.md)。

动画、列表、两种透明场景共 6150 次有效完成提交，日志精度下均为 29.00 FPS；动态 RSS 约 13.9 MiB。完整实体短窗口 240 条事件全部 Accepted、零丢失；14 条 evdev 边沿配对的中位时延 1.756 ms，最高 59.734 ms 出现在 Motion → Devices 切页抬起，含量化裕量为 60.734 ms。范围、原始记录覆盖与恢复时的一次安全拒绝见 [性能报告](nx5-showcase-performance.md)。以下保留此前采集口径，不能沿用旧 PID。

## 2026-10-13 首次展示部署（历史）

2026-10-13 09:41（北京时间）收尾：该轮常驻为六页展示应用 `308afe5`，路径 `/tmp/slint-bridge-showcase-308afe5/showcase`，观测 PID **30541**。原 `f963558` 保留为回退；随后用户确认“已操作，显示和触摸都正常”。完整结果及回退方法见 [展示应用部署报告](nx5-showcase.md)，机器可读快照见 [current-target.json](../test-app/devices/nx5/current-target.json)。

本轮设备原生 MCP 六页交互与十种界面状态检查通过；20 秒有限运行正常退出，VO 关闭、MMZ 恢复基线。收尾 RSS 15496 KiB、VmHWM 20284 KiB、3 个线程、MMZ 12100 KiB / 5 块，可用内存 49020 KiB。WBC、持续 trace、故障注入及事件录制关闭。实体确认晚于零事件录制窗口，没有新增触摸时延、WBC 或显示 FPS 测量。

设备仍为 USB1 / ADB `192.168.1.151:5555`、Linux #233，同一启动 ID `938c34d4-456a-4b1a-b532-678c0f92efd0`；未改固件、SDK、驱动、网络和开机服务。操作前仍需重新核对身份、PID 和显示所有权。此前部署与测量保留下方原始时间口径。

## 2026-10-12 透明 UI 部署记录（历史）

该轮设备记录时间为 **2026-10-12 18:32:20（北京时间）**，历史依据见 [透明 UI 实测](nx5-alpha.md)。当前机器可读快照已更新为上方的新部署状态，不能将下方旧 PID 用于操作。

该轮常驻 `f963558`，路径 `/tmp/slint-bridge-alpha-f963558/display`，观测 PID **22475**。用户最初要求部署 `287da8c`；实测发现半透明合成偏亮后，用户授权修复并部署新提交。新版本为 NX5 写屏 alpha 使用实测的 0–128 量程，Slint 画布和 TDE 运算仍保持标准 0–255。完整条件、失败与修复证据见 [透明 UI 实测](nx5-alpha.md)。

软件和 TDE feature 共检查 10 个实际前台帧、6144000 个像素，错误为 0；50 个 WBC 样本的半透明 RGB/参考色最大 Y/UV 差为 1。底层为现有黑色 VO 背景，未接入视频解码器或验证非黑色视频层。两条路径正常退出均关闭 VO/WBC，MMZ 回到 4900 KiB / 2 块，无对应测试进程的强制回收。

修复前后各运行 60 秒纹理场景：均为首个统计窗口 28.44 FPS、后续 10 个窗口 29.00 FPS，TDE 错误为 0。帧率统计成功完成的显示提交；常驻 trace 已关闭，不将 UI timer 帧号当作 FPS。部署二进制与修复版性能测试文件 SHA256 相同：`cc87c63d826ab01459a05005f5eea25888dbd1398990e4b38421128109490d21`。

基线 `287da8c` 的用户实体操作录到 12 组接触、49 条 Accepted 事件，零丢失；内核报告到 Slint 分派的保守上界约 11.3 ms，用户确认响应及时。修复后的有限录制窗口没有捕获新输入；随后用户完成所请求的 4 次点击，确认 `f963558` 实体操作响应正常，但未获得新增时延样本。最终版本另完成三轮 MCP 暂停/恢复，24 条事件 Accepted、零丢失。两种验证范围分开记录，MCP 不替代 evdev 实测。

最终界面 RUNNING，原生 MCP 截图为 1024×600，截图前可用内存 53684 KiB；[最终截图](../evidence/nx5-alpha-deploy/2026-10-12-287da8c/final-running.png) 已检查。持续 trace、WBC、输入观察器、事件录制均已关闭。资源快照为 RSS 11056 KiB、VmHWM 15796 KiB、3 个线程、MMZ 12100 KiB / 5 块、可用内存 53600 KiB。

任务开始时的 `2de9168` 保留为回退：`/tmp/slint-bridge-touch-2de9168/display`，SHA256 `425a1660192aa8886dea14d8d3d834a6a6a95a6035fe259fbdb4efab3d19af30`，原启动脚本和主机校验副本均保留。[保留策略](../test-app/devices/nx5/deploy-retain.json) 已同步；`bccf6b9` 和中间 `287da8c` 程序及不用的诊断产物已归档校验后清理。回退步骤见 [部署报告](nx5-alpha.md#最终部署与回退)。tmpfs 路径在重启后需重新部署，本轮未改开机服务。

设备仍为 USB1 root、ADB `192.168.1.151:5555`、Linux #233，启动 ID `938c34d4-456a-4b1a-b532-678c0f92efd0`。固件、SDK、驱动与网络未改；framebuffer 和触摸继续使用 `/tmp/slint-bridge-c03c3bb/dev/` 链接。旧常驻与中间版本被 SIGTERM 结束时的内核 MMZ 回收单独保存，不计为新版本正常退出结果。

原始档案位于本地 `evidence/nx5-alpha-deploy/2026-10-12-287da8c/`，不随 Git 分发，需另行取得。本轮没有重跑三个独占 ignored 用例、EIO、初始化重试或每一帧扫描的无撕裂认证。

## 历史验证范围

[此前触摸部署](nx5-touch-response.md) 对应 `2de9168` 的上一轮 10 组点触；[会话重试](nx5-review-fixes.md) 与 [截图内存复验](nx5-screenshot-memory.md) 对应 `bccf6b9`。[三项硬件回归](nx5-hardware.md) 对应 `a9fd3c8`，[启动回归](nx5-startup.md) 对应 `949ceb9`，[EIO](nx5-tde-exit.md) 对应 `177c18a`。这些原始成绩保留，不改记为本轮结果。

[并行化](nx5-parallel.md)、[调度](nx5-scheduling.md) 和 [单周期调查](nx5-single-cycle.md) 保留对应阶段的有限测量。本轮继续使用原阻塞双页完成契约，没有改成未经验证的单周期提交。

## 初次环境确认记录

2026 年 10 月 10 日初次确认时，串口从 `/dev/ttyUSB2` 重新枚举为 **DPower Terminal 的 `/dev/ttyUSB1`**，网络仍为 `192.168.1.151:5555`。通过 DPower Terminal 再次读取的启动 ID 与 ADB 均为 `f636e7cf-7beb-46f3-98aa-53c651f96572`，MAC 也一致，确认仍是同一次启动的同一台设备。

| 项目 | 初次确认结果 |
| --- | --- |
| 串口 | DPower Terminal，profile `serial:port-devttyUSB1`，root shell |
| 网络 | `192.168.1.151/24`，ADB 5555 已连接，ADB 用户 UID 2000 |
| 型号与 CPU | NX5，双核 Cortex A7，ARMv7、NEON |
| 系统 | Linux 4.9.138，构建号 233，2024 年 3 月 14 日构建 |
| 用户空间 | Buildroot 2016.11，glibc 2.21，ARM EABI5 soft float |
| 内存 | Linux 可见 91688 KiB，约 89.5 MiB；独立 MMZ 32 MiB |
| framebuffer | `fyfb`，标称 1024 × 600，32 bpp，stride 4096；最后读数虚拟尺寸 1024 × 1200，pan 0,0 |
| 输入 | `/dev/input/event0`，名称 `gsl1686` |
| VOU | v0.5.8，2021 年 6 月 1 日构建 |
| 厂商库 | `/system/app/clib/libVideoDec-1.6.6.so` |
| 内核模块文件 | `/system/ko/fyfb.ko`、`/system/ko/vou.ko` 均存在 |
| 初次检查时的 UI | 没有发现运行中的 UI 程序 |

初次连接检查通过时，显示测试的准备条件尚未满足。`/proc/umap/vo` 的 DEV CONFIG 为空，尚无启用的显示设备。查询 framebuffer 时，串口记录了 `gui layer region(0,0,1024,600) cannot exceed frame region(0,0)` 和窗口尺寸设置失败的告警。

首次 sysfs 读到虚拟尺寸 1024 × 2400；包含 `fbset -i` 的查询之后，两次读数均为 1024 × 1200。没有发送显式模式设置命令，但打开 framebuffer 可能触发厂商驱动初始化；本次没有进一步判定变化原因。不能把首次读数当作已验证的四页缓冲能力，后续初始化后应重新核对。

初次环境确认没有上传、启动或停止应用，也没有重启、运行翻页探针或修改网络配置。当时尚待核对 BSP 初始化流程与厂商库，该固件状态为“尚未测试”。随后完成的初始化与有限输出观测见前述报告；旧 #22 环境的观测始终不能作为 #233 的准入依据。

目标、驱动与库的 SHA256 见 [环境记录](../test-app/devices/nx5/current-target.json)。2026-10-10 的串口复核见 [serial-recheck.txt](../evidence/environments/2026-10-10-usbtty1/serial-recheck.txt)。初次检查时的串口和 ADB 记录保留于 [serial.txt](../evidence/environments/2026-10-10-usbtty2/serial.txt)、[adb.txt](../evidence/environments/2026-10-10-usbtty2/adb.txt)；这些原始证据属于不随 Git 分发的本地档案。
