# NX5 整屏帧率优化

> Cortex-A7/NEON 和精确脏区优化阶段的历史测量。后续还加入后台提交、等待精度优化并移除专用限帧；当前流程见 [平台说明](software-platform.md)。原始证据为不随 Git 分发的本地档案。 文中的 `NX5_SDK_INCLUDE` 外部路径保留采集环境；当前构建默认使用仓库资源，配置方法见 [使用指南](getting-started.md)。

同一 1024×600 纹理混合场景从约 19.5 FPS 提升至稳定约 **29.0 FPS**，提升约 49%。当前阻塞翻页路径在完全不绘制、不复制像素时，也只有 **29.003 FPS**，因此此版本达到该路径的实测节拍，尚未严格达到 30 FPS。

90 秒持续测试记录了 17 个完整的 150 帧统计窗口：启动窗口 28.43 FPS，其后 16 个窗口均为 29.00 FPS，TDE 错误为 0。随后常驻演示的采样范围为 28.9–29.0 FPS。见 [持续测试](../evidence/nx5-fps/stability.log)、[最终运行](../evidence/nx5-fps/final-live.log) 和 [结构化结果](../evidence/nx5-fps/results.json)。

## 瓶颈与改动

原实现串行执行绘制、写页和阻塞翻页，约 51.3 ms 才完成一帧。Slint 更新与调度开销约 0.13 ms，并非主要瓶颈。

| 阶段 | 优化前平均 | 优化后平均 |
| --- | ---: | ---: |
| Slint/TDE 绘制 | 15.58 ms | 11.95 ms |
| 转换并写入后台页 | 4.91 ms | 1.96 ms |
| 阻塞翻页 | 30.48 ms | 20.20 ms |
| 整帧间隔 | 51.34 ms | 34.48 ms |

以上阶段数据来自临时计时版本，排除前 20 帧。最终构建已移除计时探针。优化后三项改动为：

1. **NX5 编译预设**：Rust 按 Cortex-A7 优化，C 接口按 ARMv7-A、Cortex-A7、NEON/VFPv4 编译，保留与设备兼容的 soft-float 调用 ABI。仅指定这个板级预设时启用，通用构建不改变 CPU 基线。
2. **精确脏区合并**：保留当前与前一次提交的各个变化区域，逐行合并相交区间。避免把相隔较远的控件包进同一个大矩形，平均写入量由约 30 万像素降到约 15.9 万。
3. **NEON 像素转换**：RGBA→BGRA 每次处理 16 个像素，并强制输出 alpha 为 255。保留非 NEON 构建的原有写页实现，尾部像素由标量转换处理。

准备时间超过约 16.5 ms 的帧，原先会落到约 51.7 ms 的完成间隔。优化后准备时间缩短，测得的帧间隔保持在约 34.48 ms。本轮仍在 UI 线程阻塞翻页；后续 [并行化](nx5-parallel.md) 将阻塞调用移至通用提交线程。两个版本都等待驱动确认后才复用旧页。

阶段对照可重放：

```sh
python3 test-app/tools/nx5/fps/analyze-profile.py evidence/nx5-fps/profile-sync.log
python3 test-app/tools/nx5/fps/analyze-profile.py evidence/nx5-fps/profile-neon.log
```

## 当前翻页上限

独占当前 NX5 后，连续交替提交两张已有 framebuffer 页，不写任何像素。排除 8 次预热，120 次提交平均耗时 **34.4797 ms**，范围 **34.455–34.507 ms**，平均吞吐为 **29.003 FPS**。该数据测量应用看到的阻塞提交节拍，不是面板物理刷新率。

具体记录见 [纯翻页测量](../evidence/nx5-fps/flip-cadence.log)。后续已确认根因：所需 51.2064 MHz 被驱动选为 49.5 MHz，实际输出约 58 Hz；阻塞提交占两个刷新周期。相位扫描与 BSP 修改位置见 [阻塞翻页分析](nx5-flip.md)。当前保留已采用的双 framebuffer 完成契约。

## 正确性与回退

两页首次使用仍全量初始化；后续只修复本帧和前一已提交帧的区域并集。当前页不写入，提交失败后停止后续写页。

新增回归在两个相隔很远的更新区域之间设置写入哨兵：原实现会覆盖哨兵，优化后保持不变，并确认上一帧区域在交替页上正确补齐。实机复跑实际界面 100 帧，两页逐像素与独立的软件全量参考比较，最大通道差仍为 1/255；80 帧增量重绘、RGBA/预乘纹理、裁剪、缓存更新和软件回退测试通过。

NEON 转换在设备上覆盖 0–65 像素长度、0–3 字节偏移及边界哨兵。主机 ASan/UBSan 与六种 SDK 边界错误注入继续通过。不支持的操作和可恢复错误仍走软件，不确定完成状态仍禁止提交整帧。

证据见 [像素对比](../evidence/nx5-fps/pages.log)、[渲染与回退](../evidence/nx5-fps/render-compare.log)、[NEON 边界测试](../evidence/nx5-fps/native-neon-test.log)、[脏区回归](../evidence/nx5-fps/sparse.log)。

主机测试通过：`fbdev` 14 个、`nx5` 14 个、`nx5-tde` 17 个；另有按设备单独运行的忽略测试。严格 Clippy、格式检查和 ARM release 构建通过。设备退出测试后 MMZ 使用量恢复至 4900 KiB；运行优化演示时为 12100 KiB，RSS 约 9840 KiB。

## 构建和运行

从仓库根目录构建当前 NX5 优化版本：

```sh
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
cargo --config .cargo/nx5.toml build --locked --offline --release \
  --features nx5-tde --target arm-unknown-linux-gnueabi -p slint-bridge-test-app --bin display
```

板级预设为 [.cargo/nx5.toml](../.cargo/nx5.toml)。应用在自己的 workspace 构建时，也需显式加载该预设或采用相同的 CPU 编译参数，才能获得本报告中的性能；只设置 Cargo feature 不会自动启用依赖库的 CPU 优化。

设备上使用相同 API 和示例参数。本轮将构建输出部署并命名为 `display-optimized`，持续性能测试为 `display-optimized --textures --seconds=90`，使用 `SLINT_BRIDGE_TRACE=1` 记录实际完成帧数和 TDE 计数。日常部署命令见 [NX5 设备说明](../test-app/devices/nx5/README.md)。

最终证据重放：

```sh
python3 test-app/tools/nx5/fps/check.py evidence/nx5-fps
```

完整构建、ABI、帧率及文件摘要见 [验证清单](../evidence/nx5-fps/validation.json)，当前设备上的运行路径和 PID 见 [环境记录](../test-app/devices/nx5/current-target.json)。
