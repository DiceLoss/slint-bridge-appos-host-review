# NX5 测试目标

[MMA 背景](mma-background/README.md) 使用统一 Config 和普通 Cargo 构建入口；按需启用 `nx5-mma-background`，默认仍保留普通背景。

本文件维护构建、部署和运行前提。设备观测保存在带时间戳的 [current-target.json](current-target.json)，恢复文件保护策略见 [deploy-retain.json](deploy-retain.json)；操作前重新核对身份、PID、显示所有者及依赖，不能把快照当作实时状态。

现有验证目标为 USB1 root、ADB `192.168.1.151:5555`、Linux #233。当前接口与约束见 [平台说明](../../../docs/software-platform.md)，显示完成/合成依据见 [显示调查](../../../docs/nx5-display.md)，阶段结果分别见 [性能记录](../../../docs/nx5-performance.md)、[功能验证](../../../docs/nx5-validation.md) 和 [Slint 两设备对照](../../../docs/slint-optimization.md)。

构建默认使用 [仓库内 SDK 头文件与动态库](../../../third-party/README.md)，无需设置 `NX5_SDK_INCLUDE`。新机器仍需安装本文件指定的交叉工具链。字体及版权说明使用 `third-party/fonts/` 中的文件；部署 SDK 时保留版本库与 `libVideoDec.so` 别名，设备上的 `LD_LIBRARY_PATH` 指向实际部署目录。

首次先运行 `python3 tools/prepare-slint.py` 和 `cargo fetch --locked`，准备固定的官方 Slint 补丁源码及依赖。以下命令从仓库根执行：

```sh
# 纯软件测试程序。
cargo build --locked --offline --release -p slint-bridge-test-app \
  --features nx5 --target arm-unknown-linux-gnueabi --bin display

# Cortex-A7/NEON、TDE 和 Slint 原生 MCP。
SLINT_EMIT_DEBUG_INFO=1 \
cargo --config .cargo/nx5.toml build --locked --offline --release \
  -p slint-bridge-test-app --features nx5-tde,slint/mcp \
  --target arm-unknown-linux-gnueabi --bin display
```

程序输出为 `target/arm-unknown-linux-gnueabi/release/display`。使用当前配置的 ARM 工具链生成部署副本：

```sh
mkdir -p target/nx5-deploy
/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-strip \
  -o target/nx5-deploy/display target/arm-unknown-linux-gnueabi/release/display
```

通过 [部署脚本](../../tools/nx5/README.md#部署归档清理和截图) 先归档清理，再上传到新的版本目录；以下运行命令以 `/tmp/slint-bridge-next/display` 为例。脚本会校验上传摘要并赋予执行权限。字体需另行部署到下方路径。以下示例使用设备原有 SDK 目录；SDK 可以放在其他目录，只需在启动前将 `LD_LIBRARY_PATH` 改为对应位置，并提供无版本库名 `libVideoDec.so`（可链接到实际 SDK 文件）。`/run` 中的程序和字体在重启后需重新部署。确认旧显示实例已经退出后，再通过 USB1 root 会话运行：

```sh
SLINT_MCP_PORT=9315 \
SLINT_DEFAULT_FONT=/run/slint-bridge-display/DejaVuSans.ttf \
LD_LIBRARY_PATH=/system/app/clib \
/tmp/slint-bridge-next/display --textures --touch=/dev/input/event0 --seconds=20
```

部署诊断程序前及验证结束后，统一使用 [deploy.py 的归档清理流程](../../tools/nx5/README.md#部署归档清理和截图)。维护 [deploy-retain.json](deploy-retain.json) 中的当前程序、恢复程序和共用依赖；脚本执行时还会核对实时活动引用。`/run` 与 `/tmp` 共用物理内存预算，避免保留多轮二进制；保留当前依赖、活动日志、独占锁及一份必要恢复程序。

主机连接 Slint MCP：

```sh
adb -s 192.168.1.151:5555 forward tcp:19315 tcp:9315
```

endpoint 为 `http://127.0.0.1:19315/mcp`。它只通过端口转发访问设备的 localhost；UI 用例使用原生 MCP 工具，见 [测试工程说明](../../README.md)。

NX5 工具集中在 [tools/nx5/](../../tools/nx5/README.md)。WBC 必须在显示进程内加载，不能同时启动另一个加载 SDK 的采集进程。正式渲染不依赖 WBC 插件。

已启动的观察线程在正常退出和事件循环出错时都会被等待，先完成插件清理再结束应用。应用错误与观察线程 panic 的处理及主机回归入口见 [测试工程说明](../../README.md)。

使用 `--observe=N` 前，先在主机构建插件。默认链接仓库内 SDK；`NX5_SDK_LIBRARY` 仍可指向其他位置的兼容 SDK 文件；必须与显示进程加载的 SDK 为同一份库，并确保该库的 SONAME 与 `libVideoDec.so` 解析到同一文件，以复用会话：

```sh
sh test-app/tools/nx5/display/build-observer.sh
```

将输出 `target/nx5-display/libobserver.so` 部署到任意动态库目录；[nx5_observer.rs](../../src/nx5_observer.rs) 按 `libobserver.so` 加载。下例约定部署到 `/run/slint-bridge-display/`，并将它与 SDK 目录一起加入 `LD_LIBRARY_PATH`。插件接收应用配置的 framebuffer 路径并查询实际尺寸，需与新程序一起重建部署。通过设备 root 会话进行有限时长采样：

```sh
SLINT_DEFAULT_FONT=/run/slint-bridge-display/DejaVuSans.ttf \
SLINT_BRIDGE_TRACE=1 LD_LIBRARY_PATH=/run/slint-bridge-display:/system/app/clib \
/tmp/slint-bridge-next/display --pattern --observe=300 --seconds=20 \
  > /run/slint-bridge-test/pattern.log 2>&1
```

把日志取回主机后执行 `python3 test-app/tools/nx5/display/check-run.py <日志路径> --wbc`。检查器要求至少 300 个样本、黑白均有、无混色、采集速率为 28–32 FPS，以及提交统计窗口至少为默认的 28 FPS。`--split` 负对照应检出混色；不能只用应用退出码判断采样成功。

硬件回归位于库内部，可构建为专用测试二进制，但必须在显示所有者退出后单独运行；入口及覆盖范围见 [TDE 报告](../../../docs/nx5-performance.md#nx5-tde)。双 framebuffer 和无撕裂条件不因为测试工程迁移而放宽。当前实际显示吞吐约 29 FPS；单周期优化已停止在 [已记录的调查结论](../../../docs/nx5-display.md#nx5-single-cycle)。
