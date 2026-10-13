# 清理版 NX5 部署与触摸验证

2026-10-13，将清理提交 `c087071` 部署到 NX5 #233；Slint 仍为 `22804ff`。串口 USB1 root 与 ADB `192.168.1.151:5555` 的启动 ID、MAC 一致，未重启设备或修改固件、SDK、驱动、网络及开机服务。

本次清理仅删除未使用的界面导入、内部构造函数多余的 `Result` 包装。按 `.cargo/nx5.toml`、`nx5-tde,slint/mcp` 和 `SLINT_EMIT_DEBUG_INFO=1` 重建后，strip 二进制与上一版 `4a2eda2` **逐字节一致**：大小 10314112 字节，SHA256 均为 `729f5088bc670c74ddef6fa002d69859fd67e692e351286ae219ee5bc3c81b7f`。源码清理前后的主机回归、feature 矩阵、格式、严格 Clippy 和 ARM release 构建已通过。

## 部署与显示检查

旧常驻观测 PID 6818。通过原生 MCP 切到静态 Controls 页，确认 `fb-present` 在线程队列等待后发送 SIGTERM；确认进程消失、VO 关闭、MMZ 回到 4900 KiB / 2 块、显示锁释放后启动新版。旧进程的内核 MMZ 回收单独归档，不计入新版正常退出结果。

新版先运行 120 秒有限用例，观测 PID 12253。原生 Slint MCP 完成六页切换、表单、键盘、列表拖动、动画暂停/恢复、透明区域和交互记录检查；10 张受内存检查保护的截图逐张核对，227 条自动交互事件无丢失、无未知事件。截图前最低可用内存 23068 KiB。

有限用例退出码 0，VO 关闭、MMZ 回到 4900 KiB / 2 块、显示锁释放；同次内核记录窗口没有新增 MMZ 强制回收或 OOM。随后从同一二进制启动无定时器常驻。

## 实体触摸

常驻程序观测 PID 12364。原生事件录制与只读 evdev 观察器同步采集；观察器不加载 SDK、不访问 framebuffer、不独占或注入输入。录制期间没有 MCP 自动输入。

用户在 Motion 页暂停/恢复，在 Devices 页上下滑动列表，再在 Controls 页拖动 Output level；明确确认“已完成，显示和触摸都正常”。最终界面为 Controls，滑块显示 48%。

- 8 次完整接触、16 个按下/松开边沿全部与内核 `SYN_REPORT` 的顺序及坐标配对。
- Slint 共 265 条事件：251 条 Accepted、14 条 Rejected，无丢失、无未知事件。Rejected 包含首个触点位于导航按钮旁空隙的按下（原始坐标 164,150）及 13 条移动，不代表输入记录缺失。
- 边沿从内核报告到 Slint 完成分派的中位数约 4.80 ms、最大 54.91 ms；考虑 MCP 毫秒时间戳量化，最大值加 1 ms 为 55.91 ms。该值包含回调，不是触摸到 LCD 扫描的端到端延迟。

完成后停止录制和观察器，重新查询常驻窗口与 48% 状态通过。本轮没有重新测量显示 FPS、WBC、实际 framebuffer 像素、TDE/DMA 故障或逐帧扫描无撕裂；此前 NEON 像素与性能结果保留在 [原报告](nx5-neon-unpremultiply.md)。

## 常驻与回退

| 用途 | 版本 | 设备路径 |
| --- | --- | --- |
| 当前常驻 | `c087071` | `/tmp/slint-bridge-clean-c087071/showcase` |
| 当前启动脚本 | 同上 | `/tmp/slint-bridge-clean-c087071/start-live.sh` |
| 旧版回退 | `4a2eda2` | `/tmp/slint-bridge-neon-final/showcase` |
| 回退启动脚本 | 同上 | `/tmp/slint-bridge-neon-final/start-live.sh` |

常驻未设置退出定时器，trace、QuickFill 诊断开关、WBC 和观察器关闭。收尾观测 RSS 15096 KiB、VmHWM 15380 KiB、VmPeak 33608 KiB、3 个线程；MMZ 为 12100 KiB / 5 块，系统可用内存 44868 KiB。

回退前重新核对设备与实际显示 PID，通过原生 MCP 切静态页并确认提交线程空闲；停止该实际进程，确认进程消失、VO 关闭、MMZ 回到基线、显示锁释放后，执行旧版 `start-live.sh`。重新查询 MCP 句柄并检查显示；不得并行启动两份 SDK 显示程序或照抄本文 PID。路径位于 tmpfs，重启后需重新部署。

[保留策略](../test-app/devices/nx5/deploy-retain.json) 已更新为新版与上一版。17 个完成的诊断文件和更早 `d34f436` 回退文件均先归档、SHA256 校验，再从设备删除。收尾 `/tmp` 使用 21140 KiB、剩余 24704 KiB，`/run` 使用 804 KiB；当前与回退启动脚本、日志、SDK、字体及设备链接保留。

原始档案位于本地 `evidence/nx5-cleanup-deploy/2026-10-13/`：`manifest.json`、`results.json`、`device-ui/`、`finite/`、`physical-touch.log`、`physical-mcp.json`、`physical-summary.json`、`user-confirmation.json` 及 `cleanup/`。该目录被 Git 忽略，检出仓库后需另行取得。
