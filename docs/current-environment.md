# NX5 最近环境记录

最近一次设备记录时间为 **2026-10-12 18:32:20（北京时间）**，对应 [current-target.json](../test-app/devices/nx5/current-target.json)。这是观测快照；操作前仍须重新核对设备、PID 和显示所有权。

当前常驻 `f963558`，路径 `/tmp/slint-bridge-alpha-f963558/display`，观测 PID **22475**。用户最初要求部署 `287da8c`；实测发现半透明合成偏亮后，用户授权修复并部署新提交。新版本为 NX5 写屏 alpha 使用实测的 0–128 量程，Slint 画布和 TDE 运算仍保持标准 0–255。完整条件、失败与修复证据见 [透明 UI 实测](nx5-alpha.md)。

软件和 TDE feature 共检查 10 个实际前台帧、6144000 个像素，错误为 0；50 个 WBC 样本的半透明 RGB/参考色最大 Y/UV 差为 1。底层为现有黑色 VO 背景，未接入视频解码器或验证非黑色视频层。两条路径正常退出均关闭 VO/WBC，MMZ 回到 4900 KiB / 2 块，无对应测试进程的强制回收。

修复前后各运行 60 秒纹理场景：均为首个统计窗口 28.44 FPS、后续 10 个窗口 29.00 FPS，TDE 错误为 0。帧率统计成功完成的显示提交；常驻 trace 已关闭，不将 UI timer 帧号当作 FPS。部署二进制与修复版性能测试文件 SHA256 相同：`cc87c63d826ab01459a05005f5eea25888dbd1398990e4b38421128109490d21`。

基线 `287da8c` 的用户实体操作录到 12 组接触、49 条 Accepted 事件，零丢失；内核报告到 Slint 分派的保守上界约 11.3 ms，用户确认响应及时。修复后的有限实体确认窗口没有新输入；`f963558` 另完成三轮 MCP 暂停/恢复，24 条事件 Accepted、零丢失。两种验证范围分开记录，MCP 不替代 evdev 实测。

最终界面 RUNNING，原生 MCP 截图为 1024×600，截图前可用内存 53684 KiB；[最终截图](../evidence/nx5-alpha-deploy/2026-10-12-287da8c/final-running.png) 已检查。持续 trace、WBC、输入观察器、事件录制均已关闭。资源快照为 RSS 11056 KiB、VmHWM 15796 KiB、3 个线程、MMZ 12100 KiB / 5 块、可用内存 53600 KiB。

任务开始时的 `2de9168` 保留为回退：`/tmp/slint-bridge-touch-2de9168/display`，SHA256 `425a1660192aa8886dea14d8d3d834a6a6a95a6035fe259fbdb4efab3d19af30`，原启动脚本和主机校验副本均保留。[保留策略](../test-app/devices/nx5/deploy-retain.json) 已同步；`bccf6b9` 和中间 `287da8c` 程序及不用的诊断产物已归档校验后清理。回退步骤见 [部署报告](nx5-alpha.md#最终部署与回退)。tmpfs 路径在重启后需重新部署，本轮未改开机服务。

设备仍为 USB1 root、ADB `192.168.1.151:5555`、Linux #233，启动 ID `938c34d4-456a-4b1a-b532-678c0f92efd0`。固件、SDK、驱动与网络未改；framebuffer 和触摸继续使用 `/tmp/slint-bridge-c03c3bb/dev/` 链接。旧常驻与中间版本被 SIGTERM 结束时的内核 MMZ 回收单独保存，不计为新版本正常退出结果。

原始档案位于本地 `evidence/nx5-alpha-deploy/2026-10-12-287da8c/`，不随 Git 分发，需另行取得。本轮没有重跑三个独占 ignored 用例、EIO、初始化重试或每一帧扫描的无撕裂认证。

## 历史验证范围

[此前触摸部署](nx5-touch-response.md) 对应 `2de9168` 的上一轮 10 组点触；[会话重试](nx5-review-fixes.md) 与 [截图内存复验](nx5-screenshot-memory.md) 对应 `bccf6b9`。[三项硬件回归](nx5-hardware.md) 对应 `a9fd3c8`，[启动回归](nx5-startup.md) 对应 `949ceb9`，[EIO](nx5-tde-exit.md) 对应 `177c18a`。这些原始成绩保留，不改记为本轮结果。

[并行化](nx5-parallel.md)、[调度](nx5-scheduling.md) 和 [单周期调查](nx5-single-cycle.md) 保留对应阶段的有限测量。本轮继续使用原阻塞双页完成契约，没有改成未经验证的单周期提交。

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
