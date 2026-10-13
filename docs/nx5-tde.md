# NX5 TDE 接入与实机验证

> 性能表和测试计数是 TDE 初次接入阶段的记录。后续整屏优化达到约 29 FPS，NX5 专用限帧也已移除；现行运行行为见 [平台说明](software-platform.md)。现行绘制范围见本文对应小节及平台说明，构建命令与当前实现一致，原始证据为不随 Git 分发的本地档案。

`nx5-tde` 已接入当前 NX5 #233 固件，并通过真实硬件绘制、软件参考图像、缓存更新、双页输出和软件回退验证。该 feature 包含 `nx5`；应用仍使用相同的 `init(Config)` 和 Slint 组件 API。

提交 `2cd98fa` 的 [前一轮回归](nx5-regression.md) 发现完整进程退出时三块 TDE 分配仍由内核强制回收。现已修复平台与窗口的引用环及错误路径的可见组件引用；[最新退出回归](nx5-tde-exit.md) 的正常/EIO 退出均无 TDE 泄漏回收，三项硬件测试再次通过。未知 DMA 完成时保留资源的安全策略不变。 `949ceb9` 后续又通过 [窗口启动实机回归](nx5-startup.md)，覆盖首次显示失败和重试等启动路径；前述三项硬件比较没有在该轮重跑。 这三项测试现已在 [2026-10-12 补测](nx5-hardware.md) 中逐项通过；下方早期记录仍保留原始测量口径。

## 测量结果

在当前设备上渲染 512×384 图像场景，预热 5 帧后分别测量 60 帧。两条路径使用相同的预乘 RGBA 目标和真实 Slint renderer。结果包含软件绘制、TDE 提交、缓存同步及 MMZ/CPU 拷贝，排除 framebuffer 翻页。测试期间图像缓存命中，因此不代表每帧更换全新图片的性能。

| 操作 | 软件平均 | TDE 平均 | 加速比 |
| --- | ---: | ---: | ---: |
| 不透明纹理 | 16.606 ms | 6.550 ms | 2.54× |
| 普通 RGBA 透明纹理 | 35.252 ms | 13.192 ms | 2.67× |
| 预乘 RGBA 透明纹理 | 22.865 ms | 13.087 ms | 1.75× |

纹理、文字和移动图形混合的 1024×600 上屏示例，软件版约 19.2–19.4 FPS，TDE 版约 19.2–19.5 FPS。本阶段的整屏帧率接近，翻页等待占较多时间；上述倍率表示纹理场景的渲染收益。测量时平台仍有限制至多 30 FPS 的钳制，后续已移除，不能据此描述当前实现。

原始数据见 [硬件对比](../evidence/nx5-tde/compare-final.log)、[软件上屏](../evidence/nx5-tde/screen-software.log)、[TDE 上屏](../evidence/nx5-tde/screen-tde.log) 和 [结构化结果](../evidence/nx5-tde/results.json)。最近一次设备观测见带时间戳的 [环境记录](../test-app/devices/nx5/current-target.json)。

最终运行版本的 [日志](../evidence/nx5-tde/final-live.log) 中，1650 帧累计完成 1650 次复制、1482 次混合、3130 次缓存命中，TDE 错误为 0。下图为实机 WBC 亮度输出，设备画面本身为彩色。

![TDE 实机合成输出](../evidence/nx5-tde/final-live.png)

## 绘制范围与精度

纹理加速入口是本地 Slint 1.18.1 的 `TargetPixelBuffer::draw_texture()`，支持 RGBA 和预乘 RGBA 的等尺寸纹理。当前策略仅提交不少于 32768 个有效像素、最多 16 个互不重叠裁剪矩形的操作。源宽需为 4 的倍数，源高需为偶数，且源尺寸不能超过目标画布。复制使用 `FY_TDE2_QuickCopy`；透明混合使用 `FY_TDE2_Bitblit`，混合的坐标和尺寸需为偶数，目标像素需不透明。

整体透明度现已支持：准备源像素时合并 opacity，并把 opacity 纳入两槽缓存键；不直接使用 SDK 的全局 alpha 开关。缩放、旋转、着色、平铺、其他源格式、小图和不满足硬件限制的裁剪返回软件 renderer 处理。字体、路径、圆角/透明矩形继续使用软件实现，不扩大裁剪区域。

