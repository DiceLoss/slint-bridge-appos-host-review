# NX1 诊断工具

先读 [NX1 设备说明](../../devices/nx1/README.md)，核对当前身份、显示所有者、
触摸协议和恢复步骤。工具不会启动 SDK 或使用 HWC。

从仓库根构建：

```sh
NX1_CC=/opt/NX1/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc
mkdir -p target/nx1
"$NX1_CC" -O2 -Wall -Wextra -Werror test-app/tools/nx1/inspect-device.c -o target/nx1/inspect-device
"$NX1_CC" -O2 -Wall -Wextra -Werror test-app/tools/nx1/inject-touch.c -o target/nx1/inject-touch
```

- `inspect-device`：只读 fb0 的固定/可变信息和 event2 的轴范围，不写像素或翻页。
- `inject-touch X Y`：核验 `gt911_ts`、轴范围和无已有接触后，
  通过真实 event2 写入完整按下/释放帧；用于确认 evdev 解码、坐标变换和 UI 回调。
  这不验证实际传感器。必须仅在本次独占测试 UI 中运行，避免触发原业务功能。

本轮复用了 [4 KiB 物理页采样器](../f1/read-front.c) 和
[脱离串口启动器](../f1/run-detached.c)，按上述工具链交叉编译即可。
采样器固定检查本轮共同的 1024×600 BGRA8888 双页布局；
它只读 framebuffer，不调用 PAN、VSYNC 或 SDK。
输出应放在 `/mnt` 并传回主机处理，不能在低内存设备上保留多份 tmpfs 截图。

历史原始材料位于被忽略的 `evidence/nx1/2026-10-20/`，检出仓库后需另行取得。
其中失败的 HWC 探针仅是历史证据，**不得重复运行**。
