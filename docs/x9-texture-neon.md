# X9 纹理采样与 NEON 混合

2026-10-15，在 [流水线分析](x9-pipeline-audit.md) 的基础上细分 UI render，优化最重的预乘 RGBA 纹理路径，并用同一 X9 进行优化前后对照。BackgroundWithHoles 不在本轮修改范围。

## 设备、版本与测量范围

设备仍为 X9 / Lombo N7、双 Cortex-A7、Linux #452，MAC `82:a1:cf:66:54:03`，启动 ID `ebe00047-16a3-4600-89ee-0fe601f48967`。操作前通过已恢复的 root Telnet 重新核对身份和 framebuffer / DRM / evdev 所有者；仅切换 X9。NX5 的常驻演示没有重启或更换。

基线正式程序为 `showcase-07`，桥接库 `cacf98f2fedfc99a6d965468cd709adfd7c5466f`、Slint `fa6a6d0158c9730fed62ef1eb1c9730c80214e20`，SHA256 `0ea44d41b3252118e26cdb501b4401aaeb09456d83303adbd804ce751f12156e`。候选基于桥接仓库 `6ae646b`，只增加回归场景和测试；运行优化位于同源 Slint 提交 `5cfde5f2f2a9417373d4fd914730919327e935f8` 的 `internal/renderers/software/draw_functions.rs`。候选正式 ELF 的 SHA256 为 `6893765bddc82ed36d097f46fd6c682620c0385818300a7580580d09c1e09d86`。

沿用 `.cargo/x9.toml`、release / ThinLTO、Cortex-A7、hard-float 与同一 DejaVuSans 字体，没有改频率、编译目标、界面素材、列表条数或提交节拍。800×1280 物理面板仍旋转 90°，逻辑界面 1280×800。离屏 render、真实完成提交 FPS、无探针 CPU/RSS 分开测量。

## render 热点

在现有确定性富列表 benchmark 中增加临时批次计时，先预热 20 帧，再测 60 帧。计时包围纹理格式、旋转、整体 alpha 分支，以及文字、填充、渐变和路径；不逐像素读时钟。带计时代码的最终 RGBA 与无探针基线完全一致。

| render 分类，平均每帧 | 纵向 | 横向 |
| --- | ---: | ---: |
| 总 render | 91.62 ms | 59.56 ms |
| 预乘 RGBA、90°、整体 alpha=255 采样与混合 | 79.87 ms（87.18%） | 48.97 ms（82.21%） |
| AlphaMap 字形 | 2.99 ms（3.26%） | 2.59 ms（4.35%） |
| 剩余 render | 8.76 ms | 8.01 ms |

剩余项包含布局、遍历、脏区等未单独归类工作，不能全部称为某一种绘制操作。圆角、渐变及路径在这段稳态缓存列表里的直接计时为零；这不说明启动、其他页面或未缓存素材没有这些成本。计时内的 texture 项同时包含采样与混合，没有进一步把二者分离。临时探针已移除，正式程序不包含这些计时。

## 实现与反证

原先旋转纹理的 32×32 分块改善了缓存局部性，但 `blend(color)` 逐值传递的循环仍为标量。新增带默认实现的 `TargetPixel::blend_sampled_slice`：原采样器负责坐标、旋转、缩放和裁剪，按顺序每像素调用一次；默认像素类型继续逐像素混合。仅预乘 RGBA 目标重写该方法，每批最多 32 像素，使用 128 字节栈缓冲收集源色，然后进行连续混合。

全批 alpha=255 时直接复制；其余保持原整数取整语义。对 `0 ≤ v ≤ 65025`，`floor(v/255) = (v + 1 + (v >> 8)) >> 8`，中间值不溢出 u16。RGB 与目标 alpha 均使用该精确式；原单像素 `/255` 实现保留作为独立参考。没有 unsafe、手写汇编、新依赖、堆缓冲或第二张 CPU 画布。其他纹理格式、着色与整体 alpha<255 的路径保持原逻辑。

最终 benchmark 与正式 `showcase-08` ELF 的纹理函数中均已确认 `vmull.u8`、`vadd.i16`、`vshr.u16`、`vsub.i16` 与向量存储指令，证明这些混合循环实际生成 NEON；不是仅根据 CPU feature 作判断。短尾部由编译器处理。未改变写页、双 framebuffer、单笔在途提交、单帧预渲染和匹配 DRM 完成后才复用物理页的契约，因此无需修改运行时序图。