`TargetPixelBuffer::fill_background()` / `draw_rectangle()` 已连接 `FY_TDE2_QuickFill`，正常运行保留更快的 CPU 填充，可用 `SLINT_BRIDGE_TDE_FILL=1` 进行显式硬件对照。适用区域、默认策略和精度以 [平台说明](software-platform.md) 为准；本轮像素和耗时见 [2026-10-13 填充/混合报告](nx5-fill-blend.md)。

实测不透明复制、源图写时复制更新和软件回退均与参考图像逐字节一致。单次透明纹理混合的 RGB 最大通道差为 **1/255**，alpha 与软件一致。硬件与软件的混合取整并非逐字节相同，连续叠层可能积累量化误差。

普通 RGBA 在上传缓存时按 Slint 的 `/255` 截断规则预乘，避免 SDK 再做一次不同取整的预乘。混合只接受不透明目标，并将结果 alpha 保持为 255。透明目标交回软件。已验证 alpha 边界值 0、1、63、127、128、191、254、255、负坐标裁剪、非连续源 stride、奇数裁剪回退和源缓存失效。

## 缓冲与错误处理

CPU 中的保留 RGBA 图像是最终图像。硬件拥有独立的 cached MMZ 目标和两个源槽位，不直接读写 Slint 的 CPU 切片或扫描输出页。只有同步 `EndJob` 成功且完成缓存同步后，才将结果区域拷回 CPU 图像，再走原有双 framebuffer 输出路径。

