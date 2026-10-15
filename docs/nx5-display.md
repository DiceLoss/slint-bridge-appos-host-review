# NX5 实际上屏验证

> 软件上屏阶段的历史记录。下方测量使用当时的限帧与 PUT 提交；当前已接入 TDE、改用 PAN 并移除限帧，现行行为见 [平台说明](software-platform.md)。原始证据为不随 Git 分发的本地档案，见 [文档索引](README.md)。

2026 年 10 月 10 日，当前 NX5 #233 固件已运行 Rust/Slint 显示适配层。用户确认采用至多 30 FPS、双 framebuffer 的实机观测口径推进上屏。本轮完成软件渲染路径；TDE 绘制尚未接入。

## 实测结果

| 检查 | 结果 |
| --- | --- |
| 正常动态界面 | 稳态 29.00 FPS，包含启动批次为 28.72–29.00 FPS |
| 连续黑白整帧 | 300 个 WBC 样本，黑白各 150，混色 0 |
| WBC 实际采集速率 | 约 29.00 FPS；检查每行五个横向位置，共 3000 个亮度值/帧 |
| 混色负对照 | 故意绘制半屏灰色，30/30 个样本检出混色 |
| 第二个显示实例 | 在加载厂商库前被独占锁拒绝 |
| 进程内采集结束 | 显示继续运行，WBC 关闭，MMZ 回到 4900 KiB、2 个块 |
| 演示常驻内存 | RSS 7924 KiB，实测快照，非峰值承诺 |
| 主机回归 | `fbdev`、`nx5`、`nx5-tde` 各 13 个测试通过；严格 Clippy 和格式检查通过 |

应用运行在 `/run/slint-bridge-display/display`，本次启动 PID 为 288。画面包含移动条带、帧号和触摸暂停/继续操作。触摸解码已通过主机测试，当前实机已打开 `event0`；本轮没有将人工触摸操作计为已验证项。

[显示日志](../evidence/nx5-display/demo.log)、[连续帧记录](../evidence/nx5-display/pattern.log)、[负对照](../evidence/nx5-display/mixed-control.log)、[独占检查](../evidence/nx5-display/duplicate.log)、[最终设备状态](../evidence/nx5-display/final-device-state.json) 与 [构建验证](../evidence/nx5-display/validation.json) 可审查。本次结论是“约 29 FPS 双页运行期间，所采集样本未发现混色”，不扩大为覆盖全部物理 LCD 帧的证明。

下图来自真实合成输出的 WBC 亮度平面，因此显示为灰度；设备界面本身为彩色。

![实际合成输出](../evidence/nx5-display/demo.png)

## 双页与提交

`nx5` 在加载 SDK 前检查 ARM32、`/dev/fb0`、内核 `4.9.138 #233`、fyfb 名称及 SDK/fyfb/vou 文件 SHA256。只有这一固件配置已开放上屏。先获取进程锁，并检查 VO 未被其他程序占用，再启用 VO 和打开 framebuffer。

布局要求 1024×600、ARGB8888、stride 4096、虚拟高度 1200，以及完整两页容量。UI 在普通内存中的保留图像完成渲染，只将所需区域写入后台页。两页首次使用全量初始化；后续合并本帧与上次脏区。本阶段使用阻塞 `FBIOPUT_VSCREENINFO`，成功并读回目标偏移后才交换页角色；后续改用 `FBIOPAN_DISPLAY`，当前实现见 [NX5 模块](../src/backends/nx5/mod.rs)。失败时停止后续写页，没有单缓冲回退、提前覆写活动页或忽略提交失败的路径。

读取页号用于检测提交错误，并非单独证明扫描完成。NX5 使用本轮确认的驱动、映射和阻塞提交路径。通用 fbdev 及其他固件仍需要各自的提交实现与验证。

