# NX5 TDE 写 framebuffer 与格式转换调查

> 本文保留 `35db94d` 调查阶段的实现与计时；后续 [NEON 反预乘优化](nx5-neon-unpremultiply.md) 改善了 CPU 转换，未将 TDE 写页接入生产路径。

2026-10-13，当前 #233 固件、同一 SDK、1024×600 BGRA framebuffer、alpha 量程 0–128。基线运行时为 `d34f436`，使用 CPU/TDE 共享 cached MMZ 画布；工程开始点为 `08c018f`，Slint 为 `22804ff`。原始档案在本地 `evidence/nx5-tde-writeback/2026-10-13/`，不随 Git 分发，需另行取得。

结论：本轮没有找到能在一次 TDE 拷贝中正确完成当前格式转换的路径。正确的“CPU 转换到 MMZ，再 TDE 拷贝到 framebuffer”实机像素通过，但比现有直接写页更慢。正常运行继续使用现有 CPU/NEON 转换写页，未改运行时或默认值。

## 单次 TDE 转换能力

源码依据是当前外部 SDK 的 `codec/mpi_tde.h` 和 `codec/include/fy_comm_tde.h`，未修改 SDK。`QuickCopy` 的接口约束要求相同尺寸和格式；`Bitblit` 提供两张输入 surface、格式、alpha 和预乘选项。参数存在不代表本固件实现了所有组合。

[格式探针](../test-app/tools/nx5/tde/probe-format.c) 使用 256×256 的独立 MMZ 图，覆盖全部 256 个 alpha、每个 alpha 下的 256 个输入强度，RGB 取不同值后按 Slint 规则预乘。目标逐字节与 CPU 精确反预乘、通道交换及 alpha 编码比较。它不写 framebuffer。

第二轮覆盖 ARGB8888、ABGR8888、RGBA8888、BGRA8888、RABG8888 五种源描述和两种目标 alpha 标志，包含 QuickCopy 同格式正对照、不同格式请求、单输入 Bitblit，以及预乘/像素 alpha/输出 alpha/混合因子选项。共 100 组，其中同格式正对照重复出现在每种格式组。完整选项含义在源码注释中；日志记录 SDK 拒绝与实际像素，不把调用返回成功当作格式转换正确。

| 请求 | 实际结果 |
| --- | --- |
| QuickCopy，同格式 ARGB8888 | 字节复制精确；`bAlphaMax255=false` 没有将 0–255 缩放到 0–128 |
| QuickCopy，ABGR8888 → ARGB8888 | 拒绝，`0xa064800b`（不支持的操作） |
| Bitblit，ABGR8888 → ARGB8888 | 各测试组合均拒绝，`0xa0648006`（无效参数） |
| Bitblit，其他可执行格式/选项 | 有通道重排或混合输出，但没有一组同时满足所需 RGB 与 alpha；部分路径将 alpha 写为 255 |
| 输入预乘开关 | 本轮观察到再次乘 alpha 的效果，没有得到所需的反预乘输出 |

ARM 小端下，Slint 的 RGBA 字节应由 ABGR8888 描述，framebuffer 的 BGRA 字节由 ARGB8888 描述。一次操作还必须将预乘 RGB 反预乘，并把 alpha 编码为 `(alpha+1)/2`。例如输入 RGBA 字节 `[108,18,46,127]` 的正确输出为 BGRA `[92,36,217,64]`；同格式 QuickCopy 保持原字节，颜色和透明度均不正确。设置目标 alpha 标志不能替代实际量程转换。

这些结论限定于所测 SDK、格式及选项，不据此声称所有 TDE 产品或其他固件都不支持转换。证据为 `format-01.log`、`format-02.log` 及对应探针源码副本。头文件未提供明确的输出反预乘开关；本轮没有尝试未经文档定义的 CSC 参数或改写驱动。

## CPU 转换加 TDE 拷贝的真实写页对照

[写页探针](../test-app/tools/nx5/tde/probe-writeback.c) 使用生产 `bridge_rgba_to_bgra` 转换代码。两条路径为：

1. 共享 MMZ 源 → CPU/NEON 转换 → framebuffer 后台页。
2. 同一源 → CPU/NEON 转换到缓存 MMZ → `MmzFlushCache` → `QuickCopy` → 同一 framebuffer 后台页。

