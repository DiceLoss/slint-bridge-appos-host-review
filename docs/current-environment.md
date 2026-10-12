# NX5 最近环境记录

最近一次设备记录时间为 **2026-10-12 12:31:24（北京时间）**，对应 [current-target.json](../test-app/devices/nx5/current-target.json)。本文是观测快照，PID、端口和所有权在操作前仍须重新核对。

当前运行 `bccf6b9`，路径 `/tmp/slint-bridge-review-fixes/display`，PID **3972**。同进程初始化重试、VO/描述符缓存清理、WBC 正负对照及 60 秒纹理退出验证见 [审查修复报告](nx5-review-fixes.md)。framebuffer、触摸及 SDK/WBC 继续使用 `/tmp/slint-bridge-c03c3bb/` 下的设备链接和库目录，实际为 1024×600、stride 4096、双页。此前 `c03c3bb` 的完整像素与用户真实触摸验证见 [路径报告](nx5-configured-paths.md)，没有在本轮重复。

WBC 正对照 300 帧无混色，负对照 30/30 帧检出混色；纹理首批 28.44 FPS、后续 29.00 FPS，TDE 错误 0。三个有限用例退出后均回到 4900 KiB / 2 块 MMZ，无对应新进程的内核强制回收。

恢复阶段的首次 MCP 截图触发 OOM，内核终止 PID 3949；异常和此前停止旧 PID 3698 的 SIGTERM 回收均单独记录。已移除本轮约 5.7 MB 临时诊断副本，恢复 PID 3972，使用原生 MCP 属性及暂停/恢复确认 RUNNING；未重试截图。最终 RSS 11108 KiB、3 个线程、可用内存约 8640 KiB，运行中 MMZ 12100 KiB / 5 块。

MCP 设备 localhost 9315、主机转发 19315。持续 trace、WBC 和故障注入关闭；二进制 SHA256 为 `8aba5bfd344fc60f32f90ace2467206abc326956e20efdc5bcd1878d5b2fd107`，见 [部署清单](../evidence/review-fixes/2026-10-12/deployment-manifest.json) 与 [最终快照目录](../evidence/review-fixes/2026-10-12/device-restored/)。旧 `c03c3bb` 程序及启动脚本仍在原目录，可供恢复；使用前须重新确认当前显示所有者。`/tmp`、`/run` 文件重启后需重新部署。

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
