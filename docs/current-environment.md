# NX5 当前环境

2026-10-11：已移除 NX5 专用 30 FPS 钳制，当前运行 `/run/slint-bridge-mcp/display-mcp --textures`，PID 以环境 JSON 为准。Slint 原生 MCP 监听设备 `127.0.0.1:9315`，主机通过 ADB 转发到 `http://127.0.0.1:19315/mcp`；已实际验证 16/18 个工具。构建、调用方式、UI 截图及底层测试保留范围见 [MCP 接入记录](slint-mcp.md)。常驻实例关闭持续性能日志，避免填满 `/run`。

后续 [单周期翻页调查](nx5-single-cycle.md) 完成 128 次无像素写入的状态观测：新页准备与锁存仍分别占一次刷新，纯翻页 29.00031 FPS。没有改变驱动或正式显示实现，原进程和 MCP 已恢复运行；未形成可部署的安全单周期路径。

当前为 DPower Terminal `/dev/ttyUSB1`、网络 `192.168.1.151:5555`。已完成双页上屏与 TDE 接入。当前纹理混合演示已优化至约 29 FPS；运行进程以 [环境记录](../test-app/devices/nx5/current-target.json) 为准。诊断故障恢复时重启过一次，当前 boot ID 为 `938c34d4-456a-4b1a-b532-678c0f92efd0`，初始化记录见 [上屏验证](nx5-display.md)，当前绘制范围见 [TDE 报告](nx5-tde.md)，整屏性能见 [帧率优化](nx5-fps.md)。

当前版本已采用通用后台提交线程，允许 Slint 绘制与 NX5 阻塞翻页并行；最终构建 90 秒测试稳态 29.00 FPS、TDE 错误 0。模块分工、并行重叠及双页验证见 [并行化报告](nx5-parallel.md)。

后续已修复 10 ms 内核定时精度导致的调度迟到：未启用板级 CPU 优化的构建也在 90 秒测试中稳定为 29.00 FPS。当前运行优化构建；等待策略及 CPU 成本见 [调度优化](nx5-scheduling.md)。

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
| 当前 UI | 没有发现运行中的 UI 程序 |

连接检查通过，显示测试的准备条件尚未满足。`/proc/umap/vo` 的 DEV CONFIG 为空，尚无启用的显示设备。查询 framebuffer 时，串口记录了 `gui layer region(0,0,1024,600) cannot exceed frame region(0,0)` 和窗口尺寸设置失败的告警。

首次 sysfs 读到虚拟尺寸 1024 × 2400；包含 `fbset -i` 的查询之后，两次读数均为 1024 × 1200。没有发送显式模式设置命令，但打开 framebuffer 可能触发厂商驱动初始化；本次没有进一步判定变化原因。不能把首次读数当作已验证的四页缓冲能力，后续初始化后应重新核对。

初次环境确认没有上传、启动或停止应用，没有重启、运行翻页探针或修改网络配置。下一步应先核对新 BSP 对应的初始化流程与厂商库，再进行显示初始化和独立验证。当前固件的无撕裂状态为“尚未测试”，旧环境的观测不能作为它的准入依据。

当前目标、驱动与库的 SHA256 见 [环境记录](../test-app/devices/nx5/current-target.json)。最新串口复核见 [serial-recheck.txt](../evidence/environments/2026-10-10-usbtty1/serial-recheck.txt)。初次检查时的串口和 ADB 记录保留于 [serial.txt](../evidence/environments/2026-10-10-usbtty2/serial.txt)、[adb.txt](../evidence/environments/2026-10-10-usbtty2/adb.txt)。
