# F1 查询、显示与离屏验证

`inspect-device.c` 用于 [F1 显示调查](../../../docs/f1-display.md) 中的参数采集，
不是后端准入或翻页测试。当前验证对象为
`fd_f1_r88r30_spinor-20c31b4578`、Linux 5.10.44 #1（2026-05-22）。

探针以 `O_RDONLY` 打开 `/dev/fb0`、`/dev/input/event0` 和 `/dev/ge`，仅调用
`FBIOGET_FSCREENINFO`、`FBIOGET_VSCREENINFO`、`EVIOCGNAME`、
`EVIOCGBIT`、`EVIOCGABS`、`IOC_GE_VERSION`、`IOC_GE_MODE`。不映射或写入 framebuffer，不提交翻页、等待
VSYNC、加载厂商 SDK、抓取输入或读取输入事件队列。

运行前重新核对 ADB 序列号、boot ID、显示所有者及其进程 starttime。
原应用可以保持运行；采样不是原子快照，读回页号不能证明扫描完成。
其他固件使用前须审查 framebuffer 的 open/release 路径：即使只读打开，
close 仍会经过厂商 release。参考源码按调用进程释放 DMA-BUF，
本探针不导入 DMA-BUF；当前固件与该源码尚未建立完整对应。
GE 的 open/release 也经过驱动路径；参考驱动通过引用计数保留原应用的 GE
资源。运行前确认原应用仍持有 `/dev/ge`，不要把只读 ioctl 当成对任意固件
都无副作用的保证。探针不提交 GE 命令，不调用 GE_SYNC。

在仓库根编译（交叉工具链须另行安装）：

```sh
mkdir -p target/f1-inspect
/opt/F1/bin/riscv64-unknown-linux-gnu-gcc \
  -std=c11 -D_GNU_SOURCE -O2 -Wall -Wextra -Werror \
  test-app/tools/f1/inspect-device.c -o target/f1-inspect/inspect-device
```

F1 内存较小，固件和测试产物放在 SD 卡 `/tmp/sdcard`；部署前确认它仍是
`/dev/mmcblk0` 的挂载点并检查空间。不要在 SD 卡未挂载时创建同名路径，
否则会落到 RAM `/tmp`。使用指定序列号创建独立测试目录，将返回路径记录为 `F1_INSPECT_DIR`：

```sh
F1_SERIAL=fd_f1_r88r30_spinor-20c31b4578
adb -s "$F1_SERIAL" shell 'cat /proc/mounts; df -h /tmp/sdcard; cat /proc/meminfo'
# 确认 /tmp/sdcard 为真实 SD 挂载点后执行。
F1_INSPECT_DIR=$(adb -s "$F1_SERIAL" shell 'mktemp -d /tmp/sdcard/slint-f1-inspect-XXXXXX' | tr -d '\r')
test -n "$F1_INSPECT_DIR" &&
adb -s "$F1_SERIAL" push target/f1-inspect/inspect-device "$F1_INSPECT_DIR/inspect-device"
adb -s "$F1_SERIAL" shell "$F1_INSPECT_DIR/inspect-device"
```

本次旧版 adbd 不支持 `exec-out`。归档输出并复核原应用的身份、资源占用
和显示配置后，仅删除本轮临时文件：

```sh
adb -s "$F1_SERIAL" shell "rm '$F1_INSPECT_DIR/inspect-device' && rmdir '$F1_INSPECT_DIR'"
```

工具退出成功只表示参数查询成功，不验证触摸物理方向、透明合成、刷新率、
页锁存、DMA 完成或无撕裂。

`check-ge-abi.c` 是纯编译检查，对照来自参考项目的 UAPI 头文件核验 Rust
normal-mode 结构及请求编码；不运行设备代码：

```sh
/opt/F1/bin/riscv64-unknown-linux-gnu-gcc -std=c11 -Wall -Wextra -Werror \
  -Ithird-party/f1/include -fsyntax-only test-app/tools/f1/check-ge-abi.c
```

## 离屏 G2D 与缓存验证

库内 `backends::f1::g2d::native::hardware::offscreen_fill_cache` 是仅在
RISC-V 编译的 ignored 实机测试。它直接使用原生 `Native`，不允许堆画布
回退；不经过 `init()`，也不打开 framebuffer，不提供显示准入的绕过入口。
普通单元测试仍使用软件画布，不能用普通 `cargo test` 当作硬件通过。

先核对身份、显示/GE 所有者、SD 挂载及可用内存，保留原应用运行。从根目录
编译，Cargo 输出会给出实际测试二进制路径：

```sh
cargo --config .cargo/f1.toml test --locked --offline --release --features f1-g2d \
  --target riscv64gc-unknown-linux-gnu --lib --no-run
```

用 `/opt/F1/bin/riscv64-unknown-linux-gnu-strip -o 输出路径 Cargo输出的测试二进制`
生成部署副本，上传到已核对的 `/tmp/sdcard` 独立目录，并回读核对 SHA256。
在保持连接的 ADB 前台会话内仅运行这个用例（`F1_TEST_DIR` 替换为本轮目录）：

```sh
adb -s "$F1_SERIAL" shell \
  'F1_TEST_DIR/g2d-tests --ignored --exact backends::f1::g2d::native::hardware::offscreen_fill_cache --nocapture --test-threads=1'
```

