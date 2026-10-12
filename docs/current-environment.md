# NX5 最近环境记录

最近一次设备记录时间为 **2026-10-12 16:01:26（北京时间）**，对应 [current-target.json](../test-app/devices/nx5/current-target.json)。本文是观测快照，PID、端口和所有权在操作前仍须重新核对。

当前常驻 `2de9168`，路径 `/tmp/slint-bridge-touch-2de9168/display`，PID **30267**。该版本为 wake 任务批次增加回调间的 2 ms 时间预算，避免整批任务长时间推迟输入处理。完整部署、实体触摸结果与回退步骤见 [触摸实测报告](nx5-touch-response.md)。

本轮用户实际触摸了 10 次（含确认的额外连点），Slint 收到 35 条事件，全部 Accepted、无丢失。20 个按下／抬起边沿与 evdev 报告逐条匹配，最长分派时间差约 15.3 ms，MCP 时间戳粒度为 1 ms；该值不包含触摸传感器和液晶扫描延迟。用户确认暂停和恢复及时，状态记录显示 5 次暂停／恢复。

最终界面 RUNNING，MCP 设备 localhost 9315、主机转发 19315。实体录制后又出现用户输入导致的暂停，已在结束录制后通过 MCP 恢复；最终帧号从 22966 增至 22982，[最终截图](../evidence/nx5-touch-deploy/2026-10-12-2de9168/final-running.png) 显示 Frame 22994 RUNNING。持续 trace、WBC、只读 evdev 观察器及事件录制均关闭。

新版 SHA256 为 `425a1660192aa8886dea14d8d3d834a6a6a95a6035fe259fbdb4efab3d19af30`。切换前的 `bccf6b9` 程序继续保存在 `/tmp/slint-bridge-review-fixes/display`，原 `start-live.sh`、共用依赖及主机校验备份也保留；[保留策略](../test-app/devices/nx5/deploy-retain.json) 已指定它为回退版。回退前必须重新核对并停止新版显示所有者，不能同时加载两份 SDK 显示程序。`/tmp`、`/run` 文件在重启后需重新部署，本轮未更改开机服务。

新旧程序切换期间，先通过 MCP 暂停旧 PID 3972 并确认提交线程空闲，再发送 SIGTERM；内核回收旧进程的三个 MMZ 块、关闭 VO，恢复至 4900 KiB / 2 块基线后才启动新版。这是旧进程的信号退出记录，不计为新版正常析构结果。

本轮另将 11 个不用的普通文件归档校验后清理，包括观察器、采样文件和更早的 `c03c3bb` 程序副本；该目录中的共用 SDK、设备链接仍保留。最终可用内存 53548 KiB、RSS 10992 KiB、VmHWM 15800 KiB、3 个线程、MMZ 12100 KiB / 5 块；未发现对应新版进程的新 OOM 或强制回收。原始证据位于本地 `evidence/nx5-touch-deploy/2026-10-12-2de9168/`，不随 Git 分发，需另行获取。

设备仍为 DPower Terminal `/dev/ttyUSB1`、ADB `192.168.1.151:5555`、Linux 4.9.138 #233，启动 ID `938c34d4-456a-4b1a-b532-678c0f92efd0`。没有重启或修改固件、SDK、驱动和网络。framebuffer、触摸设备继续使用 `/tmp/slint-bridge-c03c3bb/` 下的链接；窗口为 1024×600。

## 历史验证范围

此前 `bccf6b9` 的同进程初始化重试、VO/描述符缓存清理、WBC 正负对照及纹理退出见 [审查修复报告](nx5-review-fixes.md)。旧 PID 3949 的截图 OOM、PID 3698 的 SIGTERM 回收，以及随后同一 PID 3972 的 [归档清理与十次截图复验](nx5-screenshot-memory.md) 均保留各自原始记录；不能据此声称本轮重复了这些测试。

自定义路径、双页实际像素和此前真实触摸见 [路径报告](nx5-configured-paths.md)；[三项 TDE/翻页回归](nx5-hardware.md) 对应 `a9fd3c8`，[启动回归](nx5-startup.md) 对应 `949ceb9`，[EIO 回归](nx5-tde-exit.md) 对应 `177c18a`。本次没有重跑 WBC、独立 ignored 测试、EIO，也没有测量新版持续呈现 FPS。

[并行化报告](nx5-parallel.md) 与 [调度测量](nx5-scheduling.md) 记录了此前约 29 FPS 的有限采样；这不是由本轮 UI 帧计数重新得出的结果。[单周期翻页调查](nx5-single-cycle.md) 没有形成安全的单周期实现；新版继续使用现有双页完成契约。

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
