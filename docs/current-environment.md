# NX5 最近环境记录

2026-10-15 已完成 [纹理 NEON 优化后的 NX5 实机回归](nx5-texture-neon.md)：同源应用优化前后纵向 27.48 → 27.45 FPS、CPU 79.35% → 79.42%；横向 25.88 → 25.77 FPS、CPU 86.11% → 86.26%。六项硬件、两项 ARM 纹理、八页交互、evdev 和背景 23 阶段物理双页通过。本轮未发现明显退化，未获得 X9 那样的加速。已恢复原 `background-holes`，观测 PID **26898**，原 ELF 和启动脚本未变；两区域初始状态、Change 44、前景计数 0、弹窗关闭，完整截图与测试前相同。日志 `live-after-neon-regression.log`，无时限或探针；诊断文件已归档清理。操作前仍须重新核对身份及实际所有者。常驻 RSS 11380 KiB、MMZ 12760 KiB / 6 块，可用内存 44792 KiB。

## 2026-10-15 背景透明区域部署（历史）

2026-10-15 `BackgroundWithHoles` 已实现并通过 [图片背景透明区域验收](nx5-background-holes.md)。当前运行独立演示 `/tmp/slint-bridge-holes-20261015/background-holes`，观测 PID **412**，原 `2fddc04` showcase 保留恢复。两区域移动、缩放、隐藏、前景与弹窗通过；23 阶段物理双页共 27648000 像素精确匹配。常驻 RSS 12008 KiB、MMZ 12760 KiB / 6 块、可用内存 44636 KiB；无定时退出、trace 或探针，尚未接入视频源。设备身份与启动 ID 未变，操作前重新核对实际所有者。

## 2026-10-15 X9 共用改动兼容性回归（历史）

2026-10-15 X9 共用改动兼容性回归完成，已恢复原 NX5 `2fddc04` + Slint `9a1a3b1`，观测 PID **22711**，路径 `/tmp/slint-bridge-rich-render-2fddc04/showcase`，日志 `live-after-x9-regression.log`。无定时退出、trace 或探针；Controls 48%、富列表纵向起点。当前候选通过六项硬件、真实页及 evdev 验证，纵横完成 FPS 与旧版分别为 27.52/27.46、25.84/26.03；详细差异与测试边界见 [NX5 链路回归](nx5-x9-regression.md)。候选已归档移除，原当前/回退版本保留。常驻 RSS 18444 KiB、MMZ 12760 KiB / 6 块、可用内存 38796 KiB；设备启动 ID 未变，操作前仍须重新核对。

## 2026-10-14 富列表脏区与绘制优化（历史）

2026-10-14 富列表脏区与绘制优化收尾：当前 `2fddc04` + Slint `9a1a3b1`，路径 `/tmp/slint-bridge-rich-render-2fddc04/showcase`，观测 PID **6607**。保留 100 项及原有视觉内容；条目子树裁剪、SVG 栅格缓存及 TDE CPU 路径优化已部署。Rich list 纵向起点、Controls 48%，无退出定时器、trace 或临时探针。

独立窗口纵向 **27.52 FPS / 78.66% CPU / 17.91 MiB 平均 RSS**，横向 **26.03 FPS / 85.99% CPU / 18.27 MiB 平均 RSS**。基线 17.52 / 15.51 FPS。主机 25 状态整帧像素一致，最终硬件与 100 帧双页、完整列表交互、两方向完整物理页比对通过；详见[脏区与绘制优化报告](nx5-rich-render.md)。

MMZ **12760 KiB / 6 块**，新增小源槽占 620 KiB；另有最多 2 MiB / 32 项的 SVG 像素缓存。清理后常驻 RSS 18904 KiB、VmHWM 18904 KiB（本次常驻峰值），可用内存 38692 KiB。板上保留当前和 `1225593` 恢复程序，更早二进制经 SHA256 校验后归档到主机。启动 ID `4aa4f5d2-8de2-42cb-9c2b-44c3e74d6e2b`；操作前重新核对身份、实际所有者和退出资源。

## 2026-10-14 富列表源缓存优化收尾（历史）

