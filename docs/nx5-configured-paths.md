# NX5 自定义路径与动态布局实机验证

2026-10-12，`c03c3bb` 已部署到 USB1 / `192.168.1.151:5555`，自定义设备路径、真实触摸、双页呈现和 WBC 动态加载均通过。内核仍为 Linux 4.9.138 #233，启动 ID 为 `938c34d4-456a-4b1a-b532-678c0f92efd0`；本轮没有重启或修改驱动。

## 部署与结果

显示程序位于 `/tmp/slint-bridge-c03c3bb/display`，使用 `nx5-tde,slint/mcp` 和 Cortex-A7 构建预设。部署副本 SHA256 为 `416c00518e7de9ffdb0ac75c771c748e1fcb1e5342da29606e697e41b2408689`；来源、恢复脚本与各程序摘要见 [部署清单](../evidence/nx5-configured-paths/2026-10-12-c03c3bb/deployment-manifest.json)。

| 验证项 | 实测结果 |
| --- | --- |
| framebuffer 路径 | 配置 `/tmp/slint-bridge-c03c3bb/dev/panel`，链接至 `/dev/fb0`，实际设备号 29:0 |
| 触摸路径 | 配置 `/tmp/slint-bridge-c03c3bb/dev/touch`，链接至 `/dev/input/event0`，实际设备号 13:64、名称 `gsl1686` |
| 实际布局 | ioctl 读出 1024×600、32 bpp、stride 4096、虚拟尺寸 1024×1200、容量 4915200 字节 |
| 双页呈现 | 启动重试用例分别呈现到偏移 0、600；每页 614400 个像素，错误像素均为 0；两次故意过宽的窗口拒绝后，同一组件恢复正常 |
| 真实点触 | 用户在实体屏幕轻点两次，完成暂停/恢复；原始 evdev 记录有两次按下/抬起，Slint 收到 12 条事件，全部 Accepted、无丢失；暂停时 Frame 2543 稳定，恢复后帧号继续增加 |
| WBC 动态加载 | `LD_LIBRARY_PATH` 仅指向 `/tmp/slint-bridge-c03c3bb/lib`；进程映射确认 `libobserver.so` 与 SDK 副本均来自该目录，只有一份 SDK 映射，构造函数只执行一次 |
| 正向 WBC | 300 帧：黑 150、白 150、混色 0；采样 29.0001 FPS，提交统计 28.81–29.00 FPS |
| 混色负对照 | 30 帧全部检出混色，采样函数结果 1，符合预期；不能把此用例进程退出码 0 当作采样通过 |
| 新程序退出清理 | 启动重试、正向 WBC、负对照三次退出均回到 4900 KiB / 2 块 MMZ 基线，无对应新进程的内核 MMZ 强制回收，VO/WBC 均关闭 |

真实触摸使用只读 evdev 观察程序和 Slint 原生 MCP 同时记录，没有注入触摸事件。原始记录见 [evdev 与页偏移](../evidence/nx5-configured-paths/2026-10-12-c03c3bb/device/results/physical-touch.log)、[Slint 事件](../evidence/nx5-configured-paths/2026-10-12-c03c3bb/mcp/physical-touch-events.json) 与 [最终截图](../evidence/nx5-configured-paths/2026-10-12-c03c3bb/mcp/final-running.png)。

SDK 副本的实际文件名为 `libVideoDec-compatible.so`，无版本入口 `libVideoDec.so` 链接到它；同时提供 SDK 自身 SONAME 的链接供 WBC 依赖解析。运行时 [映射记录](../evidence/nx5-configured-paths/2026-10-12-c03c3bb/device/results/pattern-live-root.txt) 确认复用了同一 SDK 会话，未额外启动加载 SDK 的采集进程。

## 发现与修复

第一次 WBC 启动脚本在运行显示程序前退出，原因是设备 BusyBox 没有 `stat` 命令。`run-regression.sh` 已改为通过 `LC_ALL=C ls -ldnL` 读取所选字符设备的主次设备号，再定位 `/sys/dev/char/`。该路径在目标板复核为 `29:0`，修复后正向及负对照均通过；[原始失败日志](../evidence/nx5-configured-paths/2026-10-12-c03c3bb/device/results/pattern-pre-fix-stat.log) 保留。

切换前已备份旧程序并通过 MCP 暂停，确认旧 PID 3351 的 `fb-present` 等待队列空闲后以 SIGTERM 停止。内核因此强制回收了旧进程的三块 TDE MMZ；这属于旧程序的信号终止，不计入上述新程序正常退出结果。原程序和 [恢复脚本](../evidence/nx5-configured-paths/2026-10-12-c03c3bb/restore-old.sh) 保留，后续恢复前仍须重新核对显示所有者。

## 最终状态与范围

新程序保持 RUNNING，PID **3698**，使用上述自定义路径；MCP 仍为设备 localhost 9315、主机转发 19315。持续性能日志、WBC 采集和故障注入均关闭。最终快照 RSS 11092 KiB、3 个线程、运行中 MMZ 12100 KiB / 5 块。设备探针已经退出。当前观测时间及状态见 [环境记录](current-environment.md)；`/tmp` 和 `/run` 中的部署文件在重启后需重新部署。

本轮使用设备实际的 1024×600 模式，没有切换其他物理分辨率。双页像素读回和有限 WBC 样本不能证明每个 LCD 扫描帧；三项独立 ignored TDE/翻页测试、EIO 和其他启动用例没有在本轮重跑，其历史结果仍见对应报告。

[结构化结果](../evidence/nx5-configured-paths/2026-10-12-c03c3bb/results.json) 与全部原始文件保存在本地 `evidence/nx5-configured-paths/2026-10-12-c03c3bb/`，不随 Git 分发。取得该档案后可从仓库根重放：

```sh
python3 evidence/nx5-configured-paths/2026-10-12-c03c3bb/analyze.py
python3 test-app/tools/nx5/display/check-run.py \
  evidence/nx5-configured-paths/2026-10-12-c03c3bb/device/results/pattern.log --wbc
```