当前旧版 adbd 的后台 shell 启动可能在断开时终止子进程；不要依赖 `&`。
保留会话并将日志放在 SD 卡，可用另一个只读 ADB 会话观察进展。
成功条件为 `F1_GE_DONE`、测试结果 `1 passed` 和退出码 0。
若出现 `F1_GE_QUARANTINED`，测试会停止提交并保持 GE/DMA 资源；不要用
Ctrl-C、超时 kill 或断开会话强制清理，先记录 PID/日志并按板级恢复条件处理。
本测试不主动注入真实硬件超时或复位。

用例顺序验证 64×32、256×128、1024×600，每个尺寸 16 轮，最多一份 DMA
画布常驻。CPU 写入不同预乘像素，再执行 GE 整幅、内部奇数边界、四角单像素
及交叉细条填充；flush/invalidate 后逐像素比较覆盖区和未覆盖区。
测试耗时包括 CPU 图案生成、读回、比较和校验和，不能作为 GE 吞吐基准。
它验证该调用序列的正向数据一致性，不证明缓存同步可省略、所有故障均可恢复
或显示翻页完成。结束后核对原应用身份、显示配置、内存池和驱动错误日志。

## 固定内核镜像重放

`analyze-kernel.py` 是 [当前固件分析](../../../docs/f1-kernel-analysis.md) 的
离线复核工具，不访问设备。默认输入为本地归档的 FIT 与已解压 Image，
须另行取得 `evidence/f1-display-20261019/` 原始档案：

```sh
python3 test-app/tools/f1/analyze-kernel.py
```

它先核验两个固定 SHA256，再提取操作表、ioctl 分派表并用 `/opt/F1` objdump
生成六段反汇编。其他镜像会被拒绝，不能把这些固定地址套用于新固件；
成功重放也不是新的实机翻页或设备准入验证。

## Showcase 性能采样

`profile-local.sh` 控制已启动的 showcase，不启停显示所有者。所有请求走
设备 localhost 的 Slint 原生 MCP，ADB/串口仅用于启动和取回证据。
构建并上传到同一 SD 目录：

```sh
for tool in run-detached observe-resources mcp-scroll read-front; do
  /opt/F1/bin/riscv64-unknown-linux-gnu-gcc -O2 -Wall -Wextra -Werror \
    "test-app/tools/f1/$tool.c" -o "target/f1-showcase/$tool"
done
```

- `run-detached LOG PROGRAM [ARGS...]`：确认 exec 成功后返回 PID，独立会话、
  日志独占创建；仍须检查应用启动日志与存活状态，不自动核验 SD 挂载。
- `observe-resources PID MAX_SECONDS PHASE_FILE`：核对 starttime，每秒读取
  CPU ticks、RSS/HWM、MemAvailable、缺页和 DE/GE 中断计数；不访问显示。
- `mcp-scroll NEGATIVE_JSON POSITIVE_JSON SECONDS`：重复本次 MCP 查询得到的
  句柄请求，三秒换方向，每次成功应答后等 20 ms；验证 HTTP/RPC 错误。
- `read-front NEW_OUTPUT.bgra`：只读 `/dev/fb0`，每次 4 KiB 写到 SD。
  用于有限运行退出后的静止页，或已确认无动画/输入的静态页面；读前后页号
  一致不能排除中间翻页，不能用它对动态扫描做原子截图或完成认证。

先启动一个具有原生 MCP 和 trace 的 180 秒 showcase，再执行：

```sh
"$F1_RUN_DIR/run-detached" "$F1_RUN_DIR/cpu-run-controller.log" \
  /bin/sh "$F1_RUN_DIR/profile-local.sh" showcase-cpu cpu-run
# G2D 对照：程序名改 showcase-g2d，输出 tag 改为新的 g2d-run。
```

脚本从本次进程查询窗口/元素句柄，依次测 Controls 静态、Overview、Motion、
Rich list 滚动，各预热 3 秒、采样 20 秒。截图只在性能采样前进行，
MemAvailable 不足 16 MiB 时跳过；富列表前不足 8 MiB 时结束并记录非零状态。
最后检查其他页面导航。需要 `PROFILE_DONE`、`*-exit-code.txt` 为 0，
以及有限显示程序 `APP_EXIT=0`（由启动 shell 包装记录）。每次使用新 tag，
不得覆盖历史证据。采样器 metadata/CSV、应用 trace 和 scroll 日志一起归档。

帧率用完整落在同一 phase 内的 trace 首末累计提交差除以单调时间差；
CPU 用该 phase 首末 ticks 差 / CLK_TCK / 秒数。没有周期 trace 的静态页面
标记按需绘制，不把面板 IRQ 率当作应用 FPS。实测及已知干扰见
[F1 showcase 报告](../../../docs/f1-showcase.md)。

## GE 图片混合回归

`backends::f1::g2d::native::hardware::offscreen_texture_cache` 是另一个
RISC-V ignored 用例，部署和异常隔离要求与离屏填充相同。使用同一条
`cargo ... test --no-run` 命令构建，将 `--exact` 改为该名称。
它直接构造原生 GE 画布，经真实 Slint 图片回调比较软件像素，覆盖奇数位置、
嵌套裁剪、straight/预乘输入、整体透明度、共享源复用和写时复制源变化。
不透明目标硬件 RGB 容差为 1，透明目标要求软件回退且逐字节一致；
验证每次硬件完成及缓存同步后的完整画布。成功须同时出现
`F1_GE_TEXTURE_DONE`、`1 passed`、退出码 0，不能只看进程正常退出。
该用例不访问 framebuffer，也不主动注入真实 DMA 超时。

性能与内存比较继续使用前述本地采样工具；像素误差的实测口径、
一次性微基准源码及原始数据获取条件见 [优化记录](../../../docs/f1-optimization.md)。
