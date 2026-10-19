# F1 只读查询

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