源缓存使用本地 Slint 扩展 `TextureCacheKey` 保留不可变图像身份，包含源视图。缓存键同时保留源图像存储，写时复制会产生新身份。两个源槽位按最近使用次序替换，命中时也刷新替换位置，避免新前景驱逐刚使用过的固定背景；缓存不比较大块像素内容。新回归的上传量计数与范围见 [历史经验吸收](slint-nx5-lessons.md#7-已吸收的效率改进命中时刷新两槽替换次序)。

1024×600 时，TDE 的三个 MMZ 块合计 7200 KiB，加上原显示/VPPU 的 4900 KiB，正常总量为 12100 KiB。CPU RGBA 画布约 2400 KiB。创建失败会记录原因并使用软件绘制。当前 SDK 会在首次加载时初始化 SYS/VO，因此 C 接口只允许 `RTLD_NOLOAD` 复用 `nx5` 已初始化的库，不能单独加载或关闭 SYS/VO。

已知尚未提交或已经完成的错误，不会提交部分硬件结果；当前操作由软件重新绘制，后续禁用 TDE。提交后的等待失败、取消失败等不确定状态返回 `AccelerationFailed`，整帧不显示，保留 MMZ、TDE 句柄与库引用。实现不调用全局 Reset，也不会在 `EndJob` 之后尝试 Cancel。

DMA 已知结束后的 MMZ 清理逐块执行；任一释放失败均不重试该地址，并禁止进程内再次创建 TDE，后续使用软件路径，防止部分释放失败造成分配反复累积。其余块继续释放，已无在途 DMA，因此正常关闭 TDE 和库引用；这与未知 DMA 的完整保留策略不同。主机及 ARM CPU 替身验证“不释放即报错”和“释放后仍报错”，未人为触发真实 SDK 释放失败，详见 [清理失败保护](slint-nx5-lessons.md#5-已补充保护mmz-释放失败后禁止反复分配)。

当前显示使用 `FBIOPAN_DISPLAY` 仅更新页偏移，并等待 #233 fyfb 驱动回调内的完成信号。该回调的阻塞 `down` 与通知 `up` 可在 [该阶段模块反汇编](../evidence/nx5-tde/fyfb-disassembly.txt) 中核对。这是所核对固件的完成契约证据；运行时代码不检查固件版本或文件摘要，按 ARM32、显示所有权、SDK 接口及布局进行准入。其他固件仍需验证相同完成语义，页号读回仅用于错误检测。现行准入边界见 [平台说明](software-platform.md#准入边界)。

## 初次接入阶段的验证覆盖

- `fbdev` 和 `nx5` 各 13 个主机测试；`nx5-tde` 16 个主机测试，另外两个实机专用测试在主机默认忽略。
- 实机纹理测试确认 TDE 实际启用，比较所有输出像素，并测量三种操作。80 帧增量运动与独立软件全量重绘比较，最大通道差为 1。
- 复用实际演示组件及生产双页写入代码，将 100 个交替页输出与独立线程生成的软件全量帧逐像素比较，最大通道差为 1，无超过容差的通道。该项用内存文件承载两页，TDE 使用实际硬件；真实显示另由上屏示例验证。
- 在设备上验证奇数裁剪、整体透明度、缩放、小图、180 度旋转、透明目标的软件回退，以及注入已知未提交错误后的完整软件重绘。
- C 层在 SDK 调用边界注入 Begin、绘制、Cancel、End 和缓存同步错误，确认 CPU 图像不被部分提交，未知作业不释放资源。ASan/UBSan 检查通过；运行环境不支持 LeakSanitizer，因此关闭其泄漏扫描，资源释放另有明确计数断言。未在设备上人为制造 DMA 挂死。
- Cargo feature 矩阵、严格 Clippy、格式检查及 ARM release 构建通过。

证据：[双页逐帧记录](../evidence/nx5-tde/demo-pages.log)、[原生故障注入](../evidence/nx5-tde/native-faults.log)、[构建与文件摘要](../evidence/nx5-tde/validation.json)。

## 构建与接入

```sh
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
cargo build --locked --offline --release --features nx5-tde \
  --target arm-unknown-linux-gnueabi -p slint-bridge-test-app --bin display
```

`NX5_SDK_INCLUDE` 仅在 ARM `nx5-tde` 构建时需要；通用 fbdev、纯软件 nx5、主机测试不依赖 SDK 头文件或厂商链接库。C 接口使用匹配 SDK 的结构和函数签名，动态查找现有 SDK 会话中的符号；不新增 ELF 的 VideoDec 依赖。

部署到设备临时目录后，通过 DPower Terminal 启动纹理示例：

```sh
SLINT_DEFAULT_FONT=/run/slint-bridge-display/DejaVuSans.ttf \
SLINT_BRIDGE_TRACE=1 LD_LIBRARY_PATH=/system/app/clib \
/run/slint-bridge-tde/display-tde --textures --touch=/dev/input/event0
```

上述设备命令约定已将构建输出 `target/arm-unknown-linux-gnueabi/release/display` 部署并命名为 `display-tde`。`SLINT_BRIDGE_TRACE` 只要被设置就启用统计：每 150 次完成提交报告 FPS，每 150 次绘制报告 TDE 纹理复制、混合、软件纹理回调、缓存命中、错误及填充计数；`software` 不是全部软件绘制操作的数量。禁用 TDE 时构建 `--features nx5` 即可。日常统一部署路径见 [NX5 设备说明](../test-app/devices/nx5/README.md)，UI 截图使用 [Slint MCP](slint-mcp.md)。

实机回归是忽略测试，需先退出显示所有者，并逐项单独运行：

```sh
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
cargo test --locked --offline --release -p slint-bridge --features nx5-tde \
  --target arm-unknown-linux-gnueabi --lib --no-run
# 部署上述命令打印的测试可执行文件为 tde-tests 后，在设备运行：
export SLINT_DEFAULT_FONT=/run/slint-bridge-display/DejaVuSans.ttf
export LD_LIBRARY_PATH=/system/app/clib
./tde-tests backends::nx5::tde::tests::nx5_hardware_compare --exact --ignored --nocapture
./tde-tests backends::nx5::tde::tests::nx5_demo_pages_match_full_frames --exact --ignored --nocapture
# 面积/形状阶梯：真实 TDE，比较完整回拷成本，不写物理显示页。
./tde-tests backends::nx5::tde::tests::nx5_fill_area_crossover --exact --ignored --nocapture
# 翻页节拍测试只交替现有页，不写像素：
./tde-tests backends::nx5::tests::blocking_flip_cadence --exact --ignored --nocapture
```

离线重放证据：`python3 test-app/tools/nx5/tde/check-results.py`。
