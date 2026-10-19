# F1 showcase 显示与性能记录

2026-10-19，设备 `fd_f1_r88r30_spinor-20c31b4578`，ArtInChip D211 单核
RISC-V64，Linux 5.10.44 `#1 Fri May 22 02:04:02 UTC 2026`。
本轮 boot ID 为 `dac34b8d-d45f-4fb9-a5fc-ba8902eda615`，仅用于关联证据。
原始档案在忽略目录 `evidence/f1-showcase-20261019/`，复核需另行取得；
下述 SD 目录及 PID 都是本轮快照，操作时必须重新查询。

## 显示接管与验证

按用户要求采用参考工程 `/home/dpower/data/GitLab/slint-f1` 的
PAN 后 WAITFORVSYNC 方式；板级实现仍检查固件、布局与显示所有权。
[镜像分析](f1-kernel-analysis.md) 中的提交归属/旧页退休缺口没有因此消失。
本轮不宣称逐帧无撕裂认证。

原应用运行时尝试初始化被拒绝，日志指出 framebuffer 已有所有者。
核对身份及恢复流程后，仅停止 `/system/app0`；`/system/control_service`
保持原进程及 starttime。未替换原固件、启动脚本或显示时序。
测试固件、字体、日志全部放在真实 SD 挂载下的
`/tmp/sdcard/slint-f1-showcase-gf05wi`。

CPU 与 G2D 各自有限运行 180 秒，均记录 `APP_EXIT=0`；两次控制脚本均
`PROFILE_DONE` 且退出码为 0。原生 Slint MCP 查得 1024×600、scale=1，
访问 Overview、Controls、Devices、Motion、Layers、Interaction lab、
Image scaling、Rich list、Clock 共九页，并完成富列表连续滚动。
这验证 MCP 导航及注入，不替代真实 evdev 触摸方向和完整业务功能回归。

每次有限运行退出后读取静止前台页，CPU/G2D 均为 1024×600 BGRA8888、
stride 4096、yoffset 600、2457600 bytes。CPU 前台页已解码检查：文字、
壁纸、半透明卡片和图表完整，未发现该样本的色序错误；G2D 页亦解码核对。
两页在 (0,0)、(1000,0)、(5,580) 的 BGRA 分别为
`3b2a11ff`、`ffffffff`、`3b2a11ff`。这些静止像素不证明动态扫描时无撕裂。
MCP 软件截图单独保存，未拿它代替物理页读回。

## 性能方法与结果

相同 release 构建条件：`slint/mcp`、Slint debug 元数据、同一字体和背景，
分别选择 `f1` 与 `f1-g2d`；没有改 Slint renderer，也没有修改默认帧间隔。
使用设备本地 `profile-local.sh`、`observe-resources` 与 `mcp-scroll`，
各场景预热 3 秒后标记采样 20 秒。资源采样有效跨度约 18–19 秒，
提交 trace 首末有效跨度约 17–18 秒；首末累计差计算 FPS，排除跨场景窗口。
静态页无周期提交记录，按需绘制，不填虚构的 0/60 FPS。

| 场景 | 提交 FPS：CPU / G2D | 应用 CPU：CPU / G2D | 最大 RSS MiB：CPU / G2D | 最小 MemAvailable MiB：CPU / G2D |
| --- | ---: | ---: | ---: | ---: |
| Controls 静态 | 按需 / 按需 | 0.5% / 0.5% | 15.01 / 15.07 | 17.22 / 17.22 |
| Overview 动画 | 18.78 / 18.11 | 98.8% / 96.1% | 15.02 / 15.10 | 17.26 / 17.17 |
| Motion 动画 | 21.76 / 21.70 | 98.4% / 98.4% | 15.27 / 15.35 | 17.25 / 17.25 |
| Rich list 滚动 | 10.11 / 9.82 | 95.0% / 94.1% | 18.88 / 18.73 | 13.84 / 13.88 |

单核 CPU 百分比由进程 ticks / CLK_TCK(100) / 单调时间计算。
采样器约消耗 0.2–0.3% CPU。富列表控制器 CPU/G2D 分别完成 412/404 个
MCP 请求，实际约 20.5/20.2 次每秒；每次应答后等待 20 ms、每 3 秒换向，
单次 deltaY 为 ±30。控制器自身约 3.1%/3.0% CPU，不包含在上表应用 CPU。
这是同一闭环负载，实际请求数不同，不能解读为固定输入速率的严格吞吐比较。

PAN ioctl 平均约 0.029–0.032 ms，VSYNC wait 约 9.0–9.6 ms。
DE IRQ 约 53.6 次/秒，与配置时序约 52.7 Hz 为不同口径；不当作应用 FPS。
G2D 在 Overview 有约 0.94 次/秒 GE IRQ，其余所测稳定窗口为 0；
CPU 模式均为 0。结合实现只加速受支持的不透明矩形填充，本轮复杂文字、
透明合成、图片、缩放与滚动仍主要由 CPU 执行，未观察到 G2D 明显提速。
单次样本 0–4% 的差异不足以断言硬件加速带来退化。

系统 MemTotal 57952 KiB、无 swap；应用各阶段 RSS 约 15–19 MiB，
HWM 约 19.96/20.14 MiB（包含采样前软件截图峰值）。RSS 不包含所有
DMA/驱动内存，不能据此推算系统总开销。富列表 G2D 窗口出现 1 次 major
fault，其他有效窗口为 0。频率 sysfs 字段不可用，未做 CPU 频率固定验证。

早期主机控制采样遇到 USB/ADB 重连，保留为故障档案，不并入此表。
本表使用完整的 `cpu-uart-*`、`g2d-uart-*` 设备本地采样；USB 异常仍可能
带来系统后台干扰。用户随后已修复重连，本轮不继续调查 USB。
这些数据是接入基线，尚未进行多轮方差、长时间稳定性、温度或功耗测试。

## 最终交付与检查

性能表对应归档的 `*-measured` 固件；之后仅整理驱动 ID 的 c_char 比较
以通过 clippy，再交叉构建最终 CPU/G2D，保留两种产物。最终 G2D 已部署并
回读 SHA256 一致，重新通过原生 MCP 九页导航，Overview 持续显示。
最终导航后 MemAvailable 为 14600 KiB，因此按 16 MiB 阈值跳过新的截图；
报告中的截图/像素样本来自前述有限运行。最终进程启动后日志可见约 19 FPS。

主机 feature 矩阵、fmt、nx5-tde + MCP 与 f1-g2d + MCP 的 workspace clippy
均通过；F1 定向 5 项单元测试通过（固件/布局拒绝、PAN/WAIT/读回失败停止，
以及 GE 软件回归），Rust API 文档构建通过。RISC-V release CPU/G2D 构建通过。
辅助 C 程序以 `-Wall -Wextra -Werror` 构建，控制器在本次设备实测成功。
未新增真实 GE 故障注入或物理触摸测试，未重跑无关 NX5 实机测试。

原应用与控制服务恢复方式见 [F1 设备说明](../test-app/devices/f1/README.md)。
原始固件摘要如下，用于区分性能样本与最终交付，不把旧日志算作新构建实测：

| 产物 | SHA256 |
| --- | --- |
| CPU 性能样本 | `e1f11c296d9adeeb4db249d6caf21c9a51814dbf3226957f24f300755325bad4` |
| G2D 性能样本 | `9e10d16e8ba2f0f63382b0720c5edb75ac4e14009e4af702fee54f308512b56d` |
| 最终 G2D（设备读回） | `54f573fc69915db58caeb7f38b1c081e6599a6c3cc26605d28f4b5b512f1799c` |