2026-10-14 富列表源缓存优化收尾：当前为 `1225593` + Slint `22804ff`，路径 `/tmp/slint-bridge-rich-cache-1225593/showcase`，观测 PID **3102**，日志 `live.log`。两个源槽按使用帧和源像素量选择淘汰项，避免小卡片反复挤掉全屏背景；界面仍为原有 100 项。Rich list 恢复纵向起点，Controls 48%；无退出定时器、trace、采样库或临时计时。

无探针版本纵向 **17.52 FPS / 89.53% CPU / 18.03 MiB 平均 RSS**，横向 **15.51 FPS / 91.72% CPU / 18.28 MiB 平均 RSS**。相对上一版 12.58 / 11.55 FPS，分别提高 39.3% / 34.3%。诊断对照定位到每帧约 24 ms 的重复背景转换，新策略消除了该项稳态开销。主机回归、两项实机硬件测试、横纵 100 项交互及奇数位置物理页条带通过；范围与方法见[源缓存瓶颈报告](nx5-rich-bottleneck.md)。

MMZ 仍为 12140 KiB / 5 块，没有新增源槽或画布。常驻清理前 RSS 18824 KiB、VmHWM 23588 KiB（含截图峰值），可用内存 17364 KiB。直接回退为 `b1164e4`，更早的 `8d6bd47` 与 `abb2e33` 程序和原保留文件继续保存。启动 ID 为 `4aa4f5d2-8de2-42cb-9c2b-44c3e74d6e2b`；操作前重新核对身份、实际 PID 和显示所有者。

## 2026-10-14 奇数坐标混合优化收尾（历史）

2026-10-14 奇数坐标 TDE 混合优化收尾：当前为 `b1164e4` + Slint `22804ff`，路径 `/tmp/slint-bridge-odd-b1164e4/showcase`，观测 PID **14933**，日志 `live.log`。TDE 处理对齐后的内部区域，CPU 只在硬件完成后补最多一像素边缘，保持原始位置与裁剪。第八页 Rich list 恢复纵向起点，Controls 48%；无退出定时器、trace、录制或 WBC。

同一启动、同一富列表负载，纵向 **12.58 FPS / 92.74% CPU / 17.96 MiB 平均 RSS**，横向 **11.55 FPS / 93.83% CPU / 18.25 MiB 平均 RSS**。TDE 混合分别为每绘制帧 3.02 / 3.27 次，未将调用数或缓存命中率当成硬件忙碌率。256 组主机像素组合、两项实机硬件回归、横纵 100 项遍历及六张截图通过；奇数位置物理条带共 102400 像素最大通道差 1。详见[奇数混合优化报告](nx5-odd-blend.md)。

七个最终有限进程正常退出并回到 MMZ 4900 KiB / 2 块。常驻 RSS 18760 KiB、VmHWM 23524 KiB（含截图峰值）、MMZ 12140 KiB / 5 块，比旧版多 40 KiB，可用内存 27944 KiB。上一版 `8d6bd47` 作为直接回退，`abb2e33` 继续保留；启动 ID 为 `4aa4f5d2-8de2-42cb-9c2b-44c3e74d6e2b`。操作前重新核对身份、实际 PID 和显示所有者。

## 2026-10-14 富内容长列表收尾（历史）

2026-10-14 富内容长列表收尾：当前为 `8d6bd47` + Slint `22804ff`，路径 `/run/slint-bridge-rich-8d6bd47/showcase`，观测 PID **2264**，日志 `live-final.log`。第八页 Rich list 停留纵向起点，Controls 保留 48%；无退出定时器、trace、事件录制、回放器或 WBC。

新页含 100 项渐变阴影 PNG、运行时 SVG 小图标和文本，纵向可滚动 11652 px、横向 24788 px。当前启动实测纵向 **12.43 FPS / 94.89% CPU / 17.86 MiB 平均 RSS**，横向 **11.53 FPS / 95.72% CPU / 18.20 MiB 平均 RSS**。两方向分别采样 CPU/RSS 与完成提交 FPS；原生 MCP 通过完整范围和六张实机截图，两个物理页条带共 102400 像素与同板截图最大通道差 1。TDE 每绘制帧约 1 次复制、1.25–1.62 次混合，纹理缓存命中约 37%–38%；没有将调用计数换算为硬件忙碌率。方法见[富内容列表报告](nx5-rich-list.md)。

