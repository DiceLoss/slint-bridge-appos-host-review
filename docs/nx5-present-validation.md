# NX5 显示提交与旧缓冲区复用验证

> 历史报告：以下对象为内核 #22、192.168.1.152 的旧环境。后续用户已采用 30 FPS 双页实测口径推进当前 #233 固件上屏；当前状态见 [上屏验证](nx5-display.md)。本报告保留当时数据与验收结论。

2026 年 10 月 10 日，在当前 NX5 固件上，阻塞 `FBIOPUT_VSCREENINFO` 返回后的立即旧页覆写，通过了有限覆盖的合成帧观测。**结论为未通过无撕裂准入**：回写通道约 30 FPS，不能覆盖 60 Hz LCD 的全部物理帧，也没有证明最后一次 DMA 读取结束的驱动契约。用户已确认暂时没有匹配驱动源码、完成语义文档或外部采集设备，保留该状态。

## 实机与依赖

| 项目 | 本次验证对象 |
| --- | --- |
| 型号 | NX5，双核 Cortex A7，ARM EABI5 soft float |
| 内核 | Linux 4.9.138，构建编号 22 |
| 显示驱动 | `fyfb`，VOU v0.5.8，2025 年 3 月 13 日构建 |
| 显示 | 1024 × 600，32 bpp，stride 4096，LCD 60 Hz |
| framebuffer | 4915200 字节，两页；页地址 `0x44819000`、`0x44a71000` |
| 当前应用 | `/run/slint-demo/slint-demo`，PID 1158 |
| SDK 库 | `libVideoDec.so`，SHA256 `8064b9d16c7c88b6b6d40c66e40b754e392c36ab03803cb1f04c6ed0a331bd82` |

完整身份、启动 ID 与驱动符号保存在 [device.txt](../evidence/nx5-present/device.txt)、[driver-symbols.txt](../evidence/nx5-present/driver-symbols.txt)。命令、工具链及源码摘要见 [run-manifest.json](../evidence/nx5-present/run-manifest.json)。结论只适用于这些实测条件，不能推广到其他 BSP 或显示驱动。

## 观测结果

| 测试 | 翻页次数 | 捕获帧数 | 异常帧 | 结果 |
| --- | ---: | ---: | ---: | --- |
| 不启用 WBC 的寄存器采样 | 128 | 不适用 | 0 次返回后地址或更新位检查失败 | 观测通过 |
| 返回后立即覆写旧页，空闲条件 | 64 | 85 | 0 | 有限观测通过 |
| 返回后立即覆写旧页，两个 `yes` 进程施加 CPU 压力 | 64 | 85 | 0 | 有限观测通过 |
| 故意提前覆写活动页的负对照 | 12 | 21 | 11 | 正确检出错误 |

另外完成白屏、上下黑白分区的采集校验，以及最终恢复逻辑的 8 次翻页短测。最终短测捕获 16 帧，未检出异常，恢复成功。

正式两组旧页复用测试，在翻页函数返回约 **1 微秒**后开始将旧页填为灰色，没有附加等待、页号查询或额外 vsync。每个输出帧本应为全黑或全白；灰色像素或同一帧内的黑白混合被判为异常。捕获检查每一行的五个横向位置，共 3000 个亮度采样点；这不是全像素检查。

未启用 WBC 的 128 次翻页，ioctl 平均约 **24.742 ms**，范围 **16.973–33.251 ms**。这测量的是翻页调用耗时，不是应用 FPS。中断计数差为 2 或 3，计数读取本身受调度影响，不能直接作为精确扫描完成事件。

结构化结果见 [observations.json](../evidence/nx5-present/observations.json)。完整正常、压力及负对照日志分别为 [reuse-idle.log](../evidence/nx5-present/device-run/reuse-idle.log)、[reuse-cpu.log](../evidence/nx5-present/device-run/reuse-cpu.log)、[early-control.log](../evidence/nx5-present/device-run/early-control.log)。

## 地址变化早于硬件更新完成

本地《NX5 Processor User Guide》描述了 DHD 寄存器：

| 寄存器 | 文档地址或字段 | 语义 |
| --- | --- | --- |
| `UPDATE_CTRL` | `0x34101008`，bit 5 | OSD 更新请求，硬件 shadow 后清零 |
| `CHN_WORK_CTRL` | `0x3410100c` | 自动或强制更新，以及整体或部分更新模式 |
| `CHN_INT_STS` | `0x34101010` | 更新完成、帧开始、帧结束等状态 |
| `OSD_BASE_ADDR` | `0x34101134` | 配置的图形 DRAM 起始地址 |

只读采样显示：目标地址先进入 `OSD_BASE_ADDR`，同时更新请求位为 1；约一帧后请求位清零。每次阻塞 ioctl 返回后的立即读数均为目标地址且 bit 5 已清零。采样线程可能晚于主线程读到清零，因此不将其采样时间当作精确硬件锁存时刻。

