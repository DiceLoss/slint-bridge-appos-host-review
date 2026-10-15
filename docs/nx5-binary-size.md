# NX5 测试程序体积分析

> 文中的 `NX5_SDK_INCLUDE` 外部路径保留采集环境；当前构建默认使用仓库资源，配置方法见 [使用指南](getting-started.md)。

本记录分析 2026-10-14 的 `showcase` 测试程序。测量对象是去符号后的 ARM 可执行文件，不是整机固件镜像；外置字体、厂商 SDK 及系统动态库不计入该文件。平台与运行限制见 [平台实现说明](software-platform.md) 和 [NX5 设备说明](../test-app/devices/nx5/README.md)。

## 当前文件的实测组成

分析对象为源码 `2fddc04`、Slint `9a1a3b1` 的部署副本，**10801536 字节 = 10.3011 MiB**，SHA256 `e2a0f1245c3a471329091f215b68311f16362db954837901e63b4fcc53449f39`。把保留符号的 ARM 文件重新 strip 后，与部署副本逐字节摘要一致，因此符号分析对应同一个程序。未 strip 的文件为 13747424 字节；其符号表等约 2.81 MiB 已经删除，不能再次计为缩减空间。

下面按 `.text` 中保留函数的定义命名空间归属机器码，合并相同地址/长度的别名并检查区间不重叠。可归属符号覆盖 8340696 字节，余下 2400 字节计入其余机器码。LTO 内联、泛型实例和共享函数会交叉，**应用/库各项是归属估计，不是可以独立删除的包大小**；ELF 节、文件总长和识别出的资源载荷是直接测量值。

| 组成 | 字节 | MiB | 占文件 |
| --- | ---: | ---: | ---: |
| 应用与生成 UI 机器码 | 1927128 | 1.838 | 17.84% |
| 桥接 Rust/C 机器码 | 67284 | 0.064 | 0.62% |
| Slint 本体及 MCP 机器码 | 1257068 | 1.199 | 11.64% |
| 字体与文字处理机器码 | 1449524 | 1.382 | 13.42% |
| SVG 与矢量绘制机器码 | 1354844 | 1.292 | 12.54% |
| 位图编解码及压缩机器码 | 786396 | 0.750 | 7.28% |
| Rust 运行库、通用依赖及其余机器码 | 1500852 | 1.431 | 13.89% |
| 内嵌 PNG/SVG 资源 | 1012716 | 0.966 | 9.38% |
| 其他只读/可写数据、展开表、重定位和 ELF 开销 | 1445724 | 1.379 | 13.38% |

按用户关心的分类，应用及桥接机器码约 **1.90 MiB**；Slint 本体及其图形、文字、编解码依赖机器码合计约 **4.62 MiB**；原始 PNG/SVG 资源约 **0.97 MiB**。其余为 Rust/通用代码以及未强行归属到应用或库的常量、表、重定位和文件结构。生成 UI 算在应用内；“SVG 文件字节”与“SVG 引擎代码”是两项不同成本。

| ELF 主要部分 | 字节 | 说明 |
| --- | ---: | --- |
| `.text` | 8343096 | 机器码，约 7.96 MiB |
| `.rodata` | 1678580 | 包含上述 1012716 字节图片，其余是字符串、常量、Unicode/查找数据等 |
| `.ARM.extab` + `.ARM.exidx` | 430764 | 异常展开与索引，不能在保留 unwind 行为时直接删除 |
| `.data.rel.ro` | 179744 | 启动重定位后只读的数据、表和指针等 |
| 其余文件节、ELF 头及对齐 | 169352 | 补齐到实际文件长度，不把 NOBITS 的 BSS 算为文件载荷 |

## 内嵌 PNG/SVG 载荷

从部署 ELF 扫描 PNG 签名并逐块校验 CRC/结束标记；对 SVG 核对完整文件字节或验证内嵌 XML，检查各载荷区间不重叠。源目录里用于生成 PNG 的四份卡片 SVG、原始大尺寸背景文件不等于部署资源，不能把整个 assets 目录大小相加。

| 资源 | 字节 | KiB |
| --- | ---: | ---: |
| 构建生成的 1024×600 背景 PNG | 934907 | 913.00 |
| 两张纵向底图 PNG | 28130 | 27.47 |
| 两张横向底图 PNG | 38281 | 37.38 |
| 缩放示例 PNG | 2805 | 2.74 |
| 应用的 6 个 SVG | 1994 | 1.95 |
| Fluent 等额外内嵌 SVG，10 个 | 6599 | 6.44 |
| **总计：6 PNG + 16 SVG** | **1012716** | **988.98** |

