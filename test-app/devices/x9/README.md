# X9 / Lombo N7 测试目标

2026-10-14 当前常驻 `/tmp/slint-bridge-x9-20261014/showcase-06`，观测 PID **2367**，无退出定时器；性能采样后已恢复无探针进程，停在富列表纵向起点。最终 Controls 与 PNG/SVG 缩放两张真实前台页共 2,048,000 像素，与同条件 CPU 参考全部一致；完整 MCP 与故障验证范围见接入报告。富列表纵向 **6.64 FPS / 98.14% CPU / 17.53 MiB RSS**，横向 **10.73 FPS / 98.31% / 17.75 MiB**；CPU 为单核口径，详见 [性能报告](../../../docs/x9-performance.md)。操作前须重新识别进程，不能直接使用这里的 PID。

本目录与 NX5 独立。当前设备为 ADB `192.168.1.143:5555`、双 Cortex-A7、Linux `3.10.100 #452 SMP PREEMPT Mon Dec 12 11:32:50 CST 2022`，显示驱动 `lombo-drm`。身份与运行进程均须在每次操作前重新核对；[current-target.json](current-target.json) 是观测快照，不能直接沿用其中的 PID。

物理面板 800×1280，虚拟 800×2560，stride 3200，BGRA8888 / alpha 0–255。`--rotation=90` 后 Slint 逻辑界面为 1280×800。触摸设备 `/dev/input/event1` 为 `gt9xxnew_ts`，无 slot Type-A MT。当前 `fbdev` 只接入这套已核对固件和布局；未知设备仍拒绝初始化。

渲染全部由 CPU 完成，写现有 `/dev/fb0` 双页；没有 G2D、EGL、GBM 或 SDK 会话。DRM 只负责同一 FB 的匹配完成事件，PAN 单独返回不是旧页可复用的保证。完整契约、失败行为和输入协议见 [平台说明](../../../docs/software-platform.md)，调查及本轮验证见 [X9 报告](../../../docs/x9-framebuffer.md)。

## 构建

从仓库根目录执行，使用已安装的 `arm-unknown-linux-gnueabihf` Rust target：

```sh
SLINT_EMIT_DEBUG_INFO=1 cargo --config .cargo/x9.toml build --locked --offline --release \
  --target arm-unknown-linux-gnueabihf --features fbdev,slint/mcp \
  -p slint-bridge-test-app --bin showcase
```

[.cargo/x9.toml](../../../.cargo/x9.toml) 使用用户提供的 `/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf`，hard-float ABI、Cortex-A7。不要用 NX5 的 `gnueabi` 工具链替代。构建依赖同源 Slint 及旋转截图修复，版本见平台说明；Cargo.lock 不锁定外部 path 仓库提交。

## 部署和运行

ADB shell 为 uid 2000，不能直接访问 root 所有的 fbdev / evdev。上传使用 ADB，执行使用 DPower Terminal 中已核对的 **192.168.1.143 root Telnet 会话**。

操作前读取 `uname -a`、`/proc/sys/kernel/random/boot_id`、`/sys/class/net/eth0/address`，扫描当前 `/proc/*/fd` 的 framebuffer、DRM 和输入设备所有者。原始双页备份、偏移和恢复前提见接入报告。不要停止 `viss_isp` 等不拥有本次显示资源的服务，也不要操作 NX5 的会话。

新程序上传到新的目录/文件名，核对 SHA256 后启动；不要覆盖运行中的 ELF。字体使用 `SLINT_DEFAULT_FONT` 显式选择，当前为 DejaVuSans.ttf。示例中的 RUN_DIR 须换成本次新目录：

```sh
adb -s 192.168.1.143:5555 push target/arm-unknown-linux-gnueabihf/release/showcase RUN_DIR/showcase
adb -s 192.168.1.143:5555 push /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf RUN_DIR/DejaVuSans.ttf
```

在目标 root 会话中启动，先使用 `--seconds=20` 验证正常退出，确认描述符和控制台恢复后再去掉时限常驻：

```sh
SLINT_MCP_PORT=9315 SLINT_DEFAULT_FONT=RUN_DIR/DejaVuSans.ttf \
  RUN_DIR/showcase --framebuffer=/dev/fb0 --touch=/dev/input/event1 --rotation=90 --seconds=20
```

`--seconds` 调用 Slint 正常退出并等待在途提交。SIGKILL / 强制断电不能代替正常退出验证；不确定完成后程序禁止写页并保留 graphics 控制台，须先排除其他所有者，使用工具说明中的 `recover-console` 等待匹配事件后恢复；不能只强制切回文字模式继续写页。

MCP 使用独立端口，避免和 NX5 的 19315 混用：

```sh
adb -s 192.168.1.143:5555 forward tcp:19316 tcp:9315
```

连接 [原生 MCP endpoint](http://127.0.0.1:19316/mcp)，按 [MCP 指南](../../../docs/slint-mcp.md) 查询当前窗口句柄，不能复用重启前的句柄。

## 内存与诊断

本固件没有 `MemAvailable`，不能直接复用会要求该字段的 NX5 部署脚本。`/tmp` 和 `/run` 都占 RAM；部署或截图前读取 `MemFree`、`Shmem`，检查目录占用。本轮顺序截图要求 `MemFree > 32768 KiB`，高于 1280×800 两份 RGBA 缓冲的三倍；这不是并发截图的保证。先归档普通文件并核对 SHA256，再删除不被当前进程使用的旧产物。保留当前程序、字体、日志、锁和必要恢复资料。

只读实际前台页采样与 Type-A 输入注入工具见 [X9 工具说明](../../tools/x9/README.md)。MCP 软件截图不经过物理 framebuffer，不能单独证明真实页、evdev 或屏幕扫描输出。
