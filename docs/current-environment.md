# NX5 最近环境记录

最近一次设备记录时间为 **2026-10-11 17:42:16（北京时间）**，对应 [current-target.json](../test-app/devices/nx5/current-target.json) 的 `checked_at_utc`。本文保存观测快照，不实时探测设备；PID、端口和启动状态在操作前需重新确认。时钟、调度 CPU、阻塞翻页节拍及单周期调查仍保留各自报告的历史测量，不视为本轮重测。

提交 `949ceb9` 已部署到 `/run/slint-bridge-mcp/display-mcp`，PID **2925**，参数为 `--textures --touch=/dev/input/event0`。SHA256 为 `f2fa90209df824c1b94bbddbf125a70190089f6ebc4aac24934c87dd6185dee5`；库实现未在本轮改动。完整程序及专用诊断程序摘要见 [部署清单](../evidence/nx5-startup/2026-10-11-949ceb9/deployment-manifest.json)。Slint 原生 MCP 监听设备 `127.0.0.1:9315`，主机转发到 `http://127.0.0.1:19315/mcp`；本轮验证 6/18 个原生工具，最终状态为 RUNNING。常驻实例关闭持续性能日志和故障注入。

[最新启动实机回归](nx5-startup.md) 的五个用例各运行两轮：首次显示失败清理、同一组件重试、排队创建、定时器创建，以及循环内显示失败后重试。成功呈现的 10 帧共 6,144,000 个实际前台页像素全部一致；十次退出均回到 4900 KiB / 2 块基线，且对应内核记录无 TDE MMZ 泄漏回收。前台页读回不等同于物理 LCD 扫描认证。

相同正式二进制的 WBC 用例采集 300 帧，黑白各 150、混色 0；60 秒纹理运行首窗 28.44 FPS，后 10 个窗口均为 29.00 FPS，TDE 错误为 0，两次退出同样无 MMZ 泄漏回收。原三个 ignored 硬件测试和 EIO 没有在本轮重跑，历史结果见 [177c18a 报告](nx5-tde-exit.md)；混色负对照的历史结果见 [2cd98fa 报告](nx5-regression.md)。

最终常驻时 MMZ 为 12100 KiB / 5 块，WBC 未运行；RSS 快照为 11140 KiB，`/run` 剩余 2788 KiB。诊断程序已在取回并核对后从设备移除，主机仍保留构建和原始证据。未知 DMA 完成状态仍保留资源以防止提前释放。

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
