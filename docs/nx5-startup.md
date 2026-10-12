# NX5 窗口启动实机回归：949ceb9

提交 `949ceb98b320bb61e9006e984fecfc284ec904db` 已部署到 NX5。首次显示失败清理、同一组件重试和延迟创建窗口的五个用例各运行两轮，全部通过；另外完成正常 WBC 采样和 60 秒纹理运行，最后恢复常驻界面及 Slint MCP。

本轮没有修改适配库实现、Slint、厂商 SDK 或驱动。新增 [startup-regression](../test-app/src/bin/startup-regression.rs) 专用应用，通过库的公开 `init(Config)`、Slint 组件和真实 `/dev/fb0` 验证 `949ceb9`；界面定义独立保存在 [startup.slint](../test-app/ui/startup.slint)。它没有替换板级准入、提交回调或 TDE 资源。

原始证据位于本地 `evidence/nx5-startup/2026-10-11-949ceb9/`，不随 Git 分发。下列链接和重放命令需要取得该档案：[结构化结果](../evidence/nx5-startup/2026-10-11-949ceb9/results.json)、[部署清单](../evidence/nx5-startup/2026-10-11-949ceb9/deployment-manifest.json)。

## 设备与二进制

- USB1 root 会话与 ADB `192.168.1.151:5555` 的启动 ID 一致：`938c34d4-456a-4b1a-b532-678c0f92efd0`；Linux 4.9.138 #233。三项固件摘要匹配准入，本轮没有重启。
- 旧常驻 PID 2558 的二进制先备份并核对摘要，经 MCP 暂停、确认提交线程空闲后用 SIGTERM 停止。这次人为停止不计入正常/错误返回的清理回归。
- `/run` 初始剩余约 4.4 MiB，因此分阶段部署。专用启动程序完成测试、取回并核对摘要后从设备移除，再部署正式显示程序；主机保留两个程序及旧版本备份。
- 正式 `display` 为 `949ceb9` 的原始构建，SHA256 `f2fa90209df824c1b94bbddbf125a70190089f6ebc4aac24934c87dd6185dee5`；启用 `nx5-tde,slint/mcp`。
- 专用启动程序 SHA256 `3d7e1906f0f5aff4a4208e5358d92d52f4901d1562426c7fa4fc363f8e472cf0`，启用 `nx5-tde`，链接相同的 `949ceb9` 库实现。二者使用 Cortex-A7/NEON 预设、ARM EABI5 soft-float，最高 GLIBC 依赖为 2.18。

## 启动路径结果

每个用例都在初始化后确认一个 TDE target 和两个 source 已实际分配，共 7200 KiB。不是只检查退出码或最终 MMZ 数量。

| 用例 | 两轮 PID | 断言与结果 |
| --- | --- | --- |
| 首次显示失败 `show-error` | 2665、2753 | `min-width=2048px` 在 1024px 面板上被拒绝，窗口不可见，保留原始布局错误并以预期的退出码 1 返回 |
| 同一组件重试 `retry` | 2684、2769 | 同一组件连续失败两次后修正约束；原先排队的任务执行成功；随后两次 `run()` 均呈现新颜色，结束后不可见 |
| 排队创建 `queued` | 2702、2785 | 先排入 128 个任务，再排入创建窗口的任务；无窗口进入循环后仍能越过单轮任务数量限制并成功呈现 |
| 定时器创建 `timer` | 2718、2801 | 无窗口进入循环，150 ms 定时器创建首个根窗口并成功呈现 |
| 循环内失败后重试 `queued-retry` | 2734、2817 | 排队回调内首次显示被拒绝，窗口不可见；修正同一组件后显示，循环未被回滚产生的自动退出请求提前终止 |

重试用例每轮检查两帧，其余三个成功启动用例每轮各检查一帧：共 **10 帧、6,144,000 个像素，差异为 0**。每次请求的颜色均不同于原前台页首像素，避免旧画面造成假通过；重试用例覆盖 yoffset 0 和 600 两张物理页。

像素读取发生在事件循环返回、最后一次提交已完成之后。程序检查前台页全幅 BGRA 内容及读前后的 pan 一致性；它使用既有板级完成契约，读回页号本身不作为退休依据。该检查证明显示内存中的实际提交结果，不等同于物理 LCD 扫描输出认证。

十次退出后，MMZ 均回到 **4900 KiB / 2 块**，VO/WBC 均关闭；前后有重叠的内核记录中，没有对应 PID 的 `MMB LEAK`。证据见 [第一轮](../evidence/nx5-startup/2026-10-11-949ceb9/device/round1/) 和 [第二轮](../evidence/nx5-startup/2026-10-11-949ceb9/device/round2/)。SDK 正常关闭 VO 时仍有 `Force to close voDev(0)` 日志，与 TDE MMZ 泄漏回收分别判读。

