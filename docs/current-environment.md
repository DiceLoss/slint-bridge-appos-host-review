# NX5 最近环境记录

最近一次设备记录时间为 **2026-10-12 13:50:15（北京时间）**，对应 [current-target.json](../test-app/devices/nx5/current-target.json)。本文是观测快照，PID、端口和所有权在操作前仍须重新核对。

当前运行 `bccf6b9`，路径 `/tmp/slint-bridge-review-fixes/display`，PID **3972**。最新 [截图内存复验](nx5-screenshot-memory.md) 已完成历史 tmpfs 产物归档清理，十次原生 MCP 截图通过；清理与验证期间保持同一显示进程，没有更换二进制或重启设备。

本轮删除 901 个已不用的普通文件，文件内容合计 41.66 MiB，以及两个旧故障注入库链接；所有文件先复制到主机并核对 SHA256。`/run` 已用从 43448 KiB 降至 804 KiB，`/tmp` 当前已用 12372 KiB。最终可用内存 53756 KiB，截图期间最低 48332 KiB（47.2 MiB），RSS 峰值 16168 KiB，截图后 RSS 11360 KiB；三个线程及 MMZ 12100 KiB / 5 块保持不变，内核日志无新 OOM 或强制回收。

当前程序、字体、SDK/WBC 库、设备链接、活动日志和独占锁仍在原路径，保留一份 `c03c3bb` 恢复程序及其 `start-live.sh`。过期诊断文件以及本轮采样数据均已归档到主机，设备上不再保留这些副本。归档位置及摘要清单见 [截图报告](nx5-screenshot-memory.md#证据)；后续部署须注意 `/run`、`/tmp` 共用物理内存预算。

最终界面 RUNNING，MCP 设备 localhost 9315、主机转发 19315。截图与暂停/恢复检查通过；[最后一次截图](../evidence/nx5-screenshot-memory/2026-10-12/mcp/screenshot-10.png) 和 [最终设备快照](../evidence/nx5-screenshot-memory/2026-10-12/final-device.txt) 为本地档案。持续 trace、WBC 和故障注入关闭。二进制 SHA256 仍为 `8aba5bfd344fc60f32f90ace2467206abc326956e20efdc5bcd1878d5b2fd107`；`/tmp`、`/run` 文件重启后需重新部署。

此前同进程初始化重试、VO/描述符缓存清理、WBC 正负对照及 60 秒纹理退出见 [审查修复报告](nx5-review-fixes.md)。该阶段 PID 3949 的截图 OOM、旧 PID 3698 的 SIGTERM 回收和当时未重试截图的范围均保留原始记录。此次清理后的成功截图单独记录，不改写前次结果。

framebuffer、触摸及 SDK/WBC 继续使用 `/tmp/slint-bridge-c03c3bb/` 下的设备链接和库目录，实际为 1024×600、stride 4096、双页。此前完整像素与用户真实触摸验证见 [路径报告](nx5-configured-paths.md)，没有在本轮重复。

[三项 TDE/翻页回归](nx5-hardware.md) 记录此前 `a9fd3c8` 的独立 ignored 测试；[启动回归](nx5-startup.md) 记录 `949ceb9` 的完整启动矩阵；[EIO 回归](nx5-tde-exit.md) 保留 `177c18a` 的结果。这些未在本轮全部重跑，不等同于当前二进制的新验证。

[单周期翻页调查](nx5-single-cycle.md) 完成 128 次无像素写入的状态观测：新页准备与锁存仍分别占一次刷新，纯翻页 29.00031 FPS。调查没有改变驱动或正式显示实现，结束时恢复原进程和 MCP；未形成可部署的安全单周期路径。

记录中的目标为 DPower Terminal `/dev/ttyUSB1`、网络 `192.168.1.151:5555`。已完成双页上屏与 TDE 接入，纹理混合演示的有限时长测试约为 29 FPS。诊断故障恢复时重启过一次，记录的 boot ID 为 `938c34d4-456a-4b1a-b532-678c0f92efd0`。初始化记录见 [上屏验证](nx5-display.md)，绘制范围见 [TDE 报告](nx5-tde.md)，整屏性能见 [帧率优化](nx5-fps.md)。

当前源码采用通用后台提交线程，允许 Slint 绘制与 NX5 阻塞翻页并行；该阶段构建的 90 秒测试稳态 29.00 FPS、TDE 错误 0。模块分工、并行重叠及双页验证见 [并行化报告](nx5-parallel.md)。

后续修复了 10 ms 内核定时精度导致的调度迟到：未启用板级 CPU 优化的构建也在该轮 90 秒测试中稳定为 29.00 FPS。等待策略及 CPU 成本见 [调度优化](nx5-scheduling.md)。

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