用户报告重启后重新部署，最终启动 ID 为 `4aa4f5d2-8de2-42cb-9c2b-44c3e74d6e2b`，重启原因未判定。旧 tmpfs 副本已随重启消失，本次恢复最新程序和 `abb2e33` 回退；SDK 使用与原副本同摘要的系统文件。五个新启动下的有限进程正常退出，47 个诊断文件校验归档清理。当前 RSS 18752 KiB、VmHWM 23516 KiB（含截图峰值）、MMZ 12100 KiB / 5 块，可用内存 39540 KiB；`/tmp` 使用 10344 KiB、`/run` 使用 11348 KiB。操作前重新核对身份、实际 PID 和显示所有者。

## 2026-10-14 单格式缩放收尾（历史）

2026-10-14 PNG / SVG 单独缩放测量收尾：正式程序仍为 `abb2e33` + Slint `22804ff`，路径 `/tmp/slint-bridge-scaling-abb2e33/showcase`，恢复后观测 PID **7333**，日志 `live-after-format-profile.log`。Image scaling 恢复为左右双图 100%，Controls 保留 48%；无退出定时器、trace、录制、回放或 WBC。

单 PNG 为 **28.99 FPS / 65.23% CPU / 13.49 MiB 平均 RSS**，单 SVG 为 **28.06 FPS / 84.38% CPU / 14.83 MiB 平均 RSS**；同一诊断构建的双图为 25.99 FPS，空图对照为 28.96 FPS。CPU 为关闭 trace 的单核占比，FPS 使用独立进程。临时诊断仅改变图片选择与测试文案，正式源码已恢复；方法与归因限制见[单格式缩放报告](nx5-scaling-formats.md)。

八个有限进程正常退出并释放 VO/MMZ；71 个诊断文件校验归档清理，原常驻和三个恢复二进制均保留。针对测试中 SVG 未显示的反馈，核对当时为 `none` 空图对照；恢复后原生截图与两张物理页的 16 个四色点一致，未做整帧、WBC 或 LCD 扫描验证。恢复后 RSS 15284 KiB、VmHWM 20004 KiB（含恢复截图峰值）、MMZ 12100 KiB / 5 块，可用内存 23532 KiB。下次操作前重新核对身份、实际 PID 和显示所有者。

## 2026-10-14 双图运行时缩放收尾（历史）

2026-10-14 PNG / SVG 运行时缩放验证收尾：当前为 `abb2e33` + Slint `22804ff`，路径 `/tmp/slint-bridge-scaling-abb2e33/showcase`，观测 PID **31614**，无退出定时器、trace、QuickFill、录制、回放或 WBC。界面停留独立第七页 Image scaling，缩放 100%，Controls 保留 48%。

主机保留缓冲 1420342 像素对照、NX5 七页/17 张原生 MCP 截图及四档缩放检查通过。独立关闭 trace 的缩放窗口 CPU **88.35%** 单核，RSS 15488–15612 KiB；另一个窗口的 900 次有效完成提交为 **25.87 FPS**。三个有限进程均正常退出，VO 关闭、MMZ 回到 4900 KiB / 2 块。方法及限制见[运行时缩放报告](nx5-runtime-scaling.md)。

`3892780` 为直接恢复版本，`a0e880c` 和 `c087071` 的程序、脚本及原保留日志继续保存。收尾 RSS 15360 KiB、VmHWM 15360 KiB、MMZ 12100 KiB / 5 块、可用内存 23784 KiB。未修改库运行时、SDK、驱动或开机服务，没有新增物理页、WBC、自动 evdev 或手指验证。下次操作仍须重新核对身份、实际 PID 和显示所有者。

## 2026-10-14 横向多屏滑动收尾（历史）