PNG 合计 1004123 字节，SVG 合计 8593 字节；SVG 文本本身很小，主要成本来自代码引擎。外置字体、SDK 和内核不在上述文件内，运行期 SVG 缓存及 MMZ 缓冲也不是这些资源字节。

## 不修改生产源码的缩减实验

候选使用独立 Cargo profile 构建，保留原有 `nx5-tde,slint/mcp`、`SLINT_EMIT_DEBUG_INFO=1`、Cortex-A7/NEON、`codegen-units=1`、panic unwind、八页界面与原始素材。所有产物用同一个 ARM strip 工具处理；每份候选均再次核对全部 22 个图片载荷字节及 MCP 元素 ID 仍在。未修改 Cargo 清单、资产、外部 Slint 源码或当前板上程序。

| 构建参数 | ARM 文件字节 | MiB | 相对当前减少 |
| --- | ---: | ---: | ---: |
| 当前：opt-level=3 + thin LTO | 10801536 | 10.301 | 0.00% |
| opt-level=3 + fat LTO | 10125700 | 9.657 | 6.26% |
| opt-level=s + fat LTO | 8110460 | 7.735 | 24.91% |
| opt-level=z + fat LTO | 6971772 | 6.649 | 35.46% |

另对六张 PNG 做无损重编码：背景 934907 → 752405 字节；其余 PNG 保留原辅助块，仅重压缩相同的过滤后数据流。总载荷节约 **191499 字节 = 187.01 KiB = 0.183 MiB**。Pillow 验证模式、尺寸和 RGBA 字节一致，并用当前 Slint 解码器逐像素复核全部 **994624 像素**，结果完全一致。未降低分辨率、量化颜色、改 alpha、改 PNG 为 JPEG，也未修改 SVG。

下表是将已测 PNG 载荷差额叠加到已构建 ELF 的**近似估算**，尚未组合重链接，最终文件对齐与合并可能产生小幅差别：

| 方案 | 预计 MiB | 相对 10.30 MiB 的空间 |
| --- | ---: | ---: |
| 当前：opt-level=3 + thin LTO + 无损 PNG | 10.119 | 0.183 MiB / 1.77% |
| opt-level=3 + fat LTO + 无损 PNG | 9.474 | 0.827 MiB / 8.03% |
| opt-level=s + fat LTO + 无损 PNG | 7.552 | 2.749 MiB / 26.69% |
| opt-level=z + fat LTO + 无损 PNG | 6.466 | 3.835 MiB / 37.23% |

无损 PNG 是已验证像素等价的约 0.18 MiB 资源收益；fat LTO 在保持 opt-level=3 时还测得约 0.64 MiB 文件收益，适合作为优先验证方案。s/z 的编译结果说明更大的体积空间确实存在，但优化策略可能改变软件绘制速度，尤其不能把 z 的最小文件直接作为最终推荐。**本轮没有部署候选，也没有新的 ARM 实机功能、物理页或 FPS 结果**，故尚不能保证候选保留当前 27.52/26.03 FPS。

三种 profile 分别通过 `showcase`、`showcase_scaling`、`showcase_repaint`、`showcase_rich_repaint` 四组主机回归。此外，使用相应 profile 的当前 Slint/应用库和同样的优化参数重新渲染富列表 25 状态，每组 **15360000 像素**均与上一轮保留的原版整帧基线逐字节一致，同时继续检查 `ReusedBuffer` 与独立 `NewBuffer` 的一致性。参考是历史主机像素存档，本轮重新生成的是候选主机帧；这不是 ARM 物理页或实机帧率验证。日志与结果在 `host-test-results.json`、`cross-profile-pixels.json`、`compare-rich-*.log`。

## 证据与复现

本地忽略目录 `evidence/nx5-binary-size/2026-10-14/` 保存 ELF 节与符号分析、载荷提取、无损压缩、Slint 解码对照、三组 ARM 构建日志和候选验证。需另行取得原始证据；`target/nx5-size-analysis/` 为分析副本，当前部署摘要未改变。`manifest.json` 记录源码与基线摘要。关键文件：`baseline-analysis.json`、`breakdown.json`、`variants.json`、`lossless-png.json`、`slint-png-comparison.log`。

示例：仅新增命令行诊断 profile，不修改 release 配置：

```sh
SLINT_EMIT_DEBUG_INFO=1 \
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
cargo --config .cargo/nx5.toml \
  --config 'profile.size-s.inherits="release"' \
  --config 'profile.size-s.lto="fat"' \
  --config 'profile.size-s.opt-level="s"' \
  build --locked --offline --profile size-s \
  -p slint-bridge-test-app --features nx5-tde,slint/mcp \
  --target arm-unknown-linux-gnueabi --bin showcase
```

## 依赖组成与归属口径

本轮按 `arm-unknown-linux-gnueabi`、`nx5-tde,slint/mcp` 核对普通运行时依赖；查询排除构建依赖和过程宏：

