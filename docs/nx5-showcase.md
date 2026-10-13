# NX5 六页展示应用部署与验证

2026-10-13，在 NX5 / Linux #233 上部署 `308afe5` 的 `showcase`，替换原常驻 `f963558` 显示示例。用户随后完成实体操作并确认“已操作，显示和触摸都正常”。界面、参数和主机预览用法见 [测试工程](../test-app/README.md#六页展示应用-showcase)。

## 当前版本

| 项目 | 本次结果 |
| --- | --- |
| 界面 | Overview、Controls、Devices、Motion、Layers、Interaction lab 六页 |
| 设备 | USB1 root、ADB `192.168.1.151:5555`，Linux 4.9.138 #233 |
| 启动 ID | `938c34d4-456a-4b1a-b532-678c0f92efd0`，串口与 ADB 一致，未重启 |
| 程序 | `/tmp/slint-bridge-showcase-308afe5/showcase`，观测 PID 30541 |
| 构建 | Cortex-A7/NEON、`nx5-tde,slint/mcp`、UI 调试信息；ARM EABI5，最高 GLIBC 符号版本 2.18 |
| SHA256 | `81a87f5932fdf820c3295282b9af7afddffbc7aec080826958bf4b2a7a5cc01a` |
| 回退 | `/tmp/slint-bridge-alpha-f963558/display`，原启动脚本及主机校验副本保留 |

沿用原字体、SDK 和 framebuffer/触摸设备链接；没有修改固件、SDK、驱动、网络或开机服务。新二进制约 9 MiB，包含标准控件 SVG 解码和原生 MCP；无需额外部署界面图片。设备只有原来的常规字体文件，字重效果可能与主机字体集合不同。

## 验证结果

- 主机三种板级 feature 的 workspace 测试、冲突组合拒绝检查、格式检查、严格 Clippy 和 ARM release 构建通过。新增应用状态回归覆盖无效表单、跨页保留、取消重置、键盘、筛选后的稳定 ID、日志容量及真实透明像素；主机原生 MCP 六页交互检查通过。
- 设备原生 MCP 检查导航、表单应用、滑块、开关、复选框、触屏键盘、模态遮挡、筛选空状态、设备详情、列表拖动、动画暂停/恢复、原生缓动、透明度、透明开口、点击/拖动区别和记录清空。六页及四种附加状态共保存 10 张 1024×600 截图并检查布局。
- 自动交互录到 227 条事件：224 条 Accepted，3 条 PointerMoved 为 Rejected，丢失和未知事件均为 0。列表实际位置变化及全部业务状态断言通过；未将 Rejected 事件改记为 Accepted。
- 独立 20 秒运行返回 0，退出后 VO 关闭，MMZ 从运行中的 12100 KiB / 5 块恢复到 4900 KiB / 2 块，与启动前快照一致。内核环形日志首部有旧行退出，但启动前末尾 100 行仍连续匹配；新增记录没有 MMB LEAK 或强制解除映射。
- 用户确认实体切页、控件及滑动操作后显示和触摸正常。独立实体录制窗口未捕获事件，确认反馈晚于该窗口；没有新增独立 evdev 对照或触摸时延结果。

设备截图示例：[总览](../evidence/nx5-showcase/2026-10-13/device-ui/01-overview.png)、[控件](../evidence/nx5-showcase/2026-10-13/device-ui/02-controls.png)、[列表](../evidence/nx5-showcase/2026-10-13/device-ui/03-devices.png)、[动画](../evidence/nx5-showcase/2026-10-13/device-ui/04-motion.png)、[图层](../evidence/nx5-showcase/2026-10-13/device-ui/05-layers.png)、[交互记录](../evidence/nx5-showcase/2026-10-13/device-ui/06-lab.png)。这些是 Slint 原生软件快照；原始档案在本地 `evidence/nx5-showcase/2026-10-13/`，不随 Git 分发，查阅需取得该档案。

## 资源与收尾

收尾观测为 RSS 15496 KiB、VmHWM 20284 KiB、3 个线程，MMZ 12100 KiB / 5 块，可用内存 49020 KiB。最后一次受保护截图前可用内存 49036 KiB，高于 16384 KiB 保留要求。持续 trace、WBC、故障注入和事件录制均已关闭，程序保持运行供用户查看。

旧 `f963558` 在原生 MCP 暂停、确认 `fb-present` 等待队列后才发送 SIGTERM。其内核 MMZ 回收属于旧程序信号退出，独立于新版本正常退出验证。切换前确认旧进程及 VO 已释放，没有并行运行两个 SDK 显示进程。

部署工具完成两轮清单、归档和摘要核验；收尾归档清理 23 个普通文件，包括较早的 `2de9168` 和本轮诊断产物。当前 `showcase`、`f963558` 回退、共用字体/SDK/设备链接与活动日志保留，见 [保留策略](../test-app/devices/nx5/deploy-retain.json)。主机 `rollback/display` 摘要与设备 `f963558` 一致。

本次没有测量新界面的显示 FPS，也没有重跑 WBC、三个 ignored 硬件测试、DMA 故障、每帧扫描验证或实际视频接入。界面的 UI ticks、模拟设备数据和练习完成标记不代表这些指标；此前约 29 FPS 和黑底 alpha 合成结果保持其原提交及场景范围。

## 回退

重新核对设备身份和实际显示所有者。将展示应用切到静态页（例如 Controls），关闭弹窗，等原生控件过渡结束，并确认 `fb-present` 已空闲；停止刚核对的 PID，确认进程和 VO 释放后，通过 root 终端运行：

```sh
sh /tmp/slint-bridge-alpha-f963558/start-live.sh \
  > /tmp/slint-bridge-alpha-f963558/rollback-live.log 2>&1 < /dev/null &
```

回退后重新发现 MCP 窗口，确认原界面和帧计数更新，同步环境快照及保留策略。不要直接使用本文的旧 PID。上述路径属于 tmpfs，重启后需重新部署。
