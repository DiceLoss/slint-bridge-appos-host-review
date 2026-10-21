# NX1 / sun8i 测试目标

本目录对应 2026-10-20 通过主机 `/dev/ttyUSB1` 检查的 NX1。
设备内核名称为 `X5`，不能仅凭 hostname 区分它与其他板。
本次为单核 Cortex-A7、Linux `3.4.39 #1 Fri Mar 25 14:53:50 CST 2022`、
glibc 2.21，内核可管理内存 43532 KiB。身份快照见
[current-target.json](current-target.json)，运行行为集中在
[平台说明](../../../docs/software-platform.md#准入边界)，调查与验证见
[NX1 报告](../../../docs/nx1-framebuffer.md)。

## 显示与输入

`/dev/fb0` 为 1024×600 / 虚拟 1024×1200、stride 4096、
BGRA8888 双页，物理基址 `0x43100000`，总长度 4915200 字节。
软件渲染不需要 SDK、DRM 或加速器。
运行账户还须能读写 `/dev/mem`：NX1 后端管理当前固件的 DE2 最终输出预乘位，
以保持透明图层的正确颜色和亮度；不改写物理页的 Alpha 或增加固定底层。
合成寄存器仅在初始化和析构恢复时校验，正常逐帧提交不读取；当前规则见
[平台说明](../../../docs/software-platform.md#准入边界)。`nx1` 是独立板级 feature，
不能使用仅接入 X9 的 `fbdev` 替代。

本次用户明确允许参考 PAN 约定：提交使用 `FBIOPAN_DISPLAY`，
保留双页、至多一笔在途提交和一帧预渲染。
该固件 PAN 内部丢弃等待错误；标准 WAITFORVSYNC 是空操作，
不能宣称旧页硬件退休或无撕裂已被证明。
**不要调用 `DISP_HWC_COMMIT`**：当前 12 层设备与驱动固定 16 层提交不匹配，
错误路径会遗留显示锁，必须重启恢复。正式后端不使用该接口。

GT911 位于 `/dev/input/event2`，使用 `ABS_X/ABS_Y + BTN_TOUCH` 单点协议，
范围 X=0..1024、Y=0..600；默认 `--rotation=0`。
`event0` 为 headset，`event1` 为 keypad，不要把它们当触摸设备。

## 构建

从仓库根执行：

```sh
SLINT_EMIT_DEBUG_INFO=1 cargo --config .cargo/nx1.toml build --locked --offline --release \
  -p slint-bridge-test-app --features nx1,slint/mcp \
  --target arm-unknown-linux-gnueabi --bin display --bin showcase
```

[.cargo/nx1.toml](../../../.cargo/nx1.toml) 使用
`/opt/NX1/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi`，
soft-float ABI、Cortex-A7/NEON。不要替换成 X9 的 hard-float 工具链。
Slint 运行库、软件 renderer 与 slint-build 保持锁文件的同一来源。
该分支不使用 F1 暂存分支的本地字体覆盖。

## 部署与恢复

每次先核对串口、`uname -a`、boot ID、MAC、当前显示所有者及挂载，
不能直接沿用快照 PID。设备重启可能启动 `/system/app0` 并改变网络配置；
需要先按设备维护流程退出原显示程序并处理其看门狗，不能只挂起带看门狗的程序。
本后端不会停止原程序或修改看门狗。

测试产物放在设备 `/mnt` 的独立新目录，不覆盖正在运行的 ELF。
本轮 `/mnt` 是 NFS：
`192.168.1.65:/home/dpower/data/Demo/NXF`。
确认实际挂载后，可以在主机复制已 strip 的程序、`DejaVuSans.ttf` 和版权文件；
这些路径是本轮环境，不是库内硬编码的部署路径。

在已核对的 root 设备会话中运行，`NX1_RUN_DIR` 换成实际目录：

```sh
NX1_RUN_DIR=/mnt/slint-nx1-release-001
SLINT_DEFAULT_FONT="$NX1_RUN_DIR/DejaVuSans.ttf" SLINT_BRIDGE_TRACE=1 \
  "$NX1_RUN_DIR/display" --framebuffer=/dev/fb0 --touch=/dev/input/event2 --seconds=20

SLINT_DEFAULT_FONT="$NX1_RUN_DIR/DejaVuSans.ttf" SLINT_MCP_PORT=9315 \
  "$NX1_RUN_DIR/showcase" --framebuffer=/dev/fb0 --touch=/dev/input/event2 --seconds=120
```

`--seconds` 正常退出并等待在途提交。`basic` 没有时限退出参数；
初次实机验证使用 `display`。测试结束后核对进程、描述符和会话锁释放；
退出不自动恢复原应用或初始画面。恢复旧应用前先确认测试进程已退出。
固件异常需要重启时，重新核对网络、NFS、显示所有者和看门狗状态。
不把杀掉进程或重启当作本次提交成功。

原生 MCP 使用独立本机端口，例如在已有且已核对的 ADB 连接上：

```sh
adb -s 192.168.1.152:5555 forward tcp:19318 tcp:9315
```

连接 `http://127.0.0.1:19318/mcp`，工具及截图限制见
[MCP 指南](../../../docs/slint-mcp.md)。
临时调试连接及转发只用于测试，结束后清理，不写入启动脚本。

## 内存与验证

本固件没有 `MemAvailable`。不要把 `MemFree + Cached` 简单当成可保证的空闲内存，
也不要照搬要求该字段的 NX5 截图工具。
本轮主要使用原生 MCP 查询/交互和 4 KiB 分块的物理页读取，PNG 在主机编码，
避免在设备上额外分配软件截图缓冲。
物理页采样前应使 UI 静止，确认 UI 线程等待、提交线程空闲，前后复核偏移；
页号读回仍不证明扫描输出或无撕裂。

主机回归使用 `cargo test --locked --offline --workspace --features nx1`。
独立的板级工具见 [工具说明](../../tools/nx1/README.md)。
