# NX5 实机回归：2026-10-11

被测实现为提交 `2cd98fae4959073f7070c3e3540a50b384e8bb03`。三项实机测试、WBC 错误退出、双页输出采样及纹理运行通过；**完整显示进程退出时仍存在 TDE 缓冲由内核强制回收的问题**，本轮保留为待修复项。

**后续状态：**上述问题已在 [TDE 退出清理修复](nx5-tde-exit.md) 中定位并通过新一轮实机验证。本文继续保留 `2cd98fa` 的原始失败结果。

原始记录在本地 `evidence/nx5-regression/2026-10-11-2cd98fa/`，不随 Git 分发。下文证据链接需要该档案；[结构化结果](../evidence/nx5-regression/2026-10-11-2cd98fa/results.json) 的整体状态为 `completed_with_tde_exit_cleanup_issue`。

## 设备与部署

- DPower Terminal USB1、`192.168.1.151:5555`，Linux 4.9.138 #233；启动 ID 为 `938c34d4-456a-4b1a-b532-678c0f92efd0`，本轮没有重启或修改驱动。
- SDK、fyfb、vou 三项 SHA256 均匹配代码准入。显示为 1024×600、32 bpp、stride 4096、虚拟高度 1200。
- 旧进程 PID 1338 经 MCP 暂停，确认提交线程空闲后停止；旧二进制已备份并核对摘要。此次 SIGTERM 停止不计入 Rust 错误返回清理测试。
- `/run` 初始只剩约 6.8 MiB，测试二进制和正式程序分阶段部署。三项测试完成归档后移除设备端测试二进制，再部署正式程序。
- 正式程序仍使用 `/run/slint-bridge-mcp/display-mcp`。新 SHA256 为 `3d4efe2ec1911e118b7873f5f0a0ecce539e75863b9dab68c09c508e3a8dac8a`；实机测试二进制 SHA256 为 `0be64441ffb31081367174de7f7abea38bc312fa8d82919d9eb8298cde861d5e`。二者最高 GLIBC 依赖均为 2.18。

最终身份与进程核对时间为 **2026-10-11 15:21:17（北京时间）**。最新常驻 PID 2040，参数为 `--textures --touch=/dev/input/event0`；MCP 端口为设备 localhost 9315，主机转发 19315。未启用 `LD_PRELOAD`、输入错误注入或持续 `SLINT_BRIDGE_TRACE`。原始信息见 [最终设备快照](../evidence/nx5-regression/2026-10-11-2cd98fa/final-device.txt) 和 [部署清单](../evidence/nx5-regression/2026-10-11-2cd98fa/deployment-manifest.json)。PID 属于本次快照，后续操作仍需重新核对。

## 回归结果

| 检查 | 本轮结果 | 证据 |
| --- | --- | --- |
| `nx5_hardware_compare` | 通过；复制/COW 等精确一致，透明混合最大通道差 1/255，80 帧增量重绘和各项软件回退通过 | [日志](../evidence/nx5-regression/2026-10-11-2cd98fa/hardware-compare.log) |
| `nx5_demo_pages_match_full_frames` | 通过；100 帧页偏移交替，最大通道差 1/255，超容差通道为 0 | [日志](../evidence/nx5-regression/2026-10-11-2cd98fa/demo-pages.log) |
| `blocking_flip_cadence` | 通过；128 次提交中排除 8 次预热，120 次平均 34.479858 ms，29.002 FPS | [日志](../evidence/nx5-regression/2026-10-11-2cd98fa/flip-cadence.log) |
| 正常 WBC 双页采样 | 300 帧，黑白各 150，混色 0；提交统计 28.81–29.00 FPS | [日志](../evidence/nx5-regression/2026-10-11-2cd98fa/pattern.log) |
| 混色负对照 | 30/30 帧检出混色，观察器结果为 1，符合预期 | [日志](../evidence/nx5-regression/2026-10-11-2cd98fa/mixed.log) |
| WBC 活跃期间输入出错 | EIO 后继续采集 566 帧；总计 600 帧、混色 0；等待约 19.75 秒后，观察器完成清理，应用再返回错误码 1 | [日志](../evidence/nx5-regression/2026-10-11-2cd98fa/fault-retry/fault.log) |
| 60 秒纹理运行 | 11 个完整统计窗口，启动 28.44 FPS，其后 10 个均为 29.00 FPS；TDE 错误为 0 | [日志](../evidence/nx5-regression/2026-10-11-2cd98fa/textures.log) |
| 最新界面 MCP | 1024×600；暂停后帧号 15675 保持不变，恢复后为 15693；最终为 RUNNING | [交互记录](../evidence/nx5-regression/2026-10-11-2cd98fa/final-ui-verification.json) |