ARGB8888 写页改为对齐的 32 位存储，避免 ARM 对 framebuffer 的逐字节写入开销。同一界面写页均值从约 24.3 ms 降至 4.4 ms；Slint 绘制约 5.7 ms，阻塞翻页约 24 ms。界面由约 18 FPS 提升至约 29 FPS。对齐、非对齐、通道排列、RGB565、边界哨兵和页历史有回归覆盖。临时分段计时探针已移除，保留的 `SLINT_BRIDGE_TRACE=1` 仅输出每 150 次提交的帧率和翻页耗时。

## SDK 生命周期与本轮故障恢复

当前 `libVideoDec-1.6.6.so` 的加载构造函数会初始化全局 SYS/VO。第一次使用独立截图进程时，它重置了正在运行的显示。随后改成进程内采集，但沿用的采集清理函数 `FY_MPI_VO_CloseFd()` 又关闭了显示共用的 VO 句柄，导致翻页及恢复程序进入不可中断等待。

这两次诊断工具的生命周期错误已修复：采集插件只能在显示进程内加载，复用现有库；仅释放取得的 WBC 帧并关闭 WBC，不能关闭共享 VO 句柄。显示程序退出前等待观察线程结束。新观察器禁止编译为独立 SDK 可执行程序。独占锁和启用前的 VO 状态检查保护正常显示入口。

为解除已发生的驱动等待，本轮保存记录后重启过测试板一次。当前 boot ID 为 `938c34d4-456a-4b1a-b532-678c0f92efd0`；通过 DPower Terminal 恢复 DHCP 租约 `192.168.1.151` 和临时 ADB 服务。修复后重新运行了采集、退出、重启演示、连续帧和负对照测试。原故障记录保存在 [driver-stall.json](../evidence/nx5-display/driver-stall.json) 和 [external-capture-reset.log](../evidence/nx5-display/external-capture-reset.log)。

全像素逐字节读取未缓存 WBC 内存时，采集速率只有约 7.24 FPS；该试验保留为 [慢采集记录](../evidence/nx5-display/pattern-full-pixel-slow.log)，不纳入约 30 FPS 的验收样本。最终采样器使用每行五点，并校验采集时间戳速率，判读脚本会拒绝这个慢采集记录。

## 构建和运行

以下命令按当前目录整理；执行它们会构建当前源码，不会复现本报告当时的源码快照。观察器默认使用仓库内 SDK 头文件和动态库；日常部署及 WBC 插件的搜索路径见 [NX5 设备说明](../test-app/devices/nx5/README.md)。

```sh
cargo build --locked --offline --release --features nx5 \
  --target arm-unknown-linux-gnueabi -p slint-bridge-test-app --bin display

sh test-app/tools/nx5/display/build-observer.sh
```

正式显示不依赖诊断插件，Rust 程序在运行时加载固件原有 SDK。交叉编译的 release 程序 strip 后约 3.9 MiB。传输工具只复制临时文件，设备命令通过 DPower Terminal 的 USB1 root 会话执行。测试用 DejaVuSans 字体随附原许可证。

在确认没有其他显示所有者后运行正常演示：

```sh
cd /run/slint-bridge-display
SLINT_DEFAULT_FONT=$PWD/DejaVuSans.ttf SLINT_BRIDGE_TRACE=1 \
  LD_LIBRARY_PATH=/system/app/clib ./display --touch=/dev/input/event0
```

以下选项仅用于 `display` 示例，不是平台准入开关：`--seconds=20` 到时退出；`--pattern --observe=300 --seconds=20` 运行黑白采样；再加 `--split` 则运行应检出混色的负对照。UI 截图现使用 [Slint MCP 的 `take_screenshot`](slint-mcp.md)，旧 `--capture` 入口及插件中的单次截图导出已删除。MCP 截图不代表 WBC 合成输出，硬件采样仍由同进程 `--observe` 完成。不要同时启动这些实例，也不要在显示运行时执行链接此 SDK 的旧独立采集工具。

离线判读：

```sh
python3 test-app/tools/nx5/display/check-run.py evidence/nx5-display/demo.log
python3 test-app/tools/nx5/display/check-run.py evidence/nx5-display/pattern.log --wbc
```
