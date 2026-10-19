# F1 集成状态

目标为 `fd_f1_r88r30_spinor-20c31b4578`（ArtInChip D211，RISC-V 64 位）。
设备参数和翻页机制依据见 [调查报告](../../../docs/f1-display.md)。

当前提供 `f1` / `f1-g2d` 构建目标、板级模块和候选 GE normal-mode 画布。
**显示完成契约尚未验证，`init()` 在任何设备访问前拒绝；还不能运行 Slint 界面。**
没有对应固件源码时，不能通过改用一次 VSYNC、固定等待或页号读回绕过此限制。

运行行为集中维护于 [平台说明](../../../docs/software-platform.md#构建依赖)。
本目录不提供停止原应用或接管显示的命令，当前任务要求原应用继续运行。
最新只读查询得到 GE version=0x100、mode=0；模式查询不验证绘制作业。

## 构建

需要已安装 `riscv64gc-unknown-linux-gnu` Rust target 和 `/opt/F1` T-HEAD
GCC 10.2.0 工具链。编译器、自带 sysroot、动态加载器必须配套。
从仓库根执行：

```sh
cargo --config .cargo/f1.toml build --locked --offline --release \
  -p slint-bridge-test-app --features f1-g2d \
  --target riscv64gc-unknown-linux-gnu --bin basic
```

生成文件在 `target/riscv64gc-unknown-linux-gnu/release/basic`，解释器应为
`/lib/ld-linux-riscv64xthead-lp64d.so.1`。只要准入未开放，运行它仍应返回
`DisplayNotAdmitted`；最小程序可用于独立验证 ELF/加载器与预期准入拒绝，
不能作为显示测试。仅 CPU 的集成构建将 feature 改为
`f1`。其他业务 workspace 也须自行配置 RISC-V 链接器；Cargo feature 不能
替代 target/ABI 选择。

## 验证与下一步

设备内存较小，所有测试固件及较大测试产物放到 SD 卡 `/tmp/sdcard`，
不放在 RAM 文件系统 `/tmp` 或 `/run`。部署前重新读取 `/proc/mounts`、
`df -h /tmp/sdcard` 和 `/proc/meminfo`，确认 SD 卡已挂载及空间/内存足够；
未挂载时停止部署，不在 RAM 上创建同名目录。使用独立版本目录，
保留原应用；移除测试文件前先取回日志并核对摘要。

- 主机 feature 矩阵包含 F1 的公共 API、软件像素回归、G2D 回调模拟和
  失败后画布隔离；错误的板级 feature 组合必须拒绝。
- [查询工具](../../tools/f1/README.md) 仅查询 fb/evdev/GE 状态，原应用保持运行。
  UAPI 校验工具对照随仓库头文件检查 ioctl 编码、结构大小与字段偏移。
- 尚未执行真实 GE 填充、DMA 缓存一致性、硬件故障注入、显示接管或物理页验证。
  编译/主机测试通过不等于这些检查通过。
- 下一阶段先核验当前固件的 GE 成功/失败及资源释放行为；显示部分仍需证明
  提交归属、配置锁存、旧页读取终止和超时后的恢复。没有源码时可继续对当前
  内核镜像进行针对性分析，但现有参考工程没有提供所缺的显示完成机制。
