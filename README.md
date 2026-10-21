# Slint Bridge

面向低性能 Linux 设备的 Rust / Slint 1.18.1 适配库。通用层负责事件循环、输入、保留画布和双 framebuffer；板级实现负责准入、硬件加速及翻页完成语义。

NX5 / NX1 / X9 / F1 的 [配置速查](docs/getting-started.md#按设备选择配置)、新电脑配置、Git 依赖、交叉编译和业务接入步骤见 [获取配置与使用指南](docs/getting-started.md)。

仓库包含两个 Cargo package：根目录的 `slint-bridge` 库，以及通过公开接口使用它的 [test-app/](test-app/README.md) 独立测试工程。两者共用 workspace、锁文件和构建产物。

## 目录

```text
slint_bridge/
├── Cargo.toml / Cargo.lock    # 根库 + test-app workspace
├── build.rs                  # 可选 NX5 TDE 原生桥构建
├── .cargo/                   # ARM / RISC-V 工具链及板级 CPU 预设
├── src/
│   ├── lib.rs                # Config、init、Error、Backend
│   ├── runtime.rs            # UI 事件循环与调度
│   ├── framebuffer.rs        # 保留画布、双页与脏区
│   ├── texture_renderer.rs   # CPU 批量混合与旋转纹理分块
│   ├── software_background.rs # fbdev 不透明背景缓存
│   ├── software_texture.rs    # 重复图片的等尺寸旋转缓存
│   ├── presentation.rs       # 通用后台提交与完成通知
│   ├── input.rs / geometry.rs
│   ├── wake.rs / event_wait.rs
│   ├── backends/
│   │   ├── mod.rs            # 私有板级契约与准入分派
│   │   ├── nx1/              # sun8i 纯 CPU framebuffer、参考 PAN 约定
│   │   ├── nx5/              # NX5 翻页、SDK 会话及 tde/
│   │   ├── x9/               # X9 纯 CPU framebuffer 与 DRM 完成事件
│   │   └── f1/               # PAN/VSYNC、64 位写页、GE 画布及 PNG VE
│   └── tests/                # 按职责拆分的库内回归
├── third-party/              # NX5 SDK 头文件/动态库、部署字体及版权说明
├── test-app/                 # 独立测试程序、.slint 界面、设备工具
├── docs/                     # 使用说明、设计和实测报告
├── evidence/                 # 本地原始测试证据，不入库
└── target/                   # 生成的构建与诊断产物，不入库
```

[结构图](docs/diagrams/project-structure.html) · [双页并行时序图](docs/diagrams/runtime-sequence.html) · [文档索引](docs/README.md)

## 接入库

适配代码集中在本仓库；NX5 SDK 所需头文件、动态库及部署字体保存在 [third-party/](third-party/README.md)，构建脚本默认使用仓库内资源。当前从 [Slint 仓库](http://192.168.253.238/wshl/slint-last.git) 获取同源 Slint / 软件 renderer，依赖清单不指定提交哈希；该源码包含所需扩展，不能只凭相同版本号替换为另一个来源。首次构建前联网执行 `cargo fetch --locked`，之后可使用文档中的离线构建命令。

```toml
[dependencies]
slint-bridge = { git = "ssh://git@github.com/omni-stack-gen/slint_bridge.git", default-features = false, features = ["nx5-tde"] }
slint = { version = "=1.18.1", git = "http://192.168.253.238/wshl/slint-last.git", default-features = false, features = ["std", "renderer-software", "compat-1-18"] }
```

上例直接为业务应用启用 NX5 TDE，不需要将适配层源码放入业务目录，也不需要再定义应用 feature 转发。X9 将依赖的 `features` 改为 `["fbdev"]`；需要同一应用切换设备时使用 [业务模板](docs/getting-started.md#在自己的-rust-应用中接入)。本仓库的库和 `test-app` 默认均不选板，运行仓库构建命令时必须显式传 `--features`，具体分层规则见 [平台说明](docs/software-platform.md#构建依赖)。

在 UI 线程、创建 Slint 根组件前安装平台：

```rust
use slint_bridge::{Config, InputDevice};

slint_bridge::init(Config {
    inputs: vec![InputDevice::touchscreen("/dev/input/event0")],
    ..Config::default()
})?;
// 然后按普通 Slint 应用创建组件并调用 run()。
```

| Feature | 行为 |
| --- | --- |
| `fbdev` | 纯 CPU framebuffer；当前接入 X9 Lombo #452 板级完成契约 |
| `nx1` | NX1 ARM32；纯 CPU 渲染，采用已明确选择的参考 PAN 双页约定 |
| `f1` / `f1-g2d` | F1 RISC-V；使用参考 PAN + WAITFORVSYNC，后者包含 GE 填充与受限图片混合 |
| `nx5` | NX5 VO 初始化、软件渲染与双页显示 |
| `nx5-mma-background` | 包含 `nx5-tde` 与同源 Slint 外部像素支持；运行时由 Config 选择背景存储 |
| `nx5-tde` | 包含 `nx5`，加速大块 RGBA 复制和透明混合；另有 QuickFill 诊断开关 |

必须选择一个板级 feature；`fbdev`、`nx1`、`nx5` 与 `f1` 两两互斥。NX5 按 SDK/驱动接口和双页布局运行，不比对内核版本、驱动名称或文件摘要；`fbdev` 仅准入已验证的 X9 Lombo 固件与布局，其他 framebuffer 仍拒绝。NX1 和 F1 按用户明确选定的参考翻页约定运行，适用固件及验证限制分别见 [NX1 设备说明](test-app/devices/nx1/README.md) 和 [F1 设备说明](test-app/devices/f1/README.md)。SDK 目录由系统动态库搜索路径决定，见 [平台说明](docs/software-platform.md#准入边界)。

`Config::background_storage` 默认为 `BackgroundStorage::Heap`。需要 MMA 时设为 `Mma`，在 `init(config.clone())` 后、首次绘制前通过 `config.prepare_background(image)` 准备图像；其他设备显式选择 MMA 会被拒绝。见 [背景接入与限制](test-app/devices/nx5/mma-background/README.md)。

F1 可通过 `try_decode_png(encoded)` 显式请求有界 PNG 硬解，成功返回普通 Slint 图像；`Ok(None)` 时由应用选择软件解码。错误不能作为软件回退，须保留进程等待设备恢复。格式、内存及生命周期见[平台说明](docs/software-platform.md#f1-png-加载)。展示程序只对实测受益的大背景使用该入口，SVG 加载和缓存仍由 Slint 负责。

`Config::frame_interval` 是最小渲染启动间隔，默认 16667 微秒；没有 NX5 专用 30 FPS 钳制。NX5 #233 驱动实测提交上限约为 29 FPS，原因及尚未实现的单周期改动见 [驱动调查](docs/nx5-display.md#nx5-single-cycle)。

同时最多一次翻页提交和一帧预渲染。提交完成前不改写两张物理显示页，成功后才复用旧页；不提供单缓冲或忽略完成错误的回退。framebuffer 与触摸设备路径由 `Config` 指定，分辨率和行跨度读取设备当前模式；接口、像素格式和输入范围见 [平台说明](docs/software-platform.md)。

## 测试与用法示例

从 [test-app/src/bin/basic.rs](test-app/src/bin/basic.rs) 查看最小接入方式，从 [test-app/ui/](test-app/ui/app.slint) 查看独立界面文件。完整用例、参数及设备配置见 [测试工程说明](test-app/README.md)。

完整界面展示使用 `--bin showcase`：包含总览、控件/表单、设备列表、动画、图像/透明、交互记录、运行时图片缩放、富内容长列表和时钟九页。主机原生 MCP 预览、自动交互检查及各设备运行方式见 [展示应用说明](test-app/README.md#展示应用-showcase)；`display` 保留为原有诊断场景。

图片背景中的多个透明区域使用独立 `--bin background-holes` 演示：应用组件只裁剪背景，前景控件和弹窗正常叠加，见 [接入与约束](test-app/README.md#图片背景中的多个透明区域) 及 [NX5 实测](docs/nx5-validation.md#nx5-background-holes)。

```sh
# 独立测试工程的公共接口回归。
cargo test --locked --offline -p slint-bridge-test-app --features fbdev

# 整个 workspace 的私有/公共回归。
cargo test --locked --offline --workspace --features nx5-tde
python3 test-app/tools/check-feature-matrix.py

# 独立测试程序的 ARM 构建。
cargo --config .cargo/nx5.toml build --locked --offline --release \
  -p slint-bridge-test-app --features nx5-tde \
  --target arm-unknown-linux-gnueabi --bin display
```

`--offline` 复用当前缓存；新机器需先获取锁定依赖。工具链配置保留在根 `.cargo/`，新机器所需安装项见 [资源说明](third-party/README.md#新机器仍需安装的工具)。库被其他 workspace 引用时，消费方仍须配置自己的交叉链接器。

UI 检查使用 [Slint 原生 MCP](docs/slint-mcp.md)：元素查询、截图、交互与事件录制。底层页生命周期、evdev 解码、TDE 故障注入和实际输出采样继续使用相应回归，不能用 UI 截图代替。

当前设备记录及测试命令分别见 [NX5](test-app/devices/nx5/README.md)、[NX1](test-app/devices/nx1/README.md)、[X9](test-app/devices/x9/README.md) 和 [F1](test-app/devices/f1/README.md)。历史报告由 [docs/README.md](docs/README.md) 导航；报告引用的 `evidence/` 原始证据作为本地档案保存，不纳入 Git，检出仓库后需另行获取。
