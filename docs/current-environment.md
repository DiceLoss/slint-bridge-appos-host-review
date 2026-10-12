# NX5 最近环境记录

最近一次设备记录时间为 **2026-10-12 09:20:25（北京时间）**，对应 [current-target.json](../test-app/devices/nx5/current-target.json) 的 `checked_at_utc`。本文保存观测快照，不实时探测设备；PID、端口和启动状态在操作前需重新确认。时钟、调度 CPU 及单周期调查保留各自报告的历史测量。

[三项 TDE/翻页实机回归](nx5-hardware.md) 已补齐此前未重跑的 ignored 测试。ARM 程序基于 `a9fd3c8` 重建，库实现与 `949ceb9` 一致；三个测试逐项独占运行，均为 1 passed、0 failed、0 ignored。TDE 比较和 80 帧增量重绘通过；100 帧双页像素最大通道差 1/255、超容差通道为 0；128 次真实阻塞翻页排除 8 次预热后，均值 34.479450 ms，29.003 FPS。

每项开始与结束均确认 VO/WBC 关闭，MMZ 回到 4900 KiB / 2 块基线，测试期间内核记录无新增 MMZ 泄漏回收。双页像素测试使用真实 TDE 和生产写页代码、内存文件承载两页；真实 framebuffer 的阻塞 PAN 由第三项覆盖，不等同于物理 LCD 的逐帧扫描认证。

测试后恢复原 `949ceb9` 显示程序，路径 `/run/slint-bridge-mcp/display-mcp`，PID **3351**，参数 `--textures --touch=/dev/input/event0`。SHA256 与备份一致：`f2fa90209df824c1b94bbddbf125a70190089f6ebc4aac24934c87dd6185dee5`。摘要与来源见 [构建及恢复清单](../evidence/nx5-hardware/2026-10-12-a9fd3c8/deployment-manifest.json)。MCP 设备 localhost 9315、主机转发 19315；6/18 个原生工具检查通过，暂停/恢复及截图正常，最终 RUNNING。

恢复后持续性能日志和故障注入关闭。MMZ 为运行所需的 12100 KiB / 5 块，WBC 未运行；RSS 11080 KiB，`/run` 剩余 2396 KiB。测试程序在归档后已从设备移除，主机保留原始记录和二进制备份。

[2026-10-11 启动回归](nx5-startup.md) 的十次启动/重试、10 帧前台像素、300 帧 WBC 及 60 秒纹理结果仍保持历史口径，本轮没有重跑；EIO 结果见 [177c18a 报告](nx5-tde-exit.md)，混色负对照见 [2cd98fa 报告](nx5-regression.md)。未知 DMA 完成状态仍按原实现保留资源，不能提前释放。

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
