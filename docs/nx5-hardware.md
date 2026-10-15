# NX5 三项 TDE/翻页实机回归：2026-10-12

> 文中的 `NX5_SDK_INCLUDE` 外部路径保留采集环境；当前构建默认使用仓库资源，配置方法见 [使用指南](getting-started.md)。

此前未在 `949ceb9` 启动回归中重跑的三项 ignored 硬件测试已全部补齐。基于 `a9fd3c8d8534c4a0cd0f072da304e78a86b9d5f9` 重新构建 ARM 测试程序，逐项独占运行，每次只选择一个完整测试名称。该提交的库实现与 `949ceb9` 一致；本轮未修改运行时、测试代码、Slint、SDK、驱动或内核。

原始记录在本地 `evidence/nx5-hardware/2026-10-12-a9fd3c8/`，不随 Git 分发。证据链接需要该档案：[结构化结果](../evidence/nx5-hardware/2026-10-12-a9fd3c8/results.json)、[构建与恢复清单](../evidence/nx5-hardware/2026-10-12-a9fd3c8/deployment-manifest.json)。

## 设备核对与独占过程

- 通过 USB1 root 会话与 ADB `192.168.1.151:5555` 重新比对，启动 ID 均为 `938c34d4-456a-4b1a-b532-678c0f92efd0`，内核 Linux 4.9.138 #233。
- SDK、fyfb、vou 的 SHA256 均匹配当前板级准入。原显示进程为 PID 2925，参数 `--textures --touch=/dev/input/event0`；实际运行文件与备份摘要一致。
- 原程序经 MCP 暂停，帧号 24003 保持稳定，确认 `fb-present` 空闲后停止。SIGTERM 停止常驻程序不计入 Rust 正常/错误返回的清理测试，其内核回收记录保留在停止日志中。
- `/run` 初始仅剩约 2.8 MiB。程序先备份到主机，停止后分阶段部署测试二进制；测试完成并归档后移除设备端测试副本，再恢复原显示文件。
- 新测试二进制大小 4,931,568 字节，SHA256 为 `cac74ded13c41c02cfaf9c8e24a5c9e5e5bdb18aecdef9a258303dabefd43ca7`。使用 Cortex-A7/NEON 预设，ARM EABI5 soft-float、最高 GLIBC 依赖 2.18，没有直接链接 VideoDec。

## 本轮结果

| 测试 | PID / 耗时 | 结果 |
| --- | --- | --- |
| `nx5_hardware_compare` | 3291 / 10.01 s | 通过；复制、透明混合、裁剪、跨行源、COW 缓存及软件回退验证通过；80 帧增量重绘最大通道差 1/255 |
| `nx5_demo_pages_match_full_frames` | 3310 / 8.19 s | 通过；100 帧按 yoffset 600、0 交替，最大通道差 1/255，超容差通道为 0 |
| `blocking_flip_cadence` | 3330 / 5.28 s | 通过；128 次真实阻塞翻页，排除 8 次预热，120 次均值 34.479450 ms，即 29.003 FPS |

日志：[TDE 比较](../evidence/nx5-hardware/2026-10-12-a9fd3c8/device/hardware-compare.log)、[双页像素](../evidence/nx5-hardware/2026-10-12-a9fd3c8/device/hardware-pages.log)、[翻页节拍](../evidence/nx5-hardware/2026-10-12-a9fd3c8/device/hardware-flip.log)。三项均为 `1 passed; 0 failed; 0 ignored`，进程退出码均为 0。

TDE 比较中，不透明复制与 COW 输出精确一致；透明混合等路径允许的最大通道差为 1/255。预提交错误 `-77` 为测试主动注入，日志中的 `errors: 1` 和随后禁用 TDE 属于预期软件回退验证，不能当成真实运行错误或将该测试误报为“错误计数为零”。

翻页样本最小值 34.439 ms、中位数 34.480 ms、95 分位 34.501 ms、最大值 34.530 ms。该测试只交替已有显示页、不写入像素；数值描述此次阻塞提交节拍，不改变板级完成契约。

三项测试的开始与结束均确认 VO/WBC 未被其他进程占用，MMZ 为 4900 KiB / 2 块基线。每项退出后的 VO/WBC 均关闭，连续的测试期间内核记录中没有新增 `MMB LEAK` 或强制解除映射。内核日志使用有重叠的前后片段校验，避免因日志缺失漏报回收。

双页像素用例使用真实 TDE 和生产写页代码，但两页由内存文件承载；它不等于物理 LCD 的逐帧扫描认证。真实 framebuffer 的阻塞 PAN 由第三项单独覆盖。此前 WBC、EIO 和启动路径的结果保持各自报告的采集范围，本轮没有重跑它们。

## 常驻界面恢复

最终快照为 **2026-10-12 09:20:25（北京时间）**。已恢复同一份显示程序，PID **3351**，路径 `/run/slint-bridge-mcp/display-mcp`，参数仍为 `--textures --touch=/dev/input/event0`。SHA256 与备份一致：`f2fa90209df824c1b94bbddbf125a70190089f6ebc4aac24934c87dd6185dee5`。

Slint MCP 监听设备 localhost 9315，主机转发 19315。重新执行 18 个原生工具中的 6 个：1024×600 窗口，暂停帧号 7059 保持不变，恢复后从 7060 增长到 7078，最终 RUNNING；[截图](../evidence/nx5-hardware/2026-10-12-a9fd3c8/final-screen.png) 已查看，文字和纹理正常。截图由软件重新渲染，不作为硬件像素比较的依据。

恢复后 MMZ 为运行所需的 12100 KiB / 5 块，WBC 未运行；RSS 11080 KiB、3 个线程。持续性能日志、预加载及故障注入均关闭。设备启动 ID 与三项固件摘要再次核对一致，没有重启。见 [最终设备快照](../evidence/nx5-hardware/2026-10-12-a9fd3c8/device/final-device.txt)。

## 重现与验证范围

从仓库根交叉构建；运行前依照 [NX5 设备说明](../test-app/devices/nx5/README.md) 核对当前设备、备份并停止显示所有者：

```sh
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
cargo --config .cargo/nx5.toml test --locked --offline --release \
  -p slint-bridge --features nx5-tde --target arm-unknown-linux-gnueabi --lib --no-run
```

将打印出的测试文件部署为 `tde-tests`，在设备 root 会话分别执行，不并行启动，也不一次运行所有 ignored 测试：

```sh
export SLINT_DEFAULT_FONT=/run/slint-bridge-display/DejaVuSans.ttf
export LD_LIBRARY_PATH=/system/app/clib
./tde-tests backends::nx5::tde::tests::nx5_hardware_compare --exact --ignored --nocapture --test-threads=1
./tde-tests backends::nx5::tde::tests::nx5_demo_pages_match_full_frames --exact --ignored --nocapture --test-threads=1
./tde-tests backends::nx5::tests::blocking_flip_cadence --exact --ignored --nocapture --test-threads=1
```

本轮的 [采集脚本](../evidence/nx5-hardware/2026-10-12-a9fd3c8/run-hardware.sh) 与 [结果分析脚本](../evidence/nx5-hardware/2026-10-12-a9fd3c8/analyze.py) 保存在本地证据档案中；设备端测试程序已在归档后移除，复测需重新上传。分析检查精确测试名称、退出码、完整帧序列、容差、二进制摘要及前后资源/内核记录。

本轮执行了 ARM 构建、上述三项实机测试、恢复后的 MCP 检查及文档一致性检查。没有修改 Rust 行为或构建配置，因此没有重复执行无关主机 feature 矩阵；主机默认 ignored 的计数不作为本轮实机通过依据。