2026-10-14 横向多屏滑动验证收尾：当前为 `3892780` + Slint `22804ff`，路径 `/tmp/slint-bridge-horizontal-3892780/showcase`，观测 PID **24320**，日志 `live-final.log`。无退出定时器，trace、QuickFill、事件录制、回放器和 WBC 关闭。界面停留 Devices 横向卡片起点，Controls 配置值保留为 48%。

新增 18 张横向卡片，实际滚动距离 3610 px；六页/13 张原生 MCP 截图、14 次宽幅手势的边界/误触检查及 12 次自动 evdev 接触通过。持续运动的独立窗口测得 CPU 85.43%、RSS 14136–14396 KiB、28.64 FPS；包含端点停留的往返负载综合为 22.84 FPS。两次因定时退出截断的性能窗口已排除。完整方法与范围见[横向测试报告](nx5-horizontal-scroll.md)。

上一版 `a0e880c` 保留回退，更早的 `c087071` 也显式保留；63 个本轮诊断文件已校验归档清理。常驻 RSS 15388 KiB、VmHWM 15704 KiB、MMZ 12100 KiB / 5 块，可用内存 34064 KiB；`/tmp` 保留三份程序，使用 31376 KiB。本轮未修改库运行时、SDK、驱动、Slint 或开机服务，没有新增手指、物理页逐像素或 WBC 验证。下次操作前仍需核对设备身份、实际 PID 和显示所有者。

## 2026-10-14 正式版资源对照收尾（历史）

2026-10-14 新旧正式版资源对照收尾：当前仍为 `a0e880c` + Slint `22804ff`，重新启动后的观测 PID **13408**，路径 `/tmp/slint-bridge-input-a0e880c/showcase`，日志 `live-after-release-ab.log`。原启动脚本常驻，无退出定时器，trace、临时阶段计时、QuickFill、WBC 和事件录制关闭，Controls 为 48%；`c087071` 原程序及脚本保留回退。

旧→新→新→旧的无 trace 对照显示：新版 Motion CPU 约增加 0.76 个百分点，静态页和透明滑块接近，列表约降低 9.04 个百分点；各场景 RSS 均值差为 −20 至 +36 KiB，MMZ 均为 12100 KiB / 5 块。独立帧率窗口三场景均约 29 FPS，合计 4500 次有效完成提交。六个有限进程正常退出，测试窗口无新增 MMZ 强制回收或 OOM。完整方法、范围和原始证据见 [正式版资源报告](nx5-release-overhead.md)。

本轮未重建或修改正式程序、SDK、驱动、Slint 或开机服务，没有新增物理像素、WBC 或手指触摸验证。设备身份再次核对为同一 USB1 / ADB `192.168.1.151:5555`、#233、同一启动 ID；下一次操作仍需重新核对实际 PID 和显示所有权。

## 2026-10-14 输入延迟部署记录（历史）

2026-10-14 输入延迟优化收尾：当前为 `a0e880c` + Slint `22804ff`，路径 `/tmp/slint-bridge-input-a0e880c/showcase`，观测 PID **7241**。无退出定时器，trace、QuickFill 诊断、WBC 和观察器关闭；清理版 `/tmp/slint-bridge-clean-c087071/showcase`（`c087071`）及其启动脚本保留回退。Controls 为 48%。

整屏半透明自动回放的输入 P95 由约 16.88 ms 降至 0.86 ms（MCP 精度 1 ms），每段连续写页占用约 1.05 ms；整帧写入仍约 20 ms。3686400 个真实页像素完全匹配，100 帧 TDE/内存双页对照通过。最终展示应用完成六页/十张 MCP 截图、112 次自动接触及 29 FPS 列表窗口；普通界面渲染/布局尾延迟仍存在。本轮按用户要求仅做自动输入，没有新增手指确认，详见 [完整报告](nx5-input-latency.md)。

24 个设备诊断/更早回退文件已归档清理。SDK、固件、驱动、Slint、网络和开机服务未改；设备仍为 USB1 / ADB `192.168.1.151:5555`、#233，同一启动 ID。操作前重新核对实际设备、显示所有者与恢复文件。下方内容均保留其采集阶段的历史口径。

## 2026-10-13 清理版部署记录（历史）

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
