# NX5 测试目标

本目录保存当前 NX5 目标的 [current-target.json](current-target.json)。其中引用的仓库路径仍相对于仓库根目录；设备 PID、端口及启动 ID 是上次观测记录，运行前需重新核对。

当前板为 DPower Terminal 的 `/dev/ttyUSB1`，网络为 `192.168.1.151:5555`（ADB），内核 #233。库会核对固件摘要并要求独占显示。详细环境及 SDK 使用约束见 [当前环境](../../../docs/current-environment.md)。

以下命令从仓库根执行：

```sh
# 纯软件测试程序。
cargo build --locked --offline --release -p slint-bridge-test-app \
  --features nx5 --target arm-unknown-linux-gnueabi --bin display

# Cortex-A7/NEON、TDE 和 Slint 原生 MCP。
SLINT_EMIT_DEBUG_INFO=1 \
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
cargo --config .cargo/nx5.toml build --locked --offline --release \
  -p slint-bridge-test-app --features nx5-tde,slint/mcp \
  --target arm-unknown-linux-gnueabi --bin display
```

程序输出为 `target/arm-unknown-linux-gnueabi/release/display`，不再位于 `release/examples/`。需要时使用当前 ARM 工具链的 `strip -o` 产生部署副本。确认旧显示实例已经退出后，再通过 USB1 root 会话运行：

```sh
SLINT_MCP_PORT=9315 \
SLINT_DEFAULT_FONT=/run/slint-bridge-display/DejaVuSans.ttf \
LD_LIBRARY_PATH=/system/app/clib \
/run/slint-bridge-test/display --textures --touch=/dev/input/event0 --seconds=20
```

主机连接 Slint MCP：

```sh
adb -s 192.168.1.151:5555 forward tcp:19315 tcp:9315
```

endpoint 为 `http://127.0.0.1:19315/mcp`。它只通过端口转发访问设备的 localhost；UI 用例使用原生 MCP 工具，见 [测试工程说明](../../README.md)。

NX5 工具集中在 [tools/nx5/](../../tools/nx5/README.md)。WBC 必须在显示进程内加载，不能同时启动另一个加载 SDK 的采集进程。正式渲染不依赖 WBC 插件。

硬件回归位于库内部，可构建为专用测试二进制，但必须在显示所有者退出后单独运行；入口及覆盖范围见 [TDE 报告](../../../docs/nx5-tde.md)。双 framebuffer 和无撕裂条件不因为测试工程迁移而放宽。当前实际显示吞吐约 29 FPS；单周期优化已停止在 [已记录的调查结论](../../../docs/nx5-single-cycle.md)。
