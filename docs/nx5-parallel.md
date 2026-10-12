# 渲染与双页提交并行化

> 首次并行化阶段的测量记录。下方模块路径与职责按当前实现维护；测量时的限帧与测试数量保留历史口径。现行行为见 [平台说明](software-platform.md)，原始证据为不随 Git 分发的本地档案。

参考 `slint-nx5` 的 `flip_worker.rs` 与页历史处理方式，将阻塞翻页移到后台线程。UI 线程可以在上一帧等待完成时，将下一帧绘制到原有的保留画布中。仍然只有两张 framebuffer；最多一笔在途提交、一帧待提交画布。

后续已定位较慢构建约 25.51 FPS 的调度瓶颈，并提升到稳态 29.00 FPS，见 [调度优化](nx5-scheduling.md)。本文保留首次并行化的测量结果。

## 通用层与 NX5 的分工

| 模块 | 职责 |
| --- | --- |
| [src/framebuffer.rs](../src/framebuffer.rs) | 通用布局、位域转换、两页首次初始化、脏区合并、已提交与待提交页状态；只向已获准复用的页面写入 |
| [src/presentation.rs](../src/presentation.rs) | 一笔在途任务的线程与完成通知、错误保持、退出等待；不包含 NX5 ioctl、寄存器或 SDK 调用 |
| [src/runtime.rs](../src/runtime.rs) | 在 UI 线程运行 Slint、输入与定时器；分开安排绘制与提交，等待时继续处理事件 |
| [src/backends/mod.rs](../src/backends/mod.rs) | 私有的板级提交契约及布局检查接口，分派已支持的板级实现 |
| [src/backends/nx5/mod.rs](../src/backends/nx5/mod.rs) | ARM32 与显示所有权检查、SDK/VO 会话、布局校验、阻塞 PAN 与结果读回；不核对固件版本或文件摘要 |

原来通用 framebuffer 中直接调用 NX5 布局检查的位置，已改为调用板级契约提供的检查函数。通用工作线程只接收页偏移，调用板级阻塞提交回调；只有收到成功完成后，通用层才更新前台页和脏区历史。

`fbdev` feature 的主机内存显示测试使用同一套并行代码。真实通用 fbdev 仍须提供经确认的驱动完成契约；本次没有把 NX5 的等待语义推广到未知设备，也没有增加公开的准入绕过接口。

## 执行顺序与资源生命周期

1. UI 绘制帧 N，转换并写入可用后台页，交给提交线程。
2. 提交线程执行完整的板级阻塞调用。UI 可以处理输入、定时器，并绘制帧 N+1 到保留画布。
3. 两张 framebuffer 在提交完成前都不再写入。完成事件唤醒 UI，确认成功后才交换页角色。
4. 帧 N+1 的写页会合并自身与刚完成帧 N 的脏区；只提交一次，不累积无界队列。

`Config::frame_interval` 表示最小渲染启动间隔。本轮测量时 NX5 将其钳制为至少 33334 微秒；[后续已移除此钳制](slint-mcp.md)，当前默认值为 16667 微秒。是否实际发生渲染重叠，取决于重绘时间、渲染耗时及帧间隔。并行化不会改变当前 58 Hz、双周期翻页的约 29 FPS 上限。

完成事件与业务任务共用 `eventfd` 唤醒。即使窗口静止、没有下一次绘制，提交失败或线程异常也会唤醒事件循环并报错；此后禁止写页与再次提交。退出事件循环时等待最后一笔提交，析构时先关闭请求通道并等待线程退出，再释放渲染资源和映射。

板级契约由主线程与工作线程共同持有。工作线程异常退出时，不会提前卸载 SDK；保留画布及 TDE 资源先于最终板级会话释放。Slint 和 TDE 绘制仍留在 UI 线程；TDE 作业继续同步等待完成。

## 实测

同一 NX5 #233、同一启动，1024×600 纹理混合示例：

| 构建方式 | 改动前串行 | 并行 |
| --- | ---: | ---: |
| Cortex-A7/NEON 优化构建 | 约 29.00 FPS | 约 29.00 FPS |
| 未启用板级 CPU 优化的 ARM 构建，仍使用 NX5-TDE 后端 | 21.17 FPS | 25.51 FPS |

第二组是相同编译参数、相同示例、修改前源码快照与并行实现的对照，样本约 20 秒。并行版本包含临时计时探针，稳态提升约 20.5%。它不代表通用 fbdev 已在设备上开放。

较慢构建的 490 个预热后绘制样本中，384 帧与翻页等待重叠，平均每个绘制样本重叠 **6.394 ms**；渲染均值 15.675 ms，写页均值 3.670 ms。**所有 framebuffer 写页与在途翻页的重叠次数为 0**。优化构建已经接近驱动上限，重叠时间很小，主要收益是 UI 线程不再因 PAN 而停住。

计时见 [优化构建](../evidence/nx5-parallel/profile.json)、[较慢构建](../evidence/nx5-parallel/generic-profile.json)、[串行对照](../evidence/nx5-parallel/generic-before.log)。临时计时探针已从正式源码移除。

最终构建另外完成 90 秒纹理场景测试；具体帧率窗口和错误计数见 [持续运行](../evidence/nx5-parallel/stability.log)。连续 WBC 采样 300 帧，黑白各 150，混色为 0；混色负对照检出 30/30。WBC 仍只覆盖约 29 FPS 的合成输出、每行五个采样点，不扩大为全部物理 LCD 帧的认证。

## 正确性与回退

- 主机测试：`fbdev` 19 个、`nx5` 19 个、`nx5-tde` 22 个通过；设备专用忽略测试另行运行。特性互斥、严格 Clippy 和格式检查通过。
- 阻塞替身配合真实 Slint：下一帧绘制能先完成，两张显示页保持不变；完成后交替页历史修复正确。
- 异步失败：完成通知唤醒空闲循环，已准备画布不再写入显示页，后续调用继续返回错误。
- 真实事件循环：翻页被阻塞时，跨线程业务回调和 UI 定时器仍能执行。
- 生命周期：退出等待正在执行的提交；工作线程异常不会提前释放板级会话。
- 实机：100 帧双页输出与独立软件全量参考比较，最大通道差 1/255；80 帧增量重绘及各项软件回退通过。

实机双页像素比较使用内存文件承载两页，TDE 使用真实硬件；实际显示路径由上述 WBC 测试单独验证。证据为 [像素对比](../evidence/nx5-parallel/pages.log)、[渲染与回退](../evidence/nx5-parallel/render-compare.log)、[正常采样](../evidence/nx5-parallel/pattern.log)、[负对照](../evidence/nx5-parallel/mixed-control.log)。

## 复跑与构建

```sh
python3 test-app/tools/check-feature-matrix.py
python3 test-app/tools/nx5/parallel/analyze-profile.py evidence/nx5-parallel/profile.log
python3 test-app/tools/nx5/parallel/analyze-profile.py evidence/nx5-parallel/generic-profile.log --require-overlap-ms 1
python3 test-app/tools/nx5/parallel/check.py evidence/nx5-parallel

NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
cargo --config .cargo/nx5.toml build --locked --offline --release \
  --features nx5-tde --target arm-unknown-linux-gnueabi -p slint-bridge-test-app --bin display
```

当前运行路径与进程见 [环境记录](../test-app/devices/nx5/current-target.json)，构建和证据摘要见 [验证清单](../evidence/nx5-parallel/validation.json)。
