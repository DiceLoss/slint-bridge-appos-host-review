# Slint Bridge

面向低性能 Linux 设备的 Rust / Slint 1.18.1 适配库。通用层负责事件循环、输入、保留画布和双 framebuffer；板级实现负责准入、硬件加速及翻页完成语义。

仓库包含两个 Cargo package：根目录的 `slint-bridge` 库，以及通过公开接口使用它的 [test-app/](test-app/README.md) 独立测试工程。两者共用 workspace、锁文件和构建产物。

## 目录

```text
slint_bridge/
├── Cargo.toml / Cargo.lock    # 根库 + test-app workspace
├── build.rs                  # 可选 NX5 TDE 原生桥构建
├── .cargo/                   # ARM 工具链和 NX5 / X9 CPU 预设
├── src/
│   ├── lib.rs                # Config、init、Error、Backend
│   ├── runtime.rs            # UI 事件循环与调度
│   ├── framebuffer.rs        # 保留画布、双页与脏区
│   ├── presentation.rs       # 通用后台提交与完成通知
│   ├── input.rs / geometry.rs
│   ├── wake.rs / event_wait.rs
│   ├── backends/
│   │   ├── mod.rs            # 私有板级契约与准入分派
│   │   ├── nx5/              # NX5 翻页、SDK 会话及 tde/
│   │   └── x9/               # X9 纯 CPU framebuffer 与 DRM 完成事件
│   └── tests/                # 按职责拆分的库内回归
├── test-app/                 # 独立测试程序、.slint 界面、设备工具
├── docs/                     # 使用说明、设计和实测报告
├── evidence/                 # 本地原始测试证据，不入库
└── target/                   # 生成的构建与诊断产物，不入库
```

[结构图](docs/diagrams/project-structure.html) · [双页并行时序图](docs/diagrams/runtime-sequence.html) · [文档索引](docs/README.md)

## 接入库

适配代码集中在本仓库，Slint 和厂商 SDK 是外部依赖。当前从 [Slint 仓库](http://192.168.253.238/wshl/slint-last.git) 获取同源 Slint / 软件 renderer，依赖清单不指定提交哈希；该源码包含所需扩展，不能只凭相同版本号替换为另一个来源。首次构建前联网执行 `cargo fetch --locked`，之后可使用文档中的离线构建命令。

```toml
[dependencies]
slint-bridge = { path = "/home/dpower/data/OMNI_STACK_GEN/slint_bridge", default-features = false, features = ["nx5"] }
slint = { version = "=1.18.1", git = "http://192.168.253.238/wshl/slint-last.git", default-features = false, features = ["std", "renderer-software", "compat-1-18"] }
```

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
| `nx5` | NX5 VO 初始化、软件渲染与双页显示 |
| `nx5-tde` | 包含 `nx5`，加速大块 RGBA 复制和透明混合；另有 QuickFill 诊断开关 |

必须选择一个板级 feature；`fbdev` 与 `nx5` 互斥。NX5 按 SDK/驱动接口和双页布局运行，不比对内核版本、驱动名称或文件摘要；`fbdev` 仅准入已验证的 X9 Lombo 固件与布局，其他 framebuffer 仍拒绝。SDK 目录由系统动态库搜索路径决定，见 [平台说明](docs/software-platform.md#准入边界)。

`Config::frame_interval` 是最小渲染启动间隔，默认 16667 微秒；没有 NX5 专用 30 FPS 钳制。当前驱动的实际提交上限约为 29 FPS，原因及尚未实现的单周期改动见 [驱动调查](docs/nx5-single-cycle.md)。

同时最多一次翻页提交和一帧预渲染。提交完成前不改写两张物理显示页，成功后才复用旧页；不提供单缓冲或忽略完成错误的回退。framebuffer 与触摸设备路径由 `Config` 指定，分辨率和行跨度读取设备当前模式；接口、像素格式和输入范围见 [平台说明](docs/software-platform.md)。

## 测试与用法示例

从 [test-app/src/bin/basic.rs](test-app/src/bin/basic.rs) 查看最小接入方式，从 [test-app/ui/](test-app/ui/app.slint) 查看独立界面文件。完整用例、参数及设备配置见 [测试工程说明](test-app/README.md)。

完整界面展示使用 `--bin showcase`：包含总览、控件/表单、设备列表、动画、图像/透明、交互记录、运行时图片缩放和富内容长列表八页。主机原生 MCP 预览、自动交互检查及 NX5 / X9 运行方式见 [展示应用说明](test-app/README.md#展示应用-showcase)；`display` 保留为原有诊断场景。

图片背景中的多个透明区域使用独立 `--bin background-holes` 演示：应用组件只裁剪背景，前景控件和弹窗正常叠加，见 [接入与约束](test-app/README.md#图片背景中的多个透明区域) 及 [NX5 实测](docs/nx5-background-holes.md)。

```sh
# 独立测试工程的公共接口回归。
cargo test --locked --offline -p slint-bridge-test-app --features fbdev

# 整个 workspace 的私有/公共回归。
cargo test --locked --offline --workspace --features nx5-tde
python3 test-app/tools/check-feature-matrix.py

# 独立测试程序的 ARM 构建。
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
cargo --config .cargo/nx5.toml build --locked --offline --release \
  -p slint-bridge-test-app --features nx5-tde \
  --target arm-unknown-linux-gnueabi --bin display
```

`--offline` 复用当前缓存；新机器需先获取锁定依赖。工具链配置保留在根 `.cargo/`。库被其他 workspace 引用时，消费方仍须配置自己的交叉链接器。

UI 检查使用 [Slint 原生 MCP](docs/slint-mcp.md)：元素查询、截图、交互与事件录制。底层页生命周期、evdev 解码、TDE 故障注入和实际输出采样继续使用相应回归，不能用 UI 截图代替。

当前设备记录及测试命令分别见 [NX5](test-app/devices/nx5/README.md) 和 [X9](test-app/devices/x9/README.md)。历史报告由 [docs/README.md](docs/README.md) 导航；报告引用的 `evidence/` 原始证据作为本地档案保存，不纳入 Git，检出仓库后需另行获取。
