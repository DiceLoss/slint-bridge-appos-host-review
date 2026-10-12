# NX5 截图 OOM：临时产物清理与复验

2026-10-12 在 `bccf6b9` 的同一显示进程 PID 3972 上完成清理和截图复验。运行程序、SDK、Slint 和驱动均未修改，也未重启显示或设备。此前截图 OOM 及恢复过程保留在 [审查修复报告](nx5-review-fixes.md)，本轮不改写其原始结论。

## 原因与清理

上次 OOM 日志显示共享内存占用 63792 KiB、空闲内存仅约 1200 KiB。当前板的 `/run`、`/tmp` 都是 tmpfs，且没有 swap；其中历史测试二进制、截图和日志会直接占用物理内存。它们不会因为 `Cached` 数字较大就能被回收。

本轮操作前重新核对 USB1 root 会话、ADB `192.168.1.151:5555`、#233 内核、启动 ID `938c34d4-456a-4b1a-b532-678c0f92efd0` 及活动进程。显示进程 RSS 仍为 11108 KiB，可用内存仅 8236 KiB；已闲置的 `/run/slint-bridge-tde/` 就占约 17 MiB。

本地 Slint 的软件截图实现同时持有两份 1024×600×4 字节像素缓冲，共 4800 KiB，还需编码及协议处理内存。清理后实测进程 RSS 峰值增加约 5060 KiB。清理前后程序、PID 和截图路径不变，清理后十次截图通过，支持本次 OOM 由临时产物挤占截图所需内存引起；十次复验中未观察到持续增长。

先将 912 个普通文件归档到主机并逐文件校验 SHA256，再删除其中已不用的 901 个文件（41.66 MiB 文件内容）、两个旧故障注入库链接及空目录。归档普通文件时排除指向 framebuffer/evdev 的设备链接，不通过链接读取设备。保留：

- 当前 `bccf6b9` 程序、启动脚本和正在写入的日志。
- 一份 `c03c3bb` 恢复程序及其启动脚本、共用 SDK/WBC 库和设备链接。
- 当前字体及许可证、活动 ADB 日志、DHCP 日志、显示独占锁和系统运行文件。

历史 TDE、MCP、调度、并行、启动及回归目录中的旧设备副本已清除，原始内容保留在主机档案。清理后又归档并移除了本轮采样脚本与数据，不留下新的设备诊断目录。

## 复验结果

| 项目 | 清理前 | 清理及截图复验后 |
| --- | ---: | ---: |
| `/run` tmpfs 已用 | 43448 KiB | 804 KiB |
| `/tmp` tmpfs 已用 | 约 15080 KiB | 12372 KiB |
| `MemAvailable` | 8236 KiB | 53756 KiB |
| 显示进程 RSS | 11108 KiB | 11360 KiB |
| 显示进程 RSS 高水位 | 11108 KiB | 16168 KiB |
| TDE/MMZ | 12100 KiB / 5 块 | 不变 |

原生 MCP `take_screenshot` 共调用十次，均返回有效的 1024×600 PNG，已检查运行与暂停状态截图。三个暂停截图逐字节一致；恢复后状态为 RUNNING，帧计数继续增长。图像保存在主机，没有重新写入设备 tmpfs。

64 秒内存采样共 892 点，截图期间最低 `MemAvailable` 为 **48332 KiB（47.2 MiB）**。单次截图并查询状态耗时约 0.27–0.32 秒。截图前后内核日志逐字节相同，无新 OOM 或 MMZ 强制回收；活动显示 PID、三个线程及 MMZ 块数不变。重复截图没有超过首次截图的 RSS 高水位。

最终程序仍为 `/tmp/slint-bridge-review-fixes/display`，PID 3972，SHA256 `8aba5bfd344fc60f32f90ace2467206abc326956e20efdc5bcd1878d5b2fd107`，trace/WBC/故障注入关闭。最新内存和部署快照见 [当前环境](current-environment.md)。这验证的是当前尺寸和程序下的顺序截图，没有测试并发截图或任意分辨率下的内存上限。

## 证据

本地档案位于 `evidence/nx5-screenshot-memory/2026-10-12/`，不随 Git 分发；重放和恢复被清理文件前须先取得该档案。

- [清理清单](../evidence/nx5-screenshot-memory/2026-10-12/cleanup-manifest.json) 与 [归档校验](../evidence/nx5-screenshot-memory/2026-10-12/archive-verified.json)：路径、摘要、字节数及保留项。
- [设备文件归档](../evidence/nx5-screenshot-memory/2026-10-12/archive/)：历史普通文件；设备链接仅记录目标，不归档设备内容。
- [验证汇总](../evidence/nx5-screenshot-memory/2026-10-12/results.json)、[原始采样和内核日志](../evidence/nx5-screenshot-memory/2026-10-12/verification/)及 [最终设备快照](../evidence/nx5-screenshot-memory/2026-10-12/final-device.txt)。
- [最后一次运行截图](../evidence/nx5-screenshot-memory/2026-10-12/mcp/screenshot-10.png)、[暂停截图](../evidence/nx5-screenshot-memory/2026-10-12/mcp/screenshot-08.png)及 [重复截图记录](../evidence/nx5-screenshot-memory/2026-10-12/mcp/repeat-results.json)。

设备后续部署和截图前的清理步骤已纳入 [MCP 指南](slint-mcp.md#截图前的内存检查)。截图不替代 framebuffer、TDE 或物理扫描输出验证；本轮没有重新执行这些独立硬件回归。