试验中，简单地收集 8 像素再逐值调用 `blend` 没有生成目标向量循环，反而使 90° render 达到 104.48 / 67.70 ms，已弃用。最终方案根据性能与像素对照保留，而非以源代码出现“批量”或“NEON”作为验收。

## 离屏 render 对照

同一板上执行每帧固定滚动 8 个逻辑像素的 [benchmark](../test-app/examples/x9-render-bench.rs)，每组预热 20 帧、测量 60 帧，独立于正式性能采样。以下是平均 render 耗时，不含写页或显示等待，也不能直接当作屏幕 FPS。

| 旋转 / 方向 | 基线 | 最终 | 耗时下降 | 吞吐倍率 |
| --- | ---: | ---: | ---: | ---: |
| 90° 纵向 | 90.31 ms | 69.86 ms | 22.64% | 1.293× |
| 90° 横向 | 59.05 ms | 44.56 ms | 24.52% | 1.325× |
| 0° 纵向 | 62.41 ms | 46.44 ms | 25.58% | 1.344× |
| 0° 横向 | 42.33 ms | 31.61 ms | 25.33% | 1.339× |

四组各比较完整 1280×800 RGBA，全部逐字节相同，共 4,096,000 像素。0° 收益说明优化不只依赖 90° 的列访问；这些方向间的差值仍不是一次独立旋转复制的成本。

## 正式上屏、CPU 与 RSS

基线和候选都在本轮重新运行：各用一个无计数探针的正式进程测 CPU/RSS，另开一个相同 ELF 的进程仅用 `count-flips.so` 测完成提交 FPS。每方向预热后持续拖动至少 60 秒，排除首尾阶段边界；动作核对位移超过视口 40%、未到端点、轴不漂移。CPU 口径为单核 100%，包含进程全部线程；RSS 来自无探针进程。

| 场景 | 基线 FPS | 最终 FPS | 提升 | CPU 基线→最终 | 平均 RSS 基线→最终 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 纵向 | 10.24 | 12.83 | 25.35% | 98.10% → 98.20% | 17.51 → 17.58 MiB |
| 横向 | 15.60 | 19.93 | 27.72% | 98.19% → 98.16% | 17.74 → 17.84 MiB |

有效动态窗口为 58.1–65.2 秒；FPS 只计成功请求对应的匹配 DRM 完成事件，解析检查没有溢出、cookie 错配或提交错误。动态样本 CPU 频率均为 912 MHz，没有 major fault，外部观察器自身 CPU 不超过约 0.2%。这是各版本各方向一次有效长窗口的对照，未做多轮统计显著性分析。

CPU 仍接近单核饱和，收益体现为同等 CPU 占用下更多的完成帧，不能声称总 CPU 降低约 25%。平均 RSS 增量约 0.07 / 0.10 MiB；没有增加整帧缓冲。停止拖动和返回 Controls 后 CPU 约 0.3%–0.5%，计数窗口为零新增提交。四个正式采样进程与候选 20 秒 smoke 均正常退出并恢复 KD_TEXT。

第一轮基线 CPU 控制动作完成，但通过 ADB 后台启动的外部观察器没有成功运行，故该窗口无 CPU 数据，已作废并保留原始记录。随后改由核对过的 root 启动脚本启动应用和观察器，重跑完整基线；上表只使用有效重跑窗口。性能采样期间未进行截图、输入录制或其它板端性能负载。

## 回归与可复跑入口

- `python3 test-app/tools/check-feature-matrix.py`：`fbdev`、`nx5`、`nx5-tde` workspace 测试通过，缺失/互斥 feature 按预期拒绝；没有引入桌面或 GPU backend。
- `cargo fmt --all --check`，`nx5-tde,slint/mcp` 和 `fbdev,slint/mcp` 两套 workspace / all-targets 严格 Clippy，以及 workspace / Slint renderer API 文档构建通过。
- 同源 Slint 软件 renderer 的 9 项库测试通过，包含 576 组格式、旋转、缩放、alpha、着色和裁剪的分块/扫描行对照。
- [公开像素回归](../test-app/tests/texture_blend.rs) 的两项测试在主机和 X9 release ARM 程序上通过：24 个旋转/裁剪/背景状态共 294,912 像素；131,072 组字节乘积与目标 alpha 对照；13 种 0–65 像素批次长度、四种起点偏移、透明和不透明批次的尾部及守卫检查。采样次数同时受检。
- X9 与 NX5 release 交叉构建通过。NX5 本轮只做主机与构建兼容性检查，没有运行新的 NX5 实机或 TDE/DMA 测试，既有常驻程序保持运行。
- Controls 48% 和 Scaling 125% 的同字体、同旋转独立 CPU 参考与旧版完全一致，共 2,048,000 像素。

