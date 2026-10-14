# NX5 测试目标

本目录保存 [current-target.json](current-target.json)；路径相对于仓库根。以下 PID、摘要和资源为 2026-10-14 观测，操作前须重新核对。

2026-10-14 PNG / SVG 单独缩放测量收尾：正式程序仍为 `abb2e33` + Slint `22804ff`，路径 `/tmp/slint-bridge-scaling-abb2e33/showcase`，恢复后观测 PID **7333**，日志 `live-after-format-profile.log`。Image scaling 恢复为左右双图 100%，Controls 保留 48%；无退出定时器、trace、录制、回放或 WBC。

单 PNG 为 **28.99 FPS / 65.23% CPU / 13.49 MiB 平均 RSS**，单 SVG 为 **28.06 FPS / 84.38% CPU / 14.83 MiB 平均 RSS**；同一诊断构建的双图为 25.99 FPS，空图对照为 28.96 FPS。CPU 为关闭 trace 的单核占比，FPS 使用独立进程。临时诊断仅改变图片选择与测试文案，正式源码已恢复；方法与归因限制见[单格式缩放报告](../../../docs/nx5-scaling-formats.md)。

八个有限进程正常退出并释放 VO/MMZ；71 个诊断文件校验归档清理，原常驻和三个恢复二进制均保留。针对测试中 SVG 未显示的反馈，核对当时为 `none` 空图对照；恢复后原生截图与两张物理页的 16 个四色点一致，未做整帧、WBC 或 LCD 扫描验证。恢复后 RSS 15284 KiB、VmHWM 20004 KiB（含恢复截图峰值）、MMZ 12100 KiB / 5 块，可用内存 23532 KiB。下次操作前重新核对身份、实际 PID 和显示所有者。

当前保留项见 [deploy-retain.json](deploy-retain.json)。

此前关闭 trace 的新旧正式版资源对照见[资源报告](../../../docs/nx5-release-overhead.md)，输入延迟及真实页像素验证见[输入延迟报告](../../../docs/nx5-input-latency.md)；清理版实体触摸确认和 NEON 性能保留在[部署报告](../../../docs/nx5-cleanup-deploy.md)、[NEON 报告](../../../docs/nx5-neon-unpremultiply.md)。

纯色版本的 [性能报告](../../../docs/nx5-showcase-performance.md) 和 [首次部署报告](../../../docs/nx5-showcase.md) 保留原场景与提交的数据，不改记为图片背景成绩。

原 `f963558` 的 alpha 量程、实际页/WBC 和 29 FPS 成绩仍属于 [2026-10-12 透明 UI 实测](../../../docs/nx5-alpha.md)，不改记为六页新界面的测量。

此前同进程初始化重试、VO/描述符清理、WBC 正负对照及纹理退出见 [审查修复实机报告](../../../docs/nx5-review-fixes.md)；当时截图 OOM 及清理后 [十次截图复验](../../../docs/nx5-screenshot-memory.md) 保留原始记录。双页像素和三项独立 ignored 测试等也保留各自此前报告口径，不作为本次触摸部署的新结果。

当前板为 DPower Terminal `/dev/ttyUSB1`、ADB `192.168.1.151:5555`、内核 #233。运行行为见 [平台说明](../../../docs/software-platform.md#准入边界)，部署快照及恢复前提见 [当前环境](../../../docs/current-environment.md)。

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

使用 `--observe=N` 前，先在主机构建插件。`NX5_SDK_LIBRARY` 可指向任意位置的兼容 SDK 文件；必须与显示进程加载的 SDK 为同一份库，并确保该库的 SONAME 与 `libVideoDec.so` 解析到同一文件，以复用会话：

```sh
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
NX5_SDK_LIBRARY=/home/dpower/data/GitLab/LGUI3/c_library/third_party/lib/NX5/codec_rorate/libVideoDec-1.6.6.so \
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

硬件回归位于库内部，可构建为专用测试二进制，但必须在显示所有者退出后单独运行；入口及覆盖范围见 [TDE 报告](../../../docs/nx5-tde.md)。双 framebuffer 和无撕裂条件不因为测试工程迁移而放宽。当前实际显示吞吐约 29 FPS；单周期优化已停止在 [已记录的调查结论](../../../docs/nx5-single-cycle.md)。
