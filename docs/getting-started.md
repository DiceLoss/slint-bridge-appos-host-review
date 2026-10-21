# Slint Bridge 获取配置与使用

本指南用于在新的 Linux 开发机上获取工程、配置交叉编译环境、构建 NX5、NX1、X9 和 F1 示例，以及在业务应用中引用适配层。四板共用同一个 Slint 来源，使用各自的板级 feature 和 ARM / RISC-V 工具链；下文业务模板支持四板切换。设备运行只适用于已经准入验证的硬件与固件，详细契约以 [平台说明](software-platform.md) 为准。

仅在自己的业务工程中使用适配层时，直接按 [业务接入](#在自己的-rust-应用中接入) 配置 Git 依赖即可，无需手动克隆或把适配层源码放入项目目录。前面的仓库克隆步骤用于构建仓库自带示例。

## 按设备选择配置

一次构建需要同时匹配 **Cargo feature、交叉工具链/target、运行时输入与旋转**。feature 选择板级实现，不会自动选择编译器；`Config` 配置设备路径，不会切换板级实现。下表对应已验证设备，完整准入条件见 [平台说明](software-platform.md#准入边界)。

| 设备 | feature | 仓库构建配置 | Rust target | 触摸 / 顺时针旋转 | 运行依赖与目录 |
| --- | --- | --- | --- | --- | --- |
| [NX5](../test-app/devices/nx5/README.md) | `nx5-tde`；纯软件用 `nx5` | `.cargo/nx5.toml` + 自动读取的 `.cargo/config.toml` | `arm-unknown-linux-gnueabi` | `/dev/input/event0` / 0° | `libVideoDec.so`、字体；按 NX5 部署工具管理目录 |
| [NX1](../test-app/devices/nx1/README.md) | `nx1` | `.cargo/nx1.toml` | `arm-unknown-linux-gnueabi` | `/dev/input/event2` / 0° | 字体；已核对挂载的 `/mnt` |
| [X9](../test-app/devices/x9/README.md) | `fbdev` | `.cargo/x9.toml` | `arm-unknown-linux-gnueabihf` | `/dev/input/event1` / 90° | 字体；无厂商 SDK，需 DRM 完成事件 |
| [F1](../test-app/devices/f1/README.md) | `f1-g2d`；纯软件渲染用 `f1` | `.cargo/f1.toml` | `riscv64gc-unknown-linux-gnu` | `/dev/input/event0` / 0° | 字体；真实 SD 卡挂载 `/tmp/sdcard`，不加载 MPP SDK |

当前四板均使用 `/dev/fb0`；NX5/NX1/F1 的示例面板为 1024×600，X9 物理 800×1280、旋转后逻辑 1280×800。分辨率与 stride 由库读取当前显示模式，不在 `Config` 中填写。无触摸时把 `inputs` 设为空；设备节点变化时按实际输入设备调整。

NX5 与 NX1 共用 Rust target 名称，但工具链分别位于 `/opt/X5`、`/opt/NX1`，不能只改 feature 而沿用另一板的链接器。X9 使用 hard-float ABI，F1 使用 RISC-V LP64D，均不可混用。NX1/F1 的参考翻页约定及验证限制保持不变。

## 仓库地址

| 仓库 | 地址 | 用途 |
| --- | --- | --- |
| 适配层 | [slint_bridge](https://github.com/omni-stack-gen/slint_bridge) | Rust 适配库、测试应用、设备工具、NX5 SDK 资源和字体 |
| Slint | [slint-last](http://192.168.253.238/wshl/slint-last.git) | 与适配层配套的 Slint 运行库、软件 renderer 和构建工具 |

配套 Slint 更新须先发布，再发布引用它的桥接层及锁文件；仅有本地 Slint 提交不能供新机器 `cargo fetch --locked` 获取。

适配层仓库使用 GitHub SSH 地址 `git@github.com:omni-stack-gen/slint_bridge.git`，开发机需配置具有仓库访问权限的 SSH 密钥。Slint 依赖仍位于 `192.168.253.238`，首次获取或更新依赖仍需内网连通性及对应 Git 凭据；本次迁移不改变 Slint 来源。不要把密码、私钥或令牌写入仓库。可用下面的只读命令分别检查访问权限：

```sh
git ls-remote git@github.com:omni-stack-gen/slint_bridge.git HEAD
git ls-remote http://192.168.253.238/wshl/slint-last.git HEAD
```

## 准备开发环境

安装 Git、Rust 1.92 或更高版本、主机 C 编译器及 Python 3.11+。部署设备时还需要 ADB 和可执行设备命令的 root 会话；进行像素分析时按相关工具说明安装 Pillow、NumPy。

已有 rustup 后，为当前 Rust 工具链安装所需组件和 target：

```sh
rustup component add rustfmt clippy
rustup target add arm-unknown-linux-gnueabi arm-unknown-linux-gnueabihf riscv64gc-unknown-linux-gnu
rustc --version
cargo --version
```

Rust target 提供 Rust 标准库，不能替代板级 C/C++ 交叉工具链及 sysroot。

| 设备 | Rust target | ABI | 当前工具链安装目录 |
| --- | --- | --- | --- |
| NX5 | `arm-unknown-linux-gnueabi` | soft-float ABI，Cortex-A7 | `/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi` |
| NX1 | `arm-unknown-linux-gnueabi` | soft-float ABI，Cortex-A7 | `/opt/NX1/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi` |
| X9 | `arm-unknown-linux-gnueabihf` | hard-float ABI，Cortex-A7 | `/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf` |
| F1 | `riscv64gc-unknown-linux-gnu` | LP64D，T-HEAD | `/opt/F1` |

安装匹配的厂商工具链及 sysroot。不要把 NX5 的 `gnueabi` 和 X9 的 `gnueabihf` 混用，也不要只把一个不同版本的编译器改成相同文件名。

## 获取示例工程和依赖

```sh
git clone git@github.com:omni-stack-gen/slint_bridge.git
cd slint_bridge
cargo fetch --locked
(cd third-party && sha256sum -c SHA256SUMS)
```

已有克隆可执行 `git remote set-url origin git@github.com:omni-stack-gen/slint_bridge.git` 更新远端，并用 `git remote -v` 核对。

正常构建不需要另行克隆 Slint，Cargo 会从配置的 Slint Git 地址获取源码。`Cargo.toml` 不设置 `rev` 或 tag；`Cargo.lock` 自动记录本次解析的提交，使 `--locked` 和离线构建可复现。普通构建不会自动升级到远端每个新提交。

如果 Cargo 获取私有 Git 依赖时需要复用已经配置好的 Git CLI 凭据，可执行：

```sh
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo fetch --locked
```

依赖下载完成后可以使用后文的 `--offline` 命令。新电脑直接运行离线构建而没有先获取依赖，会报缓存缺失。

仓库已经保存以下资源，无需准备原开发机上的其他项目目录：

| 目录 | 内容 |
| --- | --- |
| `third-party/nx5/include/` | TDE/MMZ、WBC 所需头文件及传递依赖，VO FFI 的 ABI 参考 |
| `third-party/nx5/lib/` | `libVideoDec-1.6.6.so` 和相对别名 `libVideoDec.so` |
| `third-party/fonts/` | `DejaVuSans.ttf` 和随字体部署的版权说明 |
| `test-app/ui/assets/` | 已入库的 PNG、SVG 和素材生成脚本 |

资源来源、摘要和覆盖变量见 [资源说明](../third-party/README.md)。交叉工具链、系统库、Cargo 缓存及历史测试证据不包含在这些资源中。

## 配置交叉编译器

默认安装目录与上表一致时，可直接使用仓库配置：

| 配置文件 | 作用 |
| --- | --- |
| [.cargo/config.toml](../.cargo/config.toml) | NX5 的 linker、CC、CXX、AR 路径 |
| [.cargo/nx5.toml](../.cargo/nx5.toml) | NX5 Cortex-A7、NEON 和 softfp 编译参数 |
| [.cargo/nx1.toml](../.cargo/nx1.toml) | NX1 工具链路径、Cortex-A7、NEON 和 softfp 参数 |
| [.cargo/x9.toml](../.cargo/x9.toml) | X9 工具链路径、Cortex-A7、NEON 和 hard-float 参数 |
| [.cargo/f1.toml](../.cargo/f1.toml) | F1 RISC-V 工具链路径与配套 LP64D sysroot |

如果工具链安装在其他目录，修改对应配置文件中的 `linker`、`CC_*`、`CXX_*`、`AR_*` 四类路径，保持 CPU 和 ABI 参数不变。NX5 的路径位于 `config.toml`，其余设备分别位于 `nx1.toml`、`x9.toml`、`f1.toml`。可以先直接执行配置中 gcc 的 `--version`，确认程序能在开发机运行。

ARM `nx5-tde` 默认使用仓库内头文件，不需要设置 `NX5_SDK_INCLUDE`。只有显式测试另一套配套 SDK 时才覆盖该变量。独立 NX5 C 工具支持 `NX5_CC`，但这个变量不会替代 Cargo 的交叉编译器配置。

## 验证并构建示例

以下命令都从适配层仓库根目录运行。选择一个板级 feature：

| feature | 用途 |
| --- | --- |
| `fbdev` | X9 当前已验证的纯 CPU framebuffer 路径 |
| `nx1` | NX1 纯 CPU framebuffer 与明确选择的参考 PAN 约定 |
| `nx5` | NX5 显示会话及软件渲染 |
| `nx5-tde` | NX5 TDE 加速和软件回退，包含 `nx5` |
| `nx5-mma-background` | 包含 `nx5-tde`；通过 Config 显式选择 MMA 背景 |
| `f1` / `f1-g2d` | F1 参考 PAN/VSYNC；后者包含 GE 填充与受限图片混合 |

`fbdev`、`nx1`、`nx5`、`f1` 两两互斥，不使用 `--all-features`。
NX1 的完整构建和 `/mnt` 部署命令见 [设备说明](../test-app/devices/nx1/README.md)；
F1 的 RISC-V 构建及 SD 卡部署见 [设备说明](../test-app/devices/f1/README.md)。
两板的参考翻页约定不代表旧页硬件退休已被证明。先运行主机检查：

```sh
python3 test-app/tools/check-feature-matrix.py
cargo fmt --all --check
cargo clippy --locked --offline --workspace --features nx5-tde,slint/mcp --all-targets -- -D warnings
sh test-app/tools/nx5/tde/check-native.sh
```

主机检查不占用设备。真实 framebuffer/TDE 的 ignored 测试需要独占设备，按设备文档单独执行。

构建 NX5 展示程序：

```sh
SLINT_EMIT_DEBUG_INFO=1 cargo --config .cargo/nx5.toml build --locked --offline --release \
  --target arm-unknown-linux-gnueabi --features nx5-tde,slint/mcp \
  -p slint-bridge-test-app --bin showcase
```

构建 X9 展示程序：

```sh
SLINT_EMIT_DEBUG_INFO=1 cargo --config .cargo/x9.toml build --locked --offline --release \
  --target arm-unknown-linux-gnueabihf --features fbdev,slint/mcp \
  -p slint-bridge-test-app --bin showcase
```

构建 NX1 展示程序：

```sh
SLINT_EMIT_DEBUG_INFO=1 cargo --config .cargo/nx1.toml build --locked --offline --release \
  --target arm-unknown-linux-gnueabi --features nx1,slint/mcp \
  -p slint-bridge-test-app --bin showcase
```

构建 F1 展示程序（纯软件渲染将 `f1-g2d` 改为 `f1`）：

```sh
SLINT_EMIT_DEBUG_INFO=1 cargo --config .cargo/f1.toml build --locked --offline --release \
  --target riscv64gc-unknown-linux-gnu --features f1-g2d,slint/mcp \
  -p slint-bridge-test-app --bin showcase
```

输出均位于 `target/<Rust target>/release/showcase`。NX5/NX1 共用输出路径，切换设备构建前应另存需要部署或回退的程序副本；不要仅凭文件名判断目标设备。`slint/mcp` 和 `SLINT_EMIT_DEBUG_INFO=1` 用于开发时的原生 UI 查询；不需要 MCP 时可去掉这两项。其他示例及参数见 [测试工程说明](../test-app/README.md)。

## 配置设备运行环境

上传前重新核对设备身份、显示资源所有者以及原程序的恢复方式。停止已确认的旧显示程序并等待资源释放后，再运行新程序；不要照抄历史记录中的 PID。新程序和资源放到新目录，先验证有限时长正常退出，再决定常驻。

### NX5

NX5 部署使用 [归档和上传工具](../test-app/tools/nx5/README.md#部署归档清理和截图)。工具依据实际内存余量和保留策略归档旧文件、保护当前/恢复程序，再上传新文件；同一事务只执行一次 `upload` 或 `finish`。首次接入设备必须先建立有效的当前/恢复文件与保留策略，不能绕过缺少恢复文件的检查。

为新部署目录准备下列文件。SDK 和字体可使用已核对的设备共用路径，不必每轮重复上传：

| 主机文件 | 设备用途 |
| --- | --- |
| NX5 构建的 `showcase` | 新目录中的可执行程序；按设备说明使用对应 strip 工具制作部署副本 |
| `third-party/nx5/lib/libVideoDec-1.6.6.so` | 新目录的 `lib/libVideoDec-1.6.6.so` |
| `third-party/fonts/DejaVuSans.ttf` | 新目录的 `DejaVuSans.ttf` |
| `third-party/fonts/DejaVu-COPYRIGHT` | 新目录的 `DejaVu-COPYRIGHT` |

SDK 目录还需 `libVideoDec.so` 相对软链接，指向同目录的 `libVideoDec-1.6.6.so`。按上传工具流程逐文件部署后，在设备 root 会话中设置实际路径。以下是目录布局示例，使用前替换为本轮已部署的新目录：

```sh
NX5_RUN_DIR=/tmp/slint-bridge-demo-001
# 仅在该新目录已部署版本库、且别名尚不存在时创建。
ln -s libVideoDec-1.6.6.so "$NX5_RUN_DIR/lib/libVideoDec.so"
SLINT_DEFAULT_FONT="$NX5_RUN_DIR/DejaVuSans.ttf" \
LD_LIBRARY_PATH="$NX5_RUN_DIR/lib" SLINT_MCP_PORT=9315 \
"$NX5_RUN_DIR/showcase" --framebuffer=/dev/fb0 --touch=/dev/input/event0 --seconds=20
```

如复用原有 SDK/字体目录，相应修改两个环境变量。SDK 由显示进程动态加载；TDE 和 WBC 复用同一会话，不能同时启动另一个加载 SDK 的采集进程。完整操作及资源释放判据见 [NX5 设备说明](../test-app/devices/nx5/README.md)。

### X9

X9 不使用 NX5 SDK。部署 X9 构建的程序、字体和版权说明到新目录，通过已核对的设备 root 会话运行。当前已验证配置为物理 800×1280、顺时针旋转 90°、逻辑 1280×800，触摸路径为 `/dev/input/event1`：

```sh
X9_RUN_DIR=/tmp/slint-bridge-demo-001
SLINT_DEFAULT_FONT="$X9_RUN_DIR/DejaVuSans.ttf" SLINT_MCP_PORT=9315 \
"$X9_RUN_DIR/showcase" --framebuffer=/dev/fb0 --touch=/dev/input/event1 --rotation=90 --seconds=20
```

该配置只对应 [X9 设备说明](../test-app/devices/x9/README.md) 中的已验证板型。X9 的内存字段和显示恢复流程与 NX5 不同，不直接套用 NX5 部署脚本。

### NX1

程序、字体及字体版权部署到已核对挂载的 `/mnt` 新目录。先按 [NX1 设备说明](../test-app/devices/nx1/README.md#部署与恢复) 核对显示所有者和原应用看门狗，再在 root 会话中运行；下例目录须替换为实际部署目录：

```sh
NX1_RUN_DIR=/mnt/slint-bridge-demo-001
SLINT_DEFAULT_FONT="$NX1_RUN_DIR/DejaVuSans.ttf" SLINT_MCP_PORT=9315 \
"$NX1_RUN_DIR/showcase" --framebuffer=/dev/fb0 --touch=/dev/input/event2 --rotation=0 --seconds=20
```

### F1

程序、字体及版权部署到真实 SD 卡挂载 `/tmp/sdcard` 的新目录。先按 [F1 设备说明](../test-app/devices/f1/README.md#sd-卡部署与所有权) 核对所有权和挂载，保留配套 control_service；下例将构建产物命名为 `showcase`：

```sh
F1_RUN_DIR=/tmp/sdcard/slint-bridge-demo-001
SLINT_DEFAULT_FONT="$F1_RUN_DIR/DejaVuSans.ttf" SLINT_MCP_PORT=9315 \
"$F1_RUN_DIR/showcase" --framebuffer=/dev/fb0 --touch=/dev/input/event0 --rotation=0 --seconds=20
```

以上两板不加载 NX5 SDK，不套用 NX5 的 tmpfs 部署流程。通过旧 adbd 启动后台进程时，使用设备说明中的 `run-detached`，不要把普通 `&` 当作断连后仍可运行的保证。

### 原生 MCP 连接

只有启用了 MCP 的构建才使用下面的连接。主机设置实际 ADB 地址后转发端口，两台设备使用不同的主机端口：

```sh
# 以下地址是现有测试环境示例，先核对再替换。
NX5_SERIAL=192.168.1.151:5555
X9_SERIAL=192.168.1.143:5555
adb -s "$NX5_SERIAL" forward tcp:19315 tcp:9315
adb -s "$X9_SERIAL" forward tcp:19316 tcp:9315
```

NX5 endpoint 为 `http://127.0.0.1:19315/mcp`，X9 为 `http://127.0.0.1:19316/mcp`。查询、交互和截图步骤见 [Slint MCP 指南](slint-mcp.md)；截图前按设备说明检查可用内存。

## 在自己的 Rust 应用中接入

业务工程直接声明两个 Git 依赖；Cargo 将源码下载到自己的 Git 缓存，不要求业务目录中存在 `slint-bridge/`，也不需要 `path` 依赖。下面的应用默认选择 NX5 TDE；NX1、X9、F1 构建时关闭默认 feature，再选择对应板级 feature。所有 Slint 包使用同一 Git 地址，不设置提交哈希。

### Cargo.toml

```toml
[package]
name = "panel-app"
version = "0.1.0"
edition = "2024"
rust-version = "1.92"

[features]
default = ["nx5-tde"]
nx5 = ["slint-bridge/nx5"]
nx5-tde = ["nx5", "slint-bridge/nx5-tde"]
fbdev = ["slint-bridge/fbdev"]
nx1 = ["slint-bridge/nx1"]
f1 = ["slint-bridge/f1"]
f1-g2d = ["f1", "slint-bridge/f1-g2d"]

[dependencies]
slint-bridge = { git = "ssh://git@github.com/omni-stack-gen/slint_bridge.git", default-features = false }
slint = { version = "=1.18.1", git = "http://192.168.253.238/wshl/slint-last.git", default-features = false, features = ["std", "renderer-software", "compat-1-18"] }

[build-dependencies]
slint-build = { version = "=1.18.1", git = "http://192.168.253.238/wshl/slint-last.git" }

[profile.release]
lto = "thin"
codegen-units = 1
```

`"slint-bridge/nx5-tde"` 中的斜杠表示“依赖包 / feature”，不是目录。它引用同一清单 `[dependencies]` 中的 Git 包；仅定义 feature 名称不会启用它，还需要 `default` 或构建参数选择。

本例默认 `nx5-tde`，因此不传 `--features` 也会选择 NX5 TDE。转发同时启用应用自身的 `nx5`，与测试工程的包含关系一致；适配库和测试工程自身仍为 `default = []`，不能把业务模板的默认值套到前面的仓库构建命令。若希望每次构建显式选板，可把应用的 `default` 改回 `[]`，并始终传 `--features nx5-tde` 或 `--features fbdev`。适配库本身仍不默认选择设备。仅使用 NX5 的应用也可以不声明这些转发项，直接在 `slint-bridge` 依赖中写 `features = ["nx5-tde"]`。

Cargo 的 `git` 字段使用完整 SSH URL `ssh://git@github.com/omni-stack-gen/slint_bridge.git`；`git clone` 和远端配置使用上面的 `git@github.com:...` 写法。已有业务工程需修改适配层依赖地址并更新自己的锁文件，Slint 依赖地址保持不变。

### build.rs

```rust
fn main() {
    slint_build::compile("ui/main.slint").expect("compile application UI");
}
```

### ui/main.slint

```slint
export component MainWindow inherits Window {
    background: #14202b;
    Text { text: "Slint Bridge"; color: white; font-size: 24px; }
}
```

### src/main.rs

```rust
use slint::ComponentHandle;
use slint_bridge::{Config, InputDevice, Rotation};

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (touch, rotation) = if cfg!(feature = "fbdev") {
        ("/dev/input/event1", Rotation::Clockwise90)
    } else if cfg!(feature = "nx1") {
        ("/dev/input/event2", Rotation::None)
    } else {
        ("/dev/input/event0", Rotation::None)
    };
    slint_bridge::init(Config {
        framebuffer: "/dev/fb0".into(),
        inputs: vec![InputDevice::touchscreen(touch)],
        rotation,
        ..Config::default()
    })?;
    MainWindow::new()?.run()?;
    Ok(())
}
```

这是上述四板已验证设备的示例配置；NX5/F1 共用 event0、无旋转这一分支。实际业务按目标设备设置路径和旋转；无触摸设备时将 `inputs` 设为空。创建 Slint 窗口前先调用 `slint_bridge::init`。

### 业务工程的交叉编译配置

**Git 依赖中的 `.cargo/` 不会自动成为业务工程的交叉编译配置。** 在业务工程根目录创建 `.cargo/`，按目标设备保存下面的独立配置文件；不需要克隆适配层仓库。这里的 NX5 文件已包含完整链接器路径，与本仓库中拆分的 `config.toml` + `nx5.toml` 等效。

通过 `cargo --config .cargo/<设备>.toml` 显式选择一个文件。此模板不需要在 `.cargo/config.toml` 中重复设置板级工具链；已有同名 target 或环境覆盖时先核对，尤其不要同时加载 NX5/NX1 的配置。工具链安装目录不同则一并修改 linker、CC/CXX/AR 路径，并保留 ABI 参数。SDK 头文件仍由适配库从 Git 依赖目录定位。

### .cargo/nx5.toml

```toml
[target.arm-unknown-linux-gnueabi]
linker = "/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc"
rustflags = ["-C", "target-cpu=cortex-a7"]

[env]
CC_arm_unknown_linux_gnueabi = "/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc"
CXX_arm_unknown_linux_gnueabi = "/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-g++"
AR_arm_unknown_linux_gnueabi = "/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-ar"
CFLAGS_arm_unknown_linux_gnueabi = "-march=armv7-a -mtune=cortex-a7 -mfpu=neon-vfpv4 -mfloat-abi=softfp"
```

### .cargo/nx1.toml

```toml
# NX1 / sun8i: Cortex-A7, soft-float ABI, CPU rendering into fbdev.
[target.arm-unknown-linux-gnueabi]
linker = "/opt/NX1/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc"
rustflags = ["-C", "target-cpu=cortex-a7"]

[env]
CC_arm_unknown_linux_gnueabi = "/opt/NX1/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc"
CXX_arm_unknown_linux_gnueabi = "/opt/NX1/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-g++"
AR_arm_unknown_linux_gnueabi = "/opt/NX1/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-ar"
CFLAGS_arm_unknown_linux_gnueabi = "-march=armv7-a -mtune=cortex-a7 -mfpu=neon-vfpv4 -mfloat-abi=softfp"
```

### .cargo/x9.toml

```toml
# X9 / Lombo N7: Cortex-A7, hard-float ABI, software framebuffer rendering.
[target.arm-unknown-linux-gnueabihf]
linker = "/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf/bin/arm-linux-gnueabihf-gcc"
rustflags = ["-C", "target-cpu=cortex-a7"]

[env]
CC_arm_unknown_linux_gnueabihf = "/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf/bin/arm-linux-gnueabihf-gcc"
CXX_arm_unknown_linux_gnueabihf = "/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf/bin/arm-linux-gnueabihf-g++"
AR_arm_unknown_linux_gnueabihf = "/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf/bin/arm-linux-gnueabihf-ar"
CFLAGS_arm_unknown_linux_gnueabihf = "-march=armv7-a -mtune=cortex-a7 -mfpu=neon-vfpv4 -mfloat-abi=hard"
```

### .cargo/f1.toml

```toml
# F1 / ArtInChip D211: T-HEAD RISC-V 64-bit, LP64D ABI.
# Use the matching vendor sysroot and loader; do not link against host libraries.
[target.riscv64gc-unknown-linux-gnu]
linker = "/opt/F1/bin/riscv64-unknown-linux-gnu-gcc"

[env]
CC_riscv64gc_unknown_linux_gnu = "/opt/F1/bin/riscv64-unknown-linux-gnu-gcc"
CXX_riscv64gc_unknown_linux_gnu = "/opt/F1/bin/riscv64-unknown-linux-gnu-g++"
AR_riscv64gc_unknown_linux_gnu = "/opt/F1/bin/riscv64-unknown-linux-gnu-ar"
```

从业务工程根目录首次执行 `cargo fetch`，生成并提交业务工程自己的 `Cargo.lock`。之后按目标设备构建：

```sh
# NX5 TDE，使用应用默认 feature。
cargo --config .cargo/nx5.toml build --locked --offline --release \
  --target arm-unknown-linux-gnueabi
# NX5 纯软件渲染。
cargo --config .cargo/nx5.toml build --locked --offline --release \
  --target arm-unknown-linux-gnueabi --no-default-features --features nx5
# NX1。
cargo --config .cargo/nx1.toml build --locked --offline --release \
  --target arm-unknown-linux-gnueabi --no-default-features --features nx1
# X9。
cargo --config .cargo/x9.toml build --locked --offline --release \
  --target arm-unknown-linux-gnueabihf --no-default-features --features fbdev
# F1 GE；纯软件渲染将 f1-g2d 改为 f1。
cargo --config .cargo/f1.toml build --locked --offline --release \
  --target riscv64gc-unknown-linux-gnu --no-default-features --features f1-g2d
```

输出程序名为 `panel-app`。这个最小例子没有测试工程的命令行参数解析，不接受 `--seconds`、`--touch` 或 `--rotation`；相关值来自上述 `Config`，需要参数支持时由业务应用自行添加。

## 更新代码与 Slint

在适配层仓库中拉取代码后，先获取更新后的锁定依赖：

```sh
git pull --ff-only
cargo fetch --locked
```

需要主动跟进 Slint 远端更新时执行：

```sh
cargo update -p slint
```

检查 `Cargo.lock` 中所有 Slint crate 仍来自同一个 Git 来源和提交，完成主机检查、ARM 构建及受影响路径的实机回归后提交锁文件。业务工程更新适配层 Git 依赖时使用 `cargo update -p slint-bridge`，其锁文件独立于适配层仓库。

如果需要修改 Slint 本身，再另行克隆：

```sh
git clone http://192.168.253.238/wshl/slint-last.git
```

编辑这个克隆目录不会自动影响 Cargo 的 Git 依赖；不要直接改 Cargo 缓存。将需要使用的修改提交并推送到 Slint 仓库后，再在消费工程更新依赖。升级 Slint 版本号时同步调整所有 `=1.18.1` 约束；补丁迁移约定见 [平台说明](software-platform.md#构建依赖)。

## 常见配置问题

| 现象 | 检查项 |
| --- | --- |
| Git/Cargo 不能访问仓库 | GitHub SSH 密钥及仓库权限、Slint 内网连通性与 Git 凭据；需要时使用 Git CLI 获取依赖 |
| 离线构建提示找不到依赖或提交 | 先联网运行 `cargo fetch --locked`；首次新业务工程没有锁文件时使用 `cargo fetch` |
| 找不到交叉 gcc，或 ELF/浮点 ABI 不匹配 | 检查对应 target 的 linker、CC/CXX/AR 和完整 sysroot，确认没有混用 NX5/X9 工具链 |
| 报必须选择板级 feature 或 feature 互斥 | `default = []` 时必须传 `--features`；已有默认设备时，切换设备或模式使用 `--no-default-features --features ...`。不要用 `--all-features`；这类错误不表示缺少本地源码目录 |
| 找不到 `mpi_tde.h` | 检查 `third-party/` 是否完整；清除指向旧机器目录的 `NX5_SDK_INCLUDE` 覆盖 |
| NX5 找不到 `libVideoDec.so` | 检查设备上的版本库、无版本别名及 `LD_LIBRARY_PATH`，不要填主机仓库路径 |
| NX5 报 VO 已启用，或设备资源被占用 | 重新核对显示所有者并按恢复流程释放，不并行启动第二个 SDK 显示程序 |
| 字体缺失或画面对照不同 | 部署仓库中的字体及版权说明，并明确设置设备上的 `SLINT_DEFAULT_FONT` |
| MCP 无法连接 | 检查编译 feature、运行端口、程序状态、ADB 目标和转发端口 |

进一步的设备参数、部署资源保护及故障处理，分别见 [NX5](../test-app/devices/nx5/README.md)、[NX1](../test-app/devices/nx1/README.md)、[X9](../test-app/devices/x9/README.md)、[F1](../test-app/devices/f1/README.md) 和 [完整应用说明](../test-app/README.md)。