三项实机测试分别单独运行，退出码均为 0，结束后 VO 关闭、MMZ 回到 4900 KiB / 2 块基线。双页逐像素测试使用内存文件承载两页、使用真实 TDE 和生产写页代码；真实合成输出由 WBC 另行覆盖。WBC 每行检查五个横向位置，采集约 29 FPS，不能扩大为全部物理 LCD 帧或全部像素的无撕裂认证。

MCP 截图重新进行软件渲染，不作为硬件输出证明。[最终界面](../evidence/nx5-regression/2026-10-11-2cd98fa/final-running-screen.png) 已查看，文字和纹理正常。本轮使用了 18 个原生工具中的 6 个，不沿用之前“16 个工具已执行”的计数，也不将 MCP 点击当作真实触摸面板操作验证。

## WBC 异常退出方法

被测显示程序保持原二进制不变。[inject-read-error.c](../test-app/tools/nx5/display/inject-read-error.c) 是仅用于本次诊断的 `LD_PRELOAD` 库：触发文件出现后，仅对指定 evdev 路径的一次 `read()` 返回 EIO，其他描述符和后续读取透传。它不加载 SDK，不断开输入设备，不改驱动。

[run-regression.sh](../test-app/tools/nx5/display/run-regression.sh) 等待至少 30 条真实 WBC 帧记录，并核对驱动的接收计数、速率及 `vou-wbc` 分配仍存在，然后创建触发文件。有效用例保存在 `fault-retry/`：

1. 注入前 WBC 正在采集，MMZ 包含 `vou-wbc`。
2. 日志出现一次 `injected evdev read EIO`，之后仍有 566 条采样记录。
3. `display: observation result=0` 出现在 `Error: read evdev input...` 之前，应用最终返回 1。
4. WBC 分配在退出后消失，内核日志没有该进程的 WBC 强制回收记录。

C 的 summary 受 stdout 缓冲影响，可能在 Rust 的 Error 行之后才写入文件，不能据此推断清理顺序。这里用观察器返回、后续采样、进程存活时长及资源记录共同判断。

首次尝试误用了寄存器 `0x34101090` 的 bit 0 作为活动条件。该寄存器在本固件实际采集中仍读为 0，脚本因此没有注入错误；其 `fault.*` 记录保留为 [未注入对照](../evidence/nx5-regression/2026-10-11-2cd98fa/fault-attempt-one.json)，不计入异常退出通过项。后续以实际帧、驱动计数和分配状态确认活动；寄存器原始读数不参与清理判定。

## 待修复：完整进程退出的 TDE 清理

正常定时退出、混色用例和输入错误退出均出现相同内核记录：该进程的 `bridge_tde_targe` 及两个 `bridge_tde_sourc` 分配被标记为 `MMB LEAK`，随后由内核强制解除映射并回收。每块 2457600 字节，合计 7200 KiB。详见 [按 PID 汇总](../evidence/nx5-regression/2026-10-11-2cd98fa/exit-resource-observation.json)。

因此，退出后的 MMZ 虽回到基线，也不能证明用户态 TDE 释放成功。本轮未发现这些分配跨进程持续累积，但**主动释放要求尚未满足**。WBC 清理通过与这个问题分别记录。

此前主机回归验证的是显式销毁 `Framebuffer` 时的顺序，本轮暴露的是完整应用退出行为。原因尚未定位；本轮保持被测提交不变，没有通过修改要求或重新分类日志消除该问题。后续修复需重新检查正常退出和 EIO 退出，确认不再产生这些内核强制回收记录。

## 复跑入口

先按 [NX5 设备说明](../test-app/devices/nx5/README.md) 核对目标、显示所有者、固件和恢复条件；每个用例独占显示，等待上一用例完成后再启动。三项忽略测试命令见 [TDE 报告](nx5-tde.md#构建与接入)。

主机编译错误注入库（无 SDK 依赖）：

```sh
: "${NX5_CC:=/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc}"
mkdir -p target/nx5-display
"$NX5_CC" -std=gnu11 -O2 -Wall -Wextra -Werror -shared -fPIC \
  test-app/tools/nx5/display/inject-read-error.c -ldl \
  -o target/nx5-display/inject-read-error.so
```

将 runner 和注入库部署到新的设备测试目录，注入库命名为 `inject-read-error.so`。观察器插件仍须位于 `/run/slint-bridge-display/libobserver.so`。以下为设备 root 会话命令示例；目录及程序需预先部署：

```sh
sh /run/nx5-regression/run-regression.sh \
  /run/slint-bridge-mcp/display-mcp /run/nx5-regression pattern
# 前一项结束后依次将最后一个参数改为 mixed、fault、textures。
```

脚本拒绝覆盖已有用例日志；`.done` 表示结果和快照已收集，仍须判读内容。`fault` 的预期应用退出码为 1；`mixed` 的观察器结果为 1，而当前应用退出码仍为 0。正常 pattern 日志使用 `check-run.py <日志> --wbc` 判读。常驻演示恢复时移除诊断环境，关闭持续 TRACE，再通过原生 MCP 确认 RUNNING。
