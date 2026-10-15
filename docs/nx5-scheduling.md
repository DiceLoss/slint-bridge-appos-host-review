# NX5 并行调度优化

> 等待精度优化阶段的历史测量，使用当时至少 33334 微秒的 NX5 帧间隔；该钳制后来已移除。现行配置与等待行为见 [平台说明](software-platform.md)，原始证据为不随 Git 分发的本地档案。 文中的 `NX5_SDK_INCLUDE` 外部路径保留采集环境；当前构建默认使用仓库资源，配置方法见 [使用指南](getting-started.md)。

未启用 Cortex-A7/NEON 编译预设的 NX5-TDE 构建，已从上一轮约 **25.51 FPS** 提升到稳态 **29.00 FPS**。本轮相同场景复测旧版为 25.74–25.89 FPS；新版本 90 秒测试在启动窗口之后的 16 个统计窗口均为 29.00 FPS，TDE 错误为 0。

改动集中在事件等待，双 framebuffer、页面复用条件、TDE 与软件回退代码、示例场景和编译参数均未改变。当前仍受约 58 Hz、两个刷新周期完成一次翻页的约 29 FPS 上限约束。

## 瓶颈

当前 #233 内核的 `/proc/timer_list` 明确报告 **10,000,000 ns** 定时器分辨率。读取单调时钟可以获得更细的时间值，但阻塞定时等待仍落在 10 ms 节拍上。

独立 Linux 探针不加载 SDK、不访问显示，测量得到：

| 等待方式 | 请求 33.334 ms 时的实际等待中位数 |
| --- | ---: |
| `poll` | 39.925 ms |
| `ppoll` | 39.916 ms |
| `nanosleep` | 39.927 ms |
| `timerfd` | 39.925 ms |
| 阻塞等待后检查最后期限 | 约 33.336 ms |

因此只换用 `timerfd` 或纳秒参数的接口不能消除量化。原始记录见 [内核精度](../evidence/nx5-scheduling/timer-resolution.txt)、[四种等待接口](../evidence/nx5-scheduling/wait-probe.log)、[改进等待](../evidence/nx5-scheduling/wait-hybrid.log)。

上一轮的较慢构建时间线中，渲染启动间隔平均 **39.16 ms**，多数为 40 ms；489 个完成帧间隔中，132 个落到约 51.72 ms，即三个刷新周期。画布准备好且页面可用后的额外提交延迟平均只有 **0.189 ms**，主要损失发生在下一帧启动前。

`frame_interval` 从实际渲染启动时刻计算，Slint 重复定时器也从实际激活时间重新安排。粗粒度唤醒使延迟反复进入后续周期。本次保留最小渲染启动间隔的原有含义，通过减少唤醒迟到解决问题，不补发过期帧、不重复提交未变化的画面，也不放宽翻页完成等待。

修复后较慢构建的稳态 PAN 平均等待约 30.8 ms，比原来的约 25.8 ms 更长。这与帧率提升不矛盾：下一帧提前绘制完成，页面可用后能更早提交，绘制与等待充分重叠；实际完成帧间隔稳定在约 34.48 ms。

## 实现与适配分工

- [src/event_wait.rs](../src/event_wait.rs) 是通用 Linux 等待实现：根据单调时钟期限，先用 `ppoll` 阻塞等待至最后一个定时器节拍之前，再用零超时 `ppoll` 检查期限和事件。
- 最后的检查阶段是短时忙等，输入或提交完成事件仍可立即结束等待。是否无限期阻塞取决于事件循环算出的等待期限；当前实现会合并定时器与可绘制的动画／重绘期限，任务积压时使用零超时。只有无期限且无待执行任务时才无限期阻塞，见 [当前事件循环](software-platform.md#事件循环)。精度参数为零时不进入忙等尾段。
- [src/backends/nx5/mod.rs](../src/backends/nx5/mod.rs) 为已验证的 #233 固件提供 10 ms 参数，经 `backends::Admission` 传给事件循环；通用层没有 NX5 寄存器、驱动常量或专用 ioctl。
- `PresentationContract` 继续只描述布局与提交完成条件。定时器到期不代表旧页退休；后台 PAN 返回成功后才允许复用页面。

没有新增公开配置项，也没有修改内核、显示时钟、线程优先级或 CPU 亲和性。`Config::frame_interval` 仍然是最小渲染启动间隔；本轮测量时 NX5 至少为 33334 微秒，后续已移除该钳制，见 [MCP 接入与限帧移除](slint-mcp.md)。

## CPU 成本

30 秒对照测试取第 5–20 秒的进程 CPU 时间，包含 UI 与提交线程；`CLK_TCK=100`。

| 口径 | 修改前 | 修改后 |
| --- | ---: | ---: |
| 一个 CPU 核心为 100% | 44.69% | 50.56% |
| 换算为双核总算力占比 | 22.34% | 25.28% |

增加量约为单核口径 **5.88 个百分点**，包括更多完成帧和精确等待的成本。这是进程 CPU 时间测量，不是整机功耗测量。较短的定时期限可能主要靠忙等完成，当前方案用 CPU 时间换取这套低精度内核上的调度精度。

原始采样和计算见 [修改前](../evidence/nx5-scheduling/cpu-before.log)、[修改后](../evidence/nx5-scheduling/cpu-after.log)、[CPU 对照](../evidence/nx5-scheduling/cpu.json)。

## 验证

先建立了可失败的回归：模拟 10 ms 内核节拍时，原等待将 1 微秒请求拖到 10 ms，测试失败；修改后覆盖不同起始相位及 1 微秒至 100 ms 的期限，通过且不提前返回。

另外验证了阻塞阶段和忙等阶段都能被事件打断、空闲与默认策略不连续轮询、长超时边界与系统错误。主机测试 `fbdev` 23 个、`nx5` 23 个、`nx5-tde` 26 个通过；特性组合、严格 Clippy 和格式检查通过。原有双页保护、异步错误、SDK 生命周期与软件回退回归继续通过。

较慢构建和优化构建的 90 秒测试，各有 17 个完整统计窗口；首个窗口分别为 28.34、28.43 FPS，其余 16 个窗口均为 29.00 FPS，TDE 错误均为 0。见 [较慢构建](../evidence/nx5-scheduling/generic-stability.log) 与 [优化构建](../evidence/nx5-scheduling/optimized-stability.log)。300 帧 WBC 样本未检出混色，混色负对照检出 30/30。WBC 仍只覆盖约 29 FPS 的合成输出及每行五个采样点，不扩大为全部物理帧的认证。

源码摘要对照确认 `framebuffer.rs`、`presentation.rs`、TDE、软件回退及示例代码未变。ARM 构建仍采用 soft-float ABI，最高 glibc 依赖为 2.18。

```sh
# 较慢构建，刻意不加载板级 CPU 编译预设：
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
cargo build --locked --offline --release --features nx5-tde \
  --target arm-unknown-linux-gnueabi -p slint-bridge-test-app --bin display

python3 test-app/tools/nx5/scheduling/check.py evidence/nx5-scheduling
```

完整结果与文件摘要见 [验证清单](../evidence/nx5-scheduling/validation.json)，当前运行进程见 [环境记录](../test-app/devices/nx5/current-target.json)。