这排除了“地址寄存器已经变化就可以复用旧页”的判断方式，也支持保留完整阻塞提交。但文档中的 shadow 完成不等于最后一次旧页 DMA 读取结束；不能仅凭该字段给生产路径放行。

本次工具只读文档定义的 MMIO 字段，不清中断、不改寄存器、不修改内核。参考源位置为 `/home/dpower/data/GitHub/sop-company/departments/cto/knowledge/corpus/NX5_Processor_User_Guide.md`，第 2975–2988、3227–3228 行；原始文档在同目录上一级的 `sources/NX5_Processor_User_Guide.doc`。

## WBC 观测范围

使用 SDK 的设备回写通道 WBC，设置为 1024 × 600 YUV420、请求 60 FPS、深度 2。硬件 `WBC_CFG=0x7` 对应合成后的 YUV 输出。捕获的白屏为 255，黑白分区上下分别为 255、0，证明采集器能响应图形内容并识别同帧混合。

实测捕获间隔约 **33.246 ms**，约 **30.08 FPS**；驱动状态显示实际速率约 31 FPS。它没有覆盖全部 60 Hz 输出帧。WBC 还会增加内存带宽并参与显示流水线，启用时的成功观测不能自动推广到关闭 WBC 的生产路径。捕获位于合成输出阶段，也不等同于 LCD 面板或 RGB 链路的直接采集。

因此本次能确认“已捕获的样本未出现旧页污染”，不能确认“所有物理帧绝不撕裂”。负对照检出 11 个异常帧说明检测器具备检错能力，并不能消除未捕获帧的盲区。

## 状态恢复

每轮测试暂时停止原 UI 的全部线程，保存两页校验值，完成后恢复原页号及像素，再恢复同一 PID。原应用二进制和启动配置没有修改。恢复证据保存在各轮的 `*.recovery.txt`、`*.pixels-before.sha256`、`*.pixels-after.sha256` 与 `*.owner-after.txt`。

最终页号为 `0,0`，PID 1158 正常存活。WBC 已关闭，其分配的 MMZ 内存已释放：MMZ 使用量恢复为 4900 KB、两个块。WBC 的闲置配置仍缓存本次尺寸与深度，但不存在活动回写任务。

若未来探针卡在内核或无法确认恢复，控制脚本会保留 UI 暂停状态，不启动竞争的提交者。发生不确定翻页失败时，像素测试工具停止写页，并尝试将原两页保存到本轮的 `*.backup.bgra`。此时不能盲目恢复应用或直接将备份写入活动页，应先核实驱动和显示状态。

## 重跑方式

主机交叉编译：

```sh
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
NX5_SDK_LIB=/home/dpower/data/setting-nx5/vendor/slint-nx5/crates/libs \
sh test-app/tools/nx5/present/build.sh
```

`NX5_CC` 可指向匹配设备 glibc 2.21 的交叉编译器。探针不链接 Slint，不据此宣称 Slint 平台已完成。输出位于 `target/nx5-present/`。

通过 ADB 将 `probe`、`pattern`、`retire`、`wbc-query`、`wbc-capture` 和 `run-device.sh` 放在同一个设备临时目录。核对设备身份、当前 UI PID 及可恢复条件后，使用 `usbtty1` 的 root shell 运行。脚本当前只接受 `/run/slint-demo/slint-demo` 作为 UI 所有者，不会自动挑选或终止其他进程。

```sh
# 每次使用新的 run 名称，脚本拒绝覆盖已有日志。
sh run-device.sh 1158 128 register-phases-new
sh run-device.sh 1158 64 reuse-idle-new retire
NX5_CPU_STRESS=2 sh run-device.sh 1158 64 reuse-cpu-new retire
sh run-device.sh 1158 12 early-control-new early-reuse
```

最后一条故意在测试图案阶段覆写活动页，预期检出错误并退出 1；恢复仍应成功。不要将它用于正常产品运行。

保存的标准证据集可离线重放：

```sh
python3 test-app/tools/nx5/present/test_analyze.py
python3 test-app/tools/nx5/present/analyze.py evidence/nx5-present/device-run
python3 test-app/tools/nx5/present/analyze.py evidence/nx5-present/device-run --require-admission
```

判读器检查样本完整性、像素分类、负对照、清理和恢复。六个回归测试包括丢样本、截断日志、更新未完成却读回匹配页号，以及不得从这些记录获得硬件准入。当前 `--require-admission` 返回 2 是预期结果。

## 下一步准入条件

需要补足至少一种能证明提交和旧页退休语义的可靠证据：匹配固件的 `fyfb/vou` 源码或厂商契约，明确更新锁存边界和最后一次 DMA 读取结束；或者覆盖物理帧的可靠显示链路采集，并将旧页覆写、随机提交相位、压力场景和真实输出关联起来。单张 framebuffer 截图、肉眼观察或增加一次固定睡眠不足以补齐该证明。

在此之前，NX5 正式后端保持未通过准入，不增加跳过验证的产品开关，也不采用可能撕裂的单缓冲回退。
