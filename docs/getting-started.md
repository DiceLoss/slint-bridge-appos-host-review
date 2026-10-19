# Slint Bridge 获取配置与使用

本指南用于在新的 Linux 开发机上获取工程、配置交叉编译环境、构建 NX5/X9 示例，以及在业务应用中引用适配层。NX5 和 X9 共用同一个 Slint 来源，使用不同的板级 feature 和 ARM 工具链。设备运行只适用于已经准入验证的硬件与固件，详细契约以 [平台说明](software-platform.md) 为准。

仅在自己的业务工程中使用适配层时，按[业务接入](#在自己的-rust-应用中接入)准备顶层 Cargo 覆盖。本方案只需一个私有仓库，不需要私有 Slint fork；官方源码仍会下载到本地。

## 仓库地址

| 仓库 | 地址 | 用途 |
| --- | --- | --- |
| 适配层 | [slint_bridge](http://192.168.253.238/wshl/slint_bridge.git) | 库、测试应用、SDK、字体、Slint 补丁和准备脚本 |
| Slint 官方源码 | [slint-ui/slint](https://github.com/slint-ui/slint) | 固定 1.18.1，具体提交和校验树由 `patches/slint/manifest.json` 管理 |

首次准备需要访问适配层内网仓库、GitHub 及 Cargo registry。不要把凭据写入清单或仓库 URL。

## 准备开发环境

安装 Git、Rust 1.92 或更高版本、主机 C 编译器及 Python 3.11+。部署设备时还需要 ADB 和可执行设备命令的 root 会话；进行像素分析时按相关工具说明安装 Pillow、NumPy。

已有 rustup 后，为当前 Rust 工具链安装所需组件和 target：

```sh
rustup component add rustfmt clippy
rustup target add arm-unknown-linux-gnueabi arm-unknown-linux-gnueabihf
rustc --version
cargo --version
```

Rust target 提供 Rust 标准库，不能替代板级 C/C++ 交叉工具链及 sysroot。

| 设备 | Rust target | ABI | 当前工具链安装目录 |
| --- | --- | --- | --- |
| NX5 | `arm-unknown-linux-gnueabi` | soft-float ABI，Cortex-A7 | `/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi` |
| X9 | `arm-unknown-linux-gnueabihf` | hard-float ABI，Cortex-A7 | `/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf` |

安装匹配的厂商工具链及 sysroot。不要把 NX5 的 `gnueabi` 和 X9 的 `gnueabihf` 混用，也不要只把一个不同版本的编译器改成相同文件名。

## 获取示例工程和依赖

```sh
git clone http://192.168.253.238/wshl/slint_bridge.git
cd slint_bridge
python3 tools/prepare-slint.py
cargo fetch --locked
(cd third-party && sha256sum -c SHA256SUMS)
```

准备脚本先校验补丁 SHA256，再取得官方固定提交、核验原始源码树、应用补丁并核验结果树，最后发布到 `.slint/source`。该目录及生成的 `.slint/cargo-patch.toml` 不入库。本仓库 `.cargo/config.toml` 已包含对应的相对路径覆盖，因此完成准备后可直接运行 Cargo。

重复执行会校验并复用源码，不重复下载或打补丁。`python3 tools/prepare-slint.py --offline` 不联网；源码尚未准备时明确失败。源码被修改、补丁摘要错误或版本不匹配时保留现有文件并报错，不自动覆盖。准备与发布使用本地锁和临时目录，失败时不发布半成品。

Cargo registry 依赖仍需通过 `cargo fetch --locked` 获取；准备 Slint 不等于下载了所有依赖。两者完成后可使用后文的离线构建命令。不要修改 Cargo 全局源码缓存，也不要用 `build.rs` 代替构建前准备。

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
| [.cargo/x9.toml](../.cargo/x9.toml) | X9 工具链路径、Cortex-A7、NEON 和 hard-float 参数 |

如果工具链安装在其他目录，修改对应配置文件中的 `linker`、`CC_*`、`CXX_*`、`AR_*` 四类路径，保持 CPU 和 ABI 参数不变。NX5 的路径位于 `config.toml`，X9 的路径位于 `x9.toml`。可以先直接执行配置中 gcc 的 `--version`，确认程序能在开发机运行。

ARM `nx5-tde` 默认使用仓库内头文件，不需要设置 `NX5_SDK_INCLUDE`。只有显式测试另一套配套 SDK 时才覆盖该变量。独立 NX5 C 工具支持 `NX5_CC`，但这个变量不会替代 Cargo 的交叉编译器配置。

## 验证并构建示例

以下命令都从适配层仓库根目录运行。选择一个板级 feature：

| feature | 用途 |
| --- | --- |
| `fbdev` | X9 当前已验证的纯 CPU framebuffer 路径 |
| `nx5` | NX5 显示会话及软件渲染 |
| `nx5-tde` | NX5 TDE 加速和软件回退，包含 `nx5` |

`fbdev` 与 `nx5` 互斥，不使用 `--all-features`。先运行主机检查：

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

对应输出分别为 `target/arm-unknown-linux-gnueabi/release/showcase` 和 `target/arm-unknown-linux-gnueabihf/release/showcase`。`slint/mcp` 和 `SLINT_EMIT_DEBUG_INFO=1` 用于开发时的原生 UI 查询；不需要 MCP 时可去掉这两项。其他示例及参数见 [测试工程说明](../test-app/README.md)。

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

推荐将 `slint_bridge` 与 `panel-app` 放在同一父目录，业务工程通过 `path` 使用该 checkout，使库代码、准备脚本和补丁保持一致。先从 `panel-app` 目录执行：

```sh
# 已有相邻 checkout 时无需重复 clone。
git clone http://192.168.253.238/wshl/slint_bridge.git ../slint_bridge
python3 ../slint_bridge/tools/prepare-slint.py
```

下例默认选择 NX5 TDE。X9 构建时关闭默认 feature 后选择 `fbdev`。所有 Slint 包使用精确版本和统一的本地补丁源码。

也可保留 `slint-bridge = { git = "http://192.168.253.238/wshl/slint_bridge.git", ... }`；准备脚本所在 checkout 必须与业务锁定的适配层版本一致，并同样传入生成的 `--config`。需要先发布本次适配层改动，才能从远端消费该版本。适配层依赖自身的 `[patch]` 或 `.cargo/config.toml` 不会自动传给业务根 workspace。

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

[dependencies]
slint-bridge = { path = "../slint_bridge", default-features = false }
slint = { version = "=1.18.1", default-features = false, features = ["std", "renderer-software", "compat-1-18"] }

[build-dependencies]
slint-build = { version = "=1.18.1" }

[profile.release]
lto = "thin"
codegen-units = 1
```

`"slint-bridge/nx5-tde"` 中的斜杠表示“依赖包 / feature”，不是目录。它引用同一清单 `[dependencies]` 中的 Git 包；仅定义 feature 名称不会启用它，还需要 `default` 或构建参数选择。

本例默认 `nx5-tde`，因此不传 `--features` 也会选择 NX5 TDE。转发同时启用应用自身的 `nx5`，与测试工程的包含关系一致；适配库和测试工程自身仍为 `default = []`，不能把业务模板的默认值套到前面的仓库构建命令。若希望每次构建显式选板，可把应用的 `default` 改回 `[]`，并始终传 `--features nx5-tde` 或 `--features fbdev`。适配库本身仍不默认选择设备。仅使用 NX5 的应用也可以不声明这些转发项，直接在 `slint-bridge` 依赖中写 `features = ["nx5-tde"]`。

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

这是两台已验证设备的示例配置。实际业务按目标设备设置路径和旋转；无触摸设备时将 `inputs` 设为空。创建 Slint 窗口前先调用 `slint_bridge::init`。

### 业务工程的交叉编译配置

**依赖库中的 `.cargo/` 不会自动成为业务工程的交叉编译配置。** 在业务工程中创建以下 `.cargo/config.toml`，与命令行传入的 Slint 覆盖配置并用。按实际安装位置修改工具链路径，保持两个 target 的 ABI 参数不变。SDK 头文件随适配层 checkout 或 Git 依赖提供，由库自身定位，不需要复制到业务工程。

### .cargo/config.toml

```toml
[target.arm-unknown-linux-gnueabi]
linker = "/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc"
rustflags = ["-C", "target-cpu=cortex-a7"]

[target.arm-unknown-linux-gnueabihf]
linker = "/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf/bin/arm-linux-gnueabihf-gcc"
rustflags = ["-C", "target-cpu=cortex-a7"]

[env]
CC_arm_unknown_linux_gnueabi = "/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc"
CXX_arm_unknown_linux_gnueabi = "/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-g++"
AR_arm_unknown_linux_gnueabi = "/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-ar"
CFLAGS_arm_unknown_linux_gnueabi = "-march=armv7-a -mtune=cortex-a7 -mfpu=neon-vfpv4 -mfloat-abi=softfp"
CC_arm_unknown_linux_gnueabihf = "/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf/bin/arm-linux-gnueabihf-gcc"
CXX_arm_unknown_linux_gnueabihf = "/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf/bin/arm-linux-gnueabihf-g++"
AR_arm_unknown_linux_gnueabihf = "/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf/bin/arm-linux-gnueabihf-ar"
CFLAGS_arm_unknown_linux_gnueabihf = "-march=armv7-a -mtune=cortex-a7 -mfpu=neon-vfpv4 -mfloat-abi=hard"
```

从业务工程根目录执行首次解析，生成并提交业务工程自己的 `Cargo.lock`：

```sh
cargo --config ../slint_bridge/.slint/cargo-patch.toml fetch
# NX5 TDE，使用应用默认 feature。
cargo --config ../slint_bridge/.slint/cargo-patch.toml build --locked --offline --release --target arm-unknown-linux-gnueabi
# X9，先关闭默认的 NX5 TDE。
cargo --config ../slint_bridge/.slint/cargo-patch.toml build --locked --offline --release --target arm-unknown-linux-gnueabihf \
  --no-default-features --features fbdev
# NX5 纯软件模式，同样先关闭默认的 TDE。
cargo --config ../slint_bridge/.slint/cargo-patch.toml build --locked --offline --release --target arm-unknown-linux-gnueabi \
  --no-default-features --features nx5
```

输出程序名为 `panel-app`。这个最小例子没有测试工程的命令行参数解析，不接受 `--seconds`、`--touch` 或 `--rotation`；相关值来自上述 `Config`，需要参数支持时由业务应用自行添加。

## 更新代码与 Slint

拉取适配层后先验证准备状态，再获取锁定依赖：

```sh
git pull --ff-only
python3 tools/prepare-slint.py
cargo fetch --locked
```

本方案固定官方 1.18.1；`cargo update -p slint` 不会下载另一版补丁源码。补丁或上游版本变化时，先保留需要的本地修改并将旧 `.slint/source` 移出，再重新准备。移动仓库目录后也应重跑准备脚本，刷新外部业务使用的绝对路径配置。

维护者更新 `patches/slint/` 中的补丁、基线提交、补丁摘要及源码树摘要；包映射变化时同步 `.cargo/config.toml`。`Cargo.lock` 记录依赖解析，官方提交及补丁清单保证 path 源码的身份，二者不能互相替代。验证脚本见 `python3 -m unittest discover -s tools/tests -v`。升级时检查上游已包含的修复，迁移剩余补丁；完成 feature 矩阵、ARM 构建和受影响路径的实机验证后再发布。

## 常见配置问题

| 现象 | 检查项 |
| --- | --- |
| 缺少 `.slint/source` 或 `external-pixel-buffer` | 先准备源码；外部业务检查是否漏传 `--config`，不要改用未经补丁的官方包 |
| Git/Cargo 不能访问仓库 | 内网适配层、GitHub、registry 的连通性及 Git 凭据 |
| 离线构建提示找不到依赖或提交 | 先运行准备脚本，再获取 Cargo 依赖；业务工程的命令还须携带生成的 `--config` |
| 找不到交叉 gcc，或 ELF/浮点 ABI 不匹配 | 检查对应 target 的 linker、CC/CXX/AR 和完整 sysroot，确认没有混用 NX5/X9 工具链 |
| 报必须选择板级 feature 或 feature 互斥 | `default = []` 时必须传 `--features`；已有默认设备时，切换设备或模式使用 `--no-default-features --features ...`。不要用 `--all-features`；这类错误不表示缺少本地源码目录 |
| 找不到 `mpi_tde.h` | 检查 `third-party/` 是否完整；清除指向旧机器目录的 `NX5_SDK_INCLUDE` 覆盖 |
| NX5 找不到 `libVideoDec.so` | 检查设备上的版本库、无版本别名及 `LD_LIBRARY_PATH`，不要填主机仓库路径 |
| NX5 报 VO 已启用，或设备资源被占用 | 重新核对显示所有者并按恢复流程释放，不并行启动第二个 SDK 显示程序 |
| 字体缺失或画面对照不同 | 部署仓库中的字体及版权说明，并明确设置设备上的 `SLINT_DEFAULT_FONT` |
| MCP 无法连接 | 检查编译 feature、运行端口、程序状态、ADB 目标和转发端口 |

进一步的设备参数、部署资源保护及故障处理，分别见 [NX5](../test-app/devices/nx5/README.md)、[X9](../test-app/devices/x9/README.md) 和 [完整应用说明](../test-app/README.md)。
