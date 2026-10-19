# F1 设备与运行

目标为 `fd_f1_r88r30_spinor-20c31b4578`（ArtInChip D211，RISC-V64）。
`f1` / `f1-g2d` 已支持本次检查的固件，按用户指定的参考项目采用
`PAN → WAITFORVSYNC`。固件、布局、独占检查及失败行为集中维护在
[平台说明](../../../docs/software-platform.md#准入边界)。
这是参考使用约定，不代表已证明旧页硬件退休或所有帧无撕裂；
[镜像分析](../../../docs/f1-kernel-analysis.md) 的限制仍成立。

[设备调查](../../../docs/f1-display.md) 保存初始查询与离屏 GE 结果；
[showcase 实测](../../../docs/f1-showcase.md) 保存初始上屏基线；
[图片与透明合成优化](../../../docs/f1-optimization.md) 保存后续实机对照。
G2D 图片源槽最多增加 1 MiB DMA 内存，不复制全屏背景；能力边界见平台说明。

## 构建

需要 `riscv64gc-unknown-linux-gnu` Rust target 和 `/opt/F1` T-HEAD GCC 10.2.0。
从仓库根执行，CPU 对照将 `f1-g2d` 改为 `f1`，并另存输出：

```sh
SLINT_EMIT_DEBUG_INFO=1 cargo --config .cargo/f1.toml build --locked --offline \
  --release -p slint-bridge-test-app --features f1-g2d,slint/mcp \
  --target riscv64gc-unknown-linux-gnu --bin showcase
mkdir -p target/f1-showcase
/opt/F1/bin/riscv64-unknown-linux-gnu-strip \
  -o target/f1-showcase/showcase-g2d target/riscv64gc-unknown-linux-gnu/release/showcase
```

ELF 解释器为 `/lib/ld-linux-riscv64xthead-lp64d.so.1`；工具链与 sysroot 必须配套。
独立业务工程须配置相同 target/linker，feature 本身不选择 ABI。

## SD 卡部署与所有权

设备可用物理内存约 57 MiB，无 swap。固件、字体、日志及截图均放在
SD 卡 `/tmp/sdcard` 的独立版本目录，不放在 RAM `/tmp` 或 `/run`。
部署前重新核对序列号、boot ID、`/proc/mounts`、`df -h /tmp/sdcard`、
`/proc/meminfo`；确认 `/tmp/sdcard` 是 `/dev/mmcblk0` 的真实挂载点。
未挂载时停止，不创建同名 RAM 目录。上传后回读核对 SHA256。

同时检查当前 `/dev/fb0` 描述符/映射、进程 exe、cmdline 和 starttime；
旧报告 PID 不能用于 kill。原显示程序为 `/system/app0`，配套程序为
`/system/control_service`。本轮仅停止 app0，保留 control_service，
未替换 `/system/app0` 或改动启动脚本。后端发现其他显示所有者会拒绝。

先记录原应用的 cwd、环境和恢复命令，再释放经过实时核对的显示所有者。
不要执行系统服务的 `stop/restart`：本固件脚本会同时强杀配套服务。
有限验证优先使用 `--seconds=180`，由事件循环正常退出并等待提交线程。

已确认无人占用显示后，在设备 shell 执行（`F1_RUN_DIR` 为本轮已上传目录）：

```sh
SLINT_DEFAULT_FONT="$F1_RUN_DIR/DejaVuSans.ttf" SLINT_MCP_PORT=9315 \
  SLINT_BRIDGE_TRACE=1 "$F1_RUN_DIR/showcase-g2d" \
  --touch=/dev/input/event0 --seconds=180
```

不旋转，逻辑/物理尺寸均为 1024×600。字体与版权随程序部署，来源见
[第三方资源](../../../third-party/README.md)。旧版 adbd 不支持 `exec-out`，
普通后台 `&` 可能被断连终止；使用 [工具说明](../../tools/f1/README.md) 的
`run-detached`，它会创建独立会话并将标准流转到 SD 卡。日志名须未被使用。
持续展示可省略 `--seconds`；本轮交付保留 G2D showcase 在 Overview 页面运行。

## 恢复与验证边界

有限测试正常退出后，确认无测试进程及 framebuffer/GE 占用，再按采集的
原 cwd/环境通过 `run-detached` 启动 `/system/app0`。本次原环境的
`LD_LIBRARY_PATH` 为 `/system/clib:/lib:/usr/lib:/system/usr/lib`。
保留现有 control_service，不启动第二份。确认 app0 重新取得 fb/GE/input。

常驻 showcase 没有专门的远程正常退出命令；恢复原固件启动状态可安排设备
正常重启（本轮未重启）。未知 GE/DMA 状态时不能用强杀代替资源完成确认，
也不能立即启动第二个显示应用。原应用和启动配置始终保留。

主机回归、ABI 检查、离屏硬件验证、MCP 与物理像素读回分别计证。
MCP 点击/滚动不验证真实触摸方向；静止页读回不证明连续扫描无撕裂。
真实 GE 故障恢复和逐帧退休仍未验证。截图前检查 MemAvailable，至少保留
16 MiB，低于阈值跳过；采样窗口内不截图。