新回归的主机与 ARM 构建命令：

```sh
cargo test --locked --offline -p slint-bridge-test-app --features fbdev --test texture_blend
cargo --config .cargo/x9.toml test --locked --offline --release \
  --target arm-unknown-linux-gnueabihf --features fbdev \
  -p slint-bridge-test-app --test texture_blend --no-run
```

第二条只构建；实际 ARM 回归需上传 Cargo 输出的测试 ELF、校验 SHA256 后在 X9 执行 `--test-threads=1`。测试使用内存软件窗口，不访问 framebuffer 或 SDK。正式构建/部署入口见 [设备说明](../test-app/devices/x9/README.md)，离屏计时、完成事件计数、CPU 采样和物理页对照入口见 [工具说明](../test-app/tools/x9/README.md)。

## 实机功能、物理页与最终状态

验收进程 PID 4993 通过八页原生 Slint MCP 检查，共保存 20 张截图（含末尾 Controls）：表单、重置、触屏键盘、弹窗、动画暂停、透明开口、PNG/SVG 缩放、富列表横纵拖动/选择，以及 18 张卡片的横向往返、筛选和边界检查。录制检查无丢失或未知事件；截图前最低 MemFree 60,364 KiB（约 58.9 MiB），高于 32 MiB 保护阈值。已查看缩放和富列表截图，布局、渐变、文字和透明合成正常。

Controls 48% 静态采样为 yoffset=1280，随后通过真实 `/dev/input/event1` 写完整 Type-A 按下/释放包导航到 Scaling，再设为 125%；该帧采样为 yoffset=0。原生事件记录得到 Accepted 的按下、释放和离开事件，业务页面同时改变。此项为 evdev 注入，不能称为用户实体触摸。

两张不同物理前台页均通过采样前后偏移一致检查；BGRA 转 RGBA 后与同字体、同旋转的独立 CPU 参考逐字节相同，共 **2,048,000 像素、零差异**。两份参考又与旧版参考完全一致。静态 framebuffer 采样证明这两帧的物理页内容，不替代动态扫描输出或任意帧无撕裂认证。

完成验收后，先保存测试进程日志和 UI 状态，再终止它、确认显示所有者已释放，并通过 `recover-console` 等待匹配完成后恢复 KD_TEXT；随后启动干净的正式常驻进程。当前 `showcase-08` 观测 PID **5679**，无时限、trace 或 LD_PRELOAD，maps 无计数/故障探针；富列表纵向起点、无选择，Controls 48%。三个线程，RSS/VmHWM 均为 17,884 KiB；KD_GRAPHICS，MemFree 98,400 KiB。该内存是新常驻进程的静态快照，不是前述性能窗口的平均值或截图峰值。

保留 `showcase-07`、字体、当前启动脚本/日志、独占锁和初始恢复材料。旧 05/06 与本轮 77 个诊断文件先逐文件 SHA256 核验归档再删除板端副本，共释放约 52.7 MiB tmpfs。原始初始 framebuffer 备份未改动。常驻 PID、摘要及恢复信息见 [设备快照](../test-app/devices/x9/current-target.json)，每次操作前仍需重新核对。

## 证据获取

本轮原始档案位于本地忽略目录 `evidence/x9-neon/2026-10-15/`，不随 Git 分发，复核需另行取得。包括 `profile-summary.json` / `profile.patch` / `render_profile.rs`、四组完整 RGBA 和 CSV、`final.asm` / `final-neon.txt` / `showcase-final.asm` / `showcase-neon.json`、`texture-arm.log`、主机检查日志、`baseline-cpu2` / `baseline-fps` / `final-cpu` / `final-fps` 的采样和 `validated.json`、`comparison.json`、`mcp-final`、`evdev-events.json`、`physical-comparison.json`、`resident-final.json` 及归档清理 manifest。`manifest.json` 记录原始文件 SHA256；原先无观察器的基线窗口和失败的 8 像素试验也保留，未改写为有效结果。
