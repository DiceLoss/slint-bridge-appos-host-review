# Slint Bridge 开发与维护

本页面向适配库维护者，包含仓库验证、测试程序构建和诊断连接。业务应用的依赖、四板配置及运行方式见 [使用指南](getting-started.md)。以下操作在现有适配库工作区执行。

先获取锁定依赖，再核对随包资源：

```sh
rustup component add rustfmt clippy
cargo fetch --locked
(cd third-party && sha256sum -c SHA256SUMS)
```

## 配置交叉编译器

以下配置供本仓库及测试工程构建使用：

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

以下命令从仓库根目录执行。选择一个板级 feature：

| feature | 用途 |
| --- | --- |
| `appos-host` | AppOS host 接收 launcher 已准入并转移的 FD；只用于集成和主机合同测试 |
| `fbdev` | X9 当前已验证的纯 CPU framebuffer 路径 |
| `nx1` | NX1 纯 CPU framebuffer 与明确选择的参考 PAN 约定 |
| `nx5` | NX5 显示会话及软件渲染 |
| `nx5-tde` | NX5 TDE 加速和软件回退，包含 `nx5` |
| `nx5-mma-background` | 包含 `nx5-tde`；通过 Config 显式选择 MMA 背景 |
| `f1` / `f1-g2d` | F1 参考 PAN/VSYNC；后者包含 GE 填充与受限图片混合 |

`appos-host`、`fbdev`、`nx1`、`nx5`、`f1` 两两互斥，不使用 `--all-features`。
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

## 原生 MCP 连接

只有启用了 MCP 的构建才使用下面的连接。主机设置实际 ADB 地址后转发端口，两台设备使用不同的主机端口：

```sh
# 以下地址是现有测试环境示例，先核对再替换。
NX5_SERIAL=192.168.1.151:5555
X9_SERIAL=192.168.1.143:5555
adb -s "$NX5_SERIAL" forward tcp:19315 tcp:9315
adb -s "$X9_SERIAL" forward tcp:19316 tcp:9315
```

NX5 endpoint 为 `http://127.0.0.1:19315/mcp`，X9 为 `http://127.0.0.1:19316/mcp`。查询、交互和截图步骤见 [Slint MCP 指南](slint-mcp.md)；截图前按设备说明检查可用内存。

## 设备测试与部署工具

测试程序的页面、参数及回归范围见 [测试工程](../test-app/README.md)。板级准入、显示所有权、恢复与诊断流程按 [NX5](../test-app/devices/nx5/README.md)、[NX1](../test-app/devices/nx1/README.md)、[X9](../test-app/devices/x9/README.md)、[F1](../test-app/devices/f1/README.md) 说明执行。

NX5 测试部署使用 [归档和上传工具](../test-app/tools/nx5/README.md#部署归档清理和截图)，维护当前/恢复文件与保留策略；同一事务只执行一次 `upload` 或 `finish`。TDE 和 WBC 必须复用显示进程 SDK 会话。有限时长测试使用测试应用的 `--seconds=N` 正常退出；强杀不能代替资源释放验证。

## 更新 Slint 与锁文件

Slint 源码修改须先在公开配套远端发布，再把所有 Slint 依赖更新到同一精确 revision：

```sh
cargo update -p slint
```

检查 `Cargo.lock` 中所有 Slint crate 仍来自同一 Git 来源和提交；完成主机 feature 矩阵、受影响设备交叉构建及实机回归，再提交锁文件。不直接修改 Cargo 缓存。升级版本号时同步调整运行库、软件 renderer 和 `slint-build` 的约束；补丁迁移与验证边界见 [平台说明](software-platform.md#构建依赖)。
