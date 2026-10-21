# Slint Bridge 获取配置与使用

本指南用于在自己的 Rust 业务工程中通过 Cargo Git 依赖接入适配层，并配置 NX5、NX1、X9 和 F1 的交叉编译及运行环境。四板共用同一个 Slint 来源，使用各自的板级 feature 和 ARM / RISC-V 工具链；下文业务模板支持四板切换。设备运行只适用于已经准入验证的硬件与固件，详细契约以 [平台说明](software-platform.md) 为准。

接入步骤为：在业务工程声明 [Cargo 依赖](#cargotoml)，保存目标设备的 [交叉编译配置](#业务工程的交叉编译配置)，再由 Cargo 获取依赖并构建。依赖源码和 SDK 头文件由 Cargo 管理；业务工程只包含自己的源码与配置。本文命令均针对用户自己的业务工程或目标设备。

## 按设备选择配置

一次构建需要同时匹配 **Cargo feature、交叉工具链/target、运行时输入与旋转**。feature 选择板级实现，不会自动选择编译器；`Config` 配置设备路径，不会切换板级实现。下表对应已验证设备，完整准入条件见 [平台说明](software-platform.md#准入边界)。

| 设备 | feature | 业务工程配置 | Rust target | 触摸 / 顺时针旋转 | 运行依赖与目录 |
| --- | --- | --- | --- | --- | --- |
| [NX5](../test-app/devices/nx5/README.md) | `nx5-tde`；纯软件用 `nx5` | [`.cargo/nx5.toml`](#cargonx5toml) | `arm-unknown-linux-gnueabi` | `/dev/input/event0` / 0° | `libVideoDec.so`、字体；独立应用目录 |
| [NX1](../test-app/devices/nx1/README.md) | `nx1` | [`.cargo/nx1.toml`](#cargonx1toml) | `arm-unknown-linux-gnueabi` | `/dev/input/event2` / 0° | 字体；已核对挂载的 `/mnt` |
| [X9](../test-app/devices/x9/README.md) | `fbdev` | [`.cargo/x9.toml`](#cargox9toml) | `arm-unknown-linux-gnueabihf` | `/dev/input/event1` / 90° | 字体；无厂商 SDK，需 DRM 完成事件 |
| [F1](../test-app/devices/f1/README.md) | `f1-g2d`；纯软件渲染用 `f1` | [`.cargo/f1.toml`](#cargof1toml) | `riscv64gc-unknown-linux-gnu` | `/dev/input/event0` / 0° | 字体；真实 SD 卡挂载 `/tmp/sdcard`，不加载 MPP SDK |

当前四板均使用 `/dev/fb0`；NX5/NX1/F1 的示例面板为 1024×600，X9 物理 800×1280、旋转后逻辑 1280×800。分辨率与 stride 由库读取当前显示模式，不在 `Config` 中填写。无触摸时把 `inputs` 设为空；设备节点变化时按实际输入设备调整。

NX5 与 NX1 共用 Rust target 名称，但工具链分别位于 `/opt/X5`、`/opt/NX1`，不能只改 feature 而沿用另一板的链接器。X9 使用 hard-float ABI，F1 使用 RISC-V LP64D，均不可混用。NX1/F1 的参考翻页约定及验证限制保持不变。

## 仓库地址

| 仓库 | 地址 | 用途 |
| --- | --- | --- |
| 适配层 | [slint_bridge](https://github.com/omni-stack-gen/slint_bridge) | Rust 适配库、测试应用、设备工具、NX5 SDK 资源和字体 |
| Slint | [slint-last](http://192.168.253.238/wshl/slint-last.git) | 与适配层配套的 Slint 运行库、软件 renderer 和构建工具 |

适配层仓库使用 GitHub SSH 地址 `git@github.com:omni-stack-gen/slint_bridge.git`，开发机需配置具有仓库访问权限的 SSH 密钥。Slint 依赖仍位于 `192.168.253.238`，首次获取或更新依赖仍需内网连通性及对应 Git 凭据；本次迁移不改变 Slint 来源。不要把密码、私钥或令牌写入仓库。可用下面的只读命令分别检查访问权限：

```sh
git ls-remote git@github.com:omni-stack-gen/slint_bridge.git HEAD
git ls-remote http://192.168.253.238/wshl/slint-last.git HEAD
```

## 准备开发环境

安装 Git、Rust 1.92 或更高版本、主机 C 编译器及 Python 3.11+。部署设备时使用设备支持的传输通道（例如 ADB）和可访问 framebuffer/input 的运行账户。

已有 rustup 后，为当前 Rust 工具链安装所需 target：

```sh
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

## 获取依赖

先在自己的业务工程中完成下文的 [Cargo.toml](#cargotoml)，然后从业务工程根目录执行：

```sh
# 首次解析，生成业务工程自己的 Cargo.lock。
cargo fetch
# 已有锁文件时，按锁定版本获取依赖。
cargo fetch --locked
```

Cargo 根据清单中的 Git 地址获取适配层与 Slint 依赖。`Cargo.toml` 不设置 `rev` 或 tag；`Cargo.lock` 自动记录本次解析的提交，使 `--locked` 和离线构建可复现。普通构建不会自动升级到远端每个新提交。

如果 Cargo 获取私有 Git 依赖时需要复用已经配置好的 Git CLI 凭据，可执行：

```sh
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo fetch --locked
```

依赖下载完成后可以使用后文的 `--offline` 命令。新电脑直接运行离线构建而没有先获取依赖，会报缓存缺失。

依赖包包含以下资源。表中路径相对于依赖包；适配库的构建脚本自动定位头文件，部署资源按设备说明从已解析的依赖包或已核对的设备共用目录取得：

| 目录 | 内容 |
| --- | --- |
| `third-party/nx5/include/` | TDE/MMZ、WBC 所需头文件及传递依赖，VO FFI 的 ABI 参考 |
| `third-party/nx5/lib/` | `libVideoDec-1.6.6.so` 和相对别名 `libVideoDec.so` |
| `third-party/fonts/` | `DejaVuSans.ttf` 和随字体部署的版权说明 |

资源来源、摘要和覆盖变量见 [资源说明](../third-party/README.md)。交叉工具链与 sysroot 仍需自行安装，业务 UI 和素材由自己的应用提供。

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

本例默认 `nx5-tde`，因此不传 `--features` 也会选择 NX5 TDE。转发同时启用应用自身的 `nx5`；该默认值属于业务应用，适配库自身的默认 feature 仍为空。若希望每次构建显式选板，可把应用的 `default` 改回 `[]`，并始终传 `--features nx5-tde` 或 `--features fbdev`。适配库本身仍不默认选择设备。仅使用 NX5 的应用也可以不声明这些转发项，直接在 `slint-bridge` 依赖中写 `features = ["nx5-tde"]`。

Cargo 的 `git` 字段使用完整 SSH URL `ssh://git@github.com/omni-stack-gen/slint_bridge.git`。已有业务工程需修改适配层依赖地址并更新自己的锁文件，Slint 依赖地址保持不变。

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

**Git 依赖中的 `.cargo/` 不会自动成为业务工程的交叉编译配置。** 在业务工程根目录创建 `.cargo/`，按目标设备保存下面的独立配置文件。四板配置均包含对应的完整链接器路径。

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

输出程序名为 `panel-app`。输入路径与旋转由上述 Rust `Config` 提供；需要命令行参数时，由业务应用自行添加解析。

## 配置设备运行环境

完成上述业务工程构建后，程序位于 `target/<Rust target>/release/panel-app`。使用对应工具链的 strip 工具生成部署副本，通过设备支持的传输通道上传至独立应用目录，并赋予执行权限。NX5/NX1 共用构建输出路径，切换设备构建前另存所需程序副本。

部署前核对设备型号、固件、实际挂载和显示所有者，保留原应用的恢复方式；已有显示程序退出并释放资源后再启动业务程序。运行账户须能访问 `/dev/fb0` 和配置的输入设备。下方命令使用本文的 `panel-app`，触摸与旋转已写在 Rust `Config` 中。

### 准备运行资源

四板均需可用字体；可使用依赖包附带的 `third-party/fonts/DejaVuSans.ttf`，并随文件提供 `DejaVu-COPYRIGHT`。NX5 还需要与固件匹配的 `libVideoDec-1.6.6.so`，可使用依赖包的 `third-party/nx5/lib/` 版本或已核对的设备共用库；其他三板不加载 NX5 SDK。

需要定位依赖包时，在业务工程根目录执行以下只读命令，输出 `slint-bridge` 的包目录。上述资源路径均相对于这个目录；不要把它们当作业务工程中的相对路径：

```sh
cargo metadata --locked --offline --format-version 1 | python3 -c 'import json, pathlib, sys; print(next(pathlib.Path(p["manifest_path"]).parent for p in json.load(sys.stdin)["packages"] if p["name"] == "slint-bridge"))'
```

把字体及版权部署到应用目录；NX5 的 SDK 部署到其 `lib/` 子目录，并保留 `libVideoDec.so` 指向 `libVideoDec-1.6.6.so` 的相对链接。复用设备共用资源时，相应调整下面的环境变量。以下目录均是示例，替换为实际部署目录。

### NX5

NX5 使用 SDK 初始化显示，`LD_LIBRARY_PATH` 指向实际 SDK 目录。程序运行期间保持独占显示会话。若使用 `/tmp`，注意它占用 RAM 且重启后文件可能丢失。

```sh
NX5_RUN_DIR=/tmp/panel-app-001
# 仅在新目录已部署版本库且别名尚不存在时创建。
ln -s libVideoDec-1.6.6.so "$NX5_RUN_DIR/lib/libVideoDec.so"
SLINT_DEFAULT_FONT="$NX5_RUN_DIR/DejaVuSans.ttf" \
LD_LIBRARY_PATH="$NX5_RUN_DIR/lib" "$NX5_RUN_DIR/panel-app"
```

### X9

X9 通过 DRM 完成事件交接显示页，应用需要相应设备权限。本文配置使用 event1、顺时针 90°，逻辑界面为 1280×800。

```sh
X9_RUN_DIR=/tmp/panel-app-001
SLINT_DEFAULT_FONT="$X9_RUN_DIR/DejaVuSans.ttf" "$X9_RUN_DIR/panel-app"
```

### NX1

程序和资源放在已核对挂载的 `/mnt` 新目录。原应用可能带看门狗，退出原显示程序时应按产品的设备管理流程处理，避免被自动重新拉起。本文配置使用 event2、不旋转。

```sh
NX1_RUN_DIR=/mnt/panel-app-001
SLINT_DEFAULT_FONT="$NX1_RUN_DIR/DejaVuSans.ttf" "$NX1_RUN_DIR/panel-app"
```

### F1

程序和资源放在真实 SD 卡挂载 `/tmp/sdcard` 的新目录；未挂载时先完成挂载，不使用同名 RAM 目录替代。退出原显示应用时保留配套 control_service。本文配置使用 event0、不旋转。

```sh
F1_RUN_DIR=/tmp/sdcard/panel-app-001
SLINT_DEFAULT_FONT="$F1_RUN_DIR/DejaVuSans.ttf" "$F1_RUN_DIR/panel-app"
```

上述命令以前台方式运行；常驻启动由产品自己的进程管理方式负责。最小业务示例不提供自动退出参数，应用需要结束时可调用 `slint::quit_event_loop()`，事件循环返回前会等待在途显示提交。

## 更新代码与 Slint

以下命令在业务工程根目录运行，用于更新其 Git 依赖：

```sh
# 更新适配层依赖。
cargo update -p slint-bridge
# 需要升级 Slint 时单独执行。
cargo update -p slint
```

检查业务工程的 `Cargo.lock` 差异，确认所有 Slint crate 仍来自同一 Git 来源和提交；完成目标设备的构建及受影响功能验证后，提交业务工程的锁文件。只获取锁定版本时使用 `cargo fetch --locked`。

业务接入使用已发布的配套依赖，由 Cargo 管理缓存。升级 Slint 版本号时同步调整运行库、软件 renderer 和 `slint-build` 的版本约束。

## 常见配置问题

| 现象 | 检查项 |
| --- | --- |
| Git/Cargo 不能访问仓库 | GitHub SSH 密钥及仓库权限、Slint 内网连通性与 Git 凭据；需要时使用 Git CLI 获取依赖 |
| 离线构建提示找不到依赖或提交 | 先联网运行 `cargo fetch --locked`；首次新业务工程没有锁文件时使用 `cargo fetch` |
| 找不到交叉 gcc，或 ELF/浮点 ABI 不匹配 | 检查对应 target 的 linker、CC/CXX/AR 和完整 sysroot，确认没有混用 NX5/X9 工具链 |
| 报必须选择板级 feature 或 feature 互斥 | `default = []` 时必须传 `--features`；已有默认设备时，切换设备或模式使用 `--no-default-features --features ...`。不要用 `--all-features`；这类错误不表示缺少本地源码目录 |
| 找不到 `mpi_tde.h` | 核对 Cargo 解析的适配库包是否包含 SDK 头文件；清除指向旧机器目录的 `NX5_SDK_INCLUDE` 覆盖 |
| NX5 找不到 `libVideoDec.so` | 检查设备上的版本库、无版本别名及 `LD_LIBRARY_PATH`，不要填主机仓库路径 |
| NX5 报 VO 已启用，或设备资源被占用 | 重新核对显示所有者并按恢复流程释放，不并行启动第二个 SDK 显示程序 |
| 字体缺失或画面对照不同 | 部署字体及版权说明，并明确设置设备上的 `SLINT_DEFAULT_FONT` |

进一步的设备参数、部署资源保护及故障处理，分别见 [NX5](../test-app/devices/nx5/README.md)、[NX1](../test-app/devices/nx1/README.md)、[X9](../test-app/devices/x9/README.md)、[F1](../test-app/devices/f1/README.md)。