```sh
cargo tree --locked --offline -p slint-bridge-test-app \
  --target arm-unknown-linux-gnueabi --features nx5-tde,slint/mcp \
  -e normal,no-proc-macro --prefix none --format '{p} {f}'
```

`Cargo.lock` 中出现某个包，不等于该包的全部代码进入部署文件。主机上的 `slint-build`、Slint 编译器、过程宏和构建期 PNG 缩放不作为运行库链接到 ARM 程序；它们生成的 UI 代码和资源则会进入。`Cargo tree` 给出启用路径，最终留存字节数仍须由 ELF 和符号分析判断。当前 workspace 使用 resolver 3，构建依赖/过程宏的 features 不与目标普通依赖简单合并，详见 [Cargo resolver 规则](https://doc.rust-lang.org/cargo/reference/resolver.html#feature-resolver-version-2)。本地入口为 [workspace 清单](../Cargo.toml)、[测试应用清单](../test-app/Cargo.toml) 和 [构建脚本](../test-app/build.rs)。

| 组成 | 当前实现与依赖 | 归属与保留原因 |
| --- | --- | --- |
| 应用逻辑、生成 UI 与桥接层 | `showcase` / `slint_bridge_test_app`、Slint 生成的组件/绑定代码、`slint_bridge`、原生 TDE 适配 | 不能只把手写 Rust 算成“应用”；生成的八页 UI 代码也属于应用。泛型和 LTO 内联使“应用”和“Slint”符号之间存在交叉 |
| Slint 核心、软件绘制与布局 | `i-slint-core`、`i-slint-renderer-software`、`i-slint-common`、布局/路径/颜色辅助库 | 属性绑定、动画、控件、文字布局、软件回退和 PNG/SVG 绘制共同使用；并非一个独立 `libSlint.so` |
| 字体和文字 | Parley、Fontique、Harfrust、Skrifa、Swash、ICU 属性/分段/归一化数据等 | 字体文件外置不代表文字引擎可删。当前 renderer 的 `std` 开启 `systemfonts`，后者开启共享字体、整形和栅格化链；编辑控件和动态文字也使用这些能力 |
| PNG/JPEG 编解码 | `image 0.25.10` 只启用 `png,jpeg`；`png`、`fdeflate`、`flate2`、`miniz_oxide`、`zune-jpeg` 等 | 没有启用 `image/default-formats`，不能声称删一个默认格式开关就能移除所有格式。PNG 解码供素材使用，PNG 编码供 MCP 截图使用 |
| SVG 解析与栅格化 | `resvg 0.48.1` 的 `text,raster-images,svgz`，以及 usvg、tiny-skia、XML/CSS/路径解析、GIF/WebP 等 | SVG 本身是少量 XML 字节，但解析、文字、滤镜和栅格化需要代码；`raster-images` 还支持 SVG 内嵌位图 |
| 原生 MCP | `i-slint-backend-testing`、异步网络、HTTP 解析、serde/JSON、prost/pbjson、base64、图片编码 | 当前测试程序的查询、交互和截图功能；不是纯粹的链接调试符号 |

上述依赖源于本轮 Slint 源码 `/home/dpower/data/GitHub/slint-1.18.1`，HEAD `9a1a3b14c454a751f9a804a5309a4eaae0faeded`。直接依据：

- [Slint workspace 的 image/resvg 声明](/home/dpower/data/GitHub/slint-1.18.1/Cargo.toml:123)；[软件 renderer 字体 feature 链](/home/dpower/data/GitHub/slint-1.18.1/internal/renderers/software/Cargo.toml:19)；[核心的 shared-parley feature](/home/dpower/data/GitHub/slint-1.18.1/internal/core/Cargo.toml:90)。
- [MCP 依赖声明](/home/dpower/data/GitHub/slint-1.18.1/internal/backends/testing/Cargo.toml:42)；[截图编码调用](/home/dpower/data/GitHub/slint-1.18.1/internal/backends/testing/introspection/mod.rs:373)；[MCP 固定请求 PNG](/home/dpower/data/GitHub/slint-1.18.1/internal/backends/testing/mcp_server.rs:382)。
- [应用 PNG/SVG 加载](/home/dpower/data/OMNI_STACK_GEN/slint_bridge/test-app/src/showcase.rs:101)；[SVG 文字的 Fontique 接入](/home/dpower/data/GitHub/slint-1.18.1/internal/common/sharedfontique/svg.rs:5)。

## 保持功能与显示效果的缩减边界

当前已关闭 Slint 默认桌面后端，使用 `compat-1-18`，release 已设置 `lto = "thin"` 和 `codegen-units = 1`，部署还执行 ARM `strip`。因此“关闭默认 features”“开 LTO”“去除符号”都不是新的大块收益。[构建配置](../Cargo.toml)、[部署去符号步骤](../test-app/devices/nx5/README.md)。

可以优先试验以下两类，收益以同条件产物和回归结果为准：

1. **无损资源重编码。** 对实际嵌入的 PNG 使用更充分的无损压缩、移除确实不参与当前解码结果的辅助块，保留尺寸、RGBA 像素和 alpha；不能用有损颜色量化、降低分辨率或改成 JPEG 来冒充等效。背景须处理构建脚本生成的 1024×600 文件，否则只压缩源图可能在重新编码时丢失收益。SVG 可去掉注释和无效元数据，但路径精度、滤镜、坐标和文本语义不能改变。通过同一运行时解码后的逐像素比较确认。
2. **构建参数比较。** 在完整保留 features、UI 元数据和 panic 策略的条件下比较 `opt-level = 3/"s"/"z"` 及 thin/fat LTO。Cargo 将 `"s"`/`"z"` 定义为体积优化，`"z"` 还关闭循环向量化；fat LTO 的跨 crate 优化更多，但没有保证一定更小或更快。当前应用刚做过绘制性能优化，因此尺寸实验通过后仍要复核 CPU/FPS，不能只看文件大小。[Cargo profile 文档](https://doc.rust-lang.org/cargo/reference/profiles.html#opt-level)

以下项目需要明确区分“当前固定界面未用到”与“完整能力不变”：

| 候选 | 当前判断 | 等效性限制 |
| --- | --- | --- |
| 收窄 JPEG、SVG 内嵌 GIF/WebP、SVGZ、SVG 文字支持 | 当前 Rust 素材入口固定加载 PNG 和 SVG；本轮检查 `test-app/ui/assets/*.svg` 未发现 `<image>` 或 `<text>` | 可作为固定素材应用的专项实验，但需要核对所有编译器内嵌 Fluent 图标及运行时入口；会收窄 Slint 原有格式能力，不能称为通用库无损裁剪。SVG 文字支持涉及 Slint 代码，不能只删一个 Cargo feature |
| 合并重复压缩实现 | 当前依赖树同时有 `miniz_oxide 0.8.9` 和 `0.9.1`，分别来自 `png 0.18.1` 的 `0.8` 约束与 `flate2 1.1.10` 的 `0.9.0` 约束 | 不是 Cargo 自动可合并的同版本副本；必须用兼容依赖组合或补丁并验证 PNG 解码、MCP 编码和错误输入，且先确认 ELF 实际留存的重复代码。不能把整个两个 crate 文件大小当可节约值 |
| 精简字体、Unicode/ICU 数据或更换文字引擎 | 动态文本、输入框、字体 fallback 和字形布局共用该链 | 固定 ASCII 字符子集可能变小，但会改变可输入字符、换行、整形或 fallback；不列入当前全部功能不变的确定收益 |
| 预先栅格化所有 SVG | 可移除部分运行时 SVG 工作 | 当前专门测试 PNG/SVG 运行时缩放，改成位图会改变测试功能及放大时效果；不列入等效方案 |

应排除出“保持当前测试功能不变”收益的选项：

- **关闭 MCP 或 `SLINT_EMIT_DEBUG_INFO`。** 前者移除查询/交互/截图，后者使元素 ID 查询等能力缺少 UI 元数据；这是 `.slint` 运行时元数据，与 ELF 的 DWARF/符号表不同。[当前 MCP 构建约定](slint-mcp.md#启用及连接)、[无元数据时拒绝元素 ID 查询](/home/dpower/data/GitHub/slint-1.18.1/internal/backends/testing/introspection/mod.rs:364)。
- **删除 PNG 编码器。** 当前 MCP 截图确实调用 PNG 编码；即使应用从不“保存图片”，该功能仍在使用编码器。
- **改成 `panic = "abort"`。** 该策略从栈展开改为立即结束进程；当前 presentation worker 的退出通知、板级会话持有和析构回归覆盖 panic 路径，不能当作保持生命周期语义的纯体积开关。[Rust/Cargo panic 策略](https://doc.rust-lang.org/cargo/reference/profiles.html#panic)、[当前 worker 与退出保护](../src/presentation.rs:21)、[页生命周期回归](../src/tests/framebuffer_lifecycle.rs:393)。
- **减小 SVG 栅格缓存、TDE 源槽或 framebuffer。** 这些是运行期内存，容量并不以同等字节存入可执行文件；减少它们不能把之前的 MiB 内存占用直接扣减成固件体积。

本节通过源码、目标 feature 树及官方构建文档核对依赖和语义；不把尚未构建的候选写成已实现节约，也不把候选编译成功当成实机功能、像素和性能已验证。