第二条路径的 QuickCopy 只搬运已经正确编码的 BGRA 字节。探针独占 SDK，临时复用一个空闲源槽；不运行 Slint 图片缓存。它没有证明可以在生产渲染中无条件占用现有源槽，接入正常运行还需另外处理暂存区和缓存所有权。

目标物理地址从实际 `FBIOGET_FSCREENINFO.smem_start` 取得，本次为 `0x46019000`，stride 4096。每次只写后台页，TDE 使用阻塞 `EndJob` 等待完成，再按已核验 #233 的阻塞 PAN 完成契约提交。下一次写页发生在上一次 PAN 成功返回后；没有用延时或页号读回来替代退休条件。此独立 C 探针设置 `FB_ACTIVATE_VBL`；生产 Rust 后端保留设备原 activate 标志、只修改 yoffset，因此该探针不是生产提交代码的逐行复用。计时均排除 PAN。

每种图案预热 5 轮、测量 30 轮，两路径逐轮换序。每轮更新图案种子；计时含转换、缓存同步和实际同步拷贝，不含图案生成、页读回、翻页等待。编译器为现有 ARM GCC，Cortex-A7/NEON、`-O2`；两路径在同一个探针里使用同一转换函数。

| 图案 | CPU 直接写页 | 转换 MMZ + TDE 拷贝 | 增加耗时 |
| --- | ---: | ---: | ---: |
| 不透明渐变 | 6.605 ms | 11.393 ms | 4.788 ms，约 72% |
| 全透明 | 7.018 ms | 11.846 ms | 4.828 ms，约 69% |
| alpha=127 | 52.751 ms | 66.357 ms | 13.606 ms，约 26% |
| alpha 0–255 渐变 | 51.612 ms | 66.166 ms | 14.554 ms，约 28% |

混合路径中，缓存同步约 0.52–0.60 ms，TDE 提交/拷贝/等待约 4.24–4.30 ms。转换到 MMZ 本身没有更快；固定半透明直接写页有少数较高样本，其 p50 为 51.275 ms，混合路径 p50 为 66.392 ms，结论不依赖该组均值波动。不能只引用约 4.3 ms 的 TDE 拷贝时间来声称完整写页快于约 6.6 ms，因为源仍需先转换。

## 像素、透明度和资源核验

最终复验在每组最后一轮计时前先将后台页写入非零 `0xa5` 测试值，避免已有透明像素掩盖漏写。四种图案、两条路径的最后一轮均从实际目标页读回全幅，共 **4915200 像素完全一致**。RGB 用整数除法参考计算精确反预乘；alpha 用 `(a+1)/2` 比较，覆盖透明、不透明、固定半透明及全部 256 个 alpha。另逐字节确认源 MMZ 未变，TDE/CPU 写后台页前后当前显示页未变。

最终复验共 280 次实际页面呈现；上述读回检查针对每组最后一轮，不代表逐帧扫描输出采样。没有重做 WBC、非黑色视频合成、实体触摸时延或完整展示应用性能测量。

探针正常退出：VO 关闭，MMZ 回到 4900 KiB / 2 块，显示锁释放，内核窗口无新的 MMZ 强制回收。旧常驻界面按静态页、提交线程空闲后 SIGTERM 停止；旧进程的强制回收记录与探针正常退出分开保存。恢复常驻时，串口启动命令的结束标记被内核输出打断而超时；随后核对 `/proc` 和原生 MCP 确认新进程已正常启动，没有重复加载 SDK。

证据为 `writeback-final/remote/result.log`、`writeback-final.json`、`writeback-final/remote/kernel-window.log` 和退出 MMZ/VO 快照。仅新增诊断工具与文档，生产 Rust/C 运行时未改动；ARM 严格警告构建通过，未重复无关 Rust feature 矩阵。首轮相同规模的性能及像素对照另存于 `writeback/`，没有覆盖旧日志。常驻恢复、归档清理及本轮探针摘要见 `manifest.json`、`resident-final.json` 和当前设备快照。

收尾常驻仍为 `d34f436` 的原二进制，观测 PID 20127，关闭 trace、无退出定时器；RSS 14864 KiB，MMZ 12100 KiB / 5 块。最后 7 个测试/旧日志文件经 SHA256 归档后清理，当前与恢复程序及依赖保留；`/tmp` 剩余 24732 KiB，系统可用内存 44936 KiB。设备路径和 PID 属于该次观测，后续操作须重新核对。