## 常规显示与最终状态

原始 `949ceb9` 显示程序的正常 WBC 用例 PID 2844 通过：300 帧黑白各 150、混色 0，提交统计 28.81–29.00 FPS。纹理用例 PID 2886 连续运行 60 秒，11 个统计窗口中首窗 28.44 FPS、其余均为 29.00 FPS；最后统计为复制 1650 次、混合 1482 次、缓存命中 3130 次、TDE 错误 0。两次退出也均无 TDE 泄漏回收。见 [WBC 日志](../evidence/nx5-startup/2026-10-11-949ceb9/device/pattern.log) 和 [纹理日志](../evidence/nx5-startup/2026-10-11-949ceb9/device/textures.log)。

最终设备快照时间为 **2026-10-11 17:42:16（北京时间）**。常驻 PID **2925**，路径 `/run/slint-bridge-mcp/display-mcp`，参数 `--textures --touch=/dev/input/event0`，摘要与上述正式程序一致。持续性能日志、预加载及故障注入均关闭；MMZ 为运行时所需的 12100 KiB / 5 块，WBC 未运行，RSS 快照 11140 KiB。见 [最终快照](../evidence/nx5-startup/2026-10-11-949ceb9/device/final-device.txt)。

MCP 设备端口 9315、主机转发端口 19315。重新验证了 18 个原生工具中的 6 个：1024×600 窗口，暂停帧号 1779 保持不变，恢复后递增，最终 RUNNING；[截图](../evidence/nx5-startup/2026-10-11-949ceb9/final-screen.png) 已查看，文字和纹理正常。MCP 截图是软件重新渲染，不替代上述实际 framebuffer 读取或 WBC 合成输出采样。

本轮没有重跑原先三个 ignored TDE/翻页测试和 EIO 注入，历史结果见 [177c18a 报告](nx5-tde-exit.md)；混色负对照的历史结果见 [2cd98fa 报告](nx5-regression.md)。这些均不计入本轮通过数量。显式退出请求的保留由 `949ceb9` 的主机回归覆盖，此处没有另做实机注入。

**后续补测：**上述三项 ignored TDE/翻页测试已于 2026-10-12 逐项独占运行并全部通过，见 [独立补测记录](nx5-hardware.md)。本文继续保留启动回归当时的原始范围。

## 重现

从仓库根构建专用程序，运行前按 [设备说明](../test-app/devices/nx5/README.md) 核对目标、备份并停止当前显示所有者：

```sh
SLINT_EMIT_DEBUG_INFO=1 \
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
cargo --config .cargo/nx5.toml build --locked --offline --release \
  -p slint-bridge-test-app --features nx5-tde \
  --target arm-unknown-linux-gnueabi --bin startup-regression
```

将程序及 [run-startup.sh](../test-app/tools/nx5/display/run-startup.sh) 部署到新的设备目录，创建全新的输出目录；本轮的设备端专用程序已为释放空间移除，复测需重新上传。通过 root 会话运行：

```sh
mkdir /run/slint-bridge-startup-949ceb9/round3
sh /run/slint-bridge-startup-949ceb9/run-startup.sh \
  /run/slint-bridge-startup-949ceb9/startup-regression \
  /run/slint-bridge-startup-949ceb9/round3 show-error
# show-error 预期返回 1；其余用例预期为 0。
for mode in retry queued timer queued-retry; do
  sh /run/slint-bridge-startup-949ceb9/run-startup.sh \
    /run/slint-bridge-startup-949ceb9/startup-regression \
    /run/slint-bridge-startup-949ceb9/round3 "$mode"
done
```

取回日志及 `*.state/` 快照后执行 [检查器](../test-app/tools/nx5/display/check-startup.py)：

```sh
python3 test-app/tools/nx5/display/check-startup.py evidence/nx5-startup/2026-10-11-949ceb9/device/round1/show-error
python3 test-app/tools/nx5/display/check-startup.py evidence/nx5-startup/2026-10-11-949ceb9/device/round1/retry
```

检查器要求运行中存在真实 TDE 分配，退出后 MMZ/VO/WBC 干净，内核记录覆盖整次运行，无该 PID 的泄漏回收，且窗口状态、创建顺序、任务执行及全部像素满足预期。八种破坏后的证据均被正确拒绝：泄漏、缺少 TDE、内核日志断档、错像素、旧画面、错误可见状态、丢失任务及错误退出码。

新增诊断程序和脚本的 feature 矩阵、格式、严格 Clippy、启用 MCP 的 workspace 测试及 ARM 构建均通过。库内仍为 33 个主机测试通过、3 个实机测试默认忽略；另有 3 个 WBC 生命周期和 2 个公共接口测试通过。新专用应用的设备结果单独按上述十个用例记录。
