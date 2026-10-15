# NX5 图片背景裁剪与多个透明区域

2026-10-15，`BackgroundWithHoles` 已按应用侧背景裁剪方案实现。使用 Slint 公共模型 API、`clip` 和偏移 Image；本轮没有修改 Slint、库运行时、板级提交、SDK 或驱动。组件、模型字段和接入方式集中在 [测试工程说明](../test-app/README.md#图片背景中的多个透明区域)。

## 条件与实现

设备为 USB1 / ADB `192.168.1.151:5555`、Linux #233、1024×600 BGRA framebuffer，启动 ID `4aa4f5d2-8de2-42cb-9c2b-44c3e74d6e2b`。运行库基于 `095eca7`，新增应用组件及测试；Slint 保持 `fa6a6d0158c9730fed62ef1eb1c9730c80214e20`。交叉构建使用 `.cargo/nx5.toml`、`nx5-tde,slint/mcp` 和 UI 调试元数据。最终复建 strip ELF 与实测 ELF 逐字节一致：

```text
background-holes: e9630815aab2023601d57ce54390bdb04fd9a439a09a6a703a064fb3ad536d22
libbackground-checker.so: 42d5d6883c8846872af38c8d8d9aea51eedacc369f0e1c4e81a05a850de54351
```

背景先扣除可见矩形的并集，再绘制剩余的互不重叠区域；每个片段引用同一张原尺寸图片。前景按钮、半透明浮层与弹窗作为后续兄弟组件照常合成。移动、缩放或隐藏区域仅更新模型，Slint 负责重绘。本轮复现了 Rust 回调未登记模型依赖时，`VecModel::set_row_data` 不触发重绘的问题；通过公开的行数跟踪和 `row_data_tracked` 修复，保留了先失败再通过的日志。

独立演示窗口提供 A/B 两个区域的移动、缩放、隐藏/恢复，以及跨区域按钮、弹窗和复位。图片按源像素与逻辑像素 1:1 绘制；当前演示限定本轮面板大小。任意图片缩放、圆角开口和视频坐标同步不在本轮范围。

## 验证结果

| 层次 | 本轮结果 |
| --- | --- |
| 主机组件像素 | 同一保留缓冲内 22 个连续状态、270336 像素通过；独立逐点区域并集预期，alpha 精确、RGB 容差 1。覆盖重叠、越界、整数端点、空/负尺寸、模型增删/替换、隐藏恢复、图片替换和前景/弹窗 |
| NX5 物理双页 | `--verify` 的 23 个阶段全部通过，页号逐次交替；23 次前台页与 22 次后台页，共 27648000 像素。前台对应当前状态，后台对应上一状态；本轮匹配参考的 RGB 最大误差 **0**，alpha 编码完全一致 |
| 检查器负对照 | 首轮故意破坏参考图一个像素的 alpha，检查器准确检出 1 个错误并拒绝 |
| 原生 MCP | 7 个交互状态、2392091 个未被前景遮盖位置的 alpha 断言通过；另查看初始截图。两区域重叠、分别缩放、全部隐藏及复位正常 |
| 前景交互 | 跨区域按钮计数正常；弹窗打开后点击底层操作和原按钮均不改变状态，关闭后继续工作。56 条事件全部 Accepted，丢失/未知均为 0 |
| 正常退出 | 验证进程返回 0，VO 释放，MMZ 回到 **4900 KiB / 2 块**；该验证窗口无新 MMZ 强制回收或 OOM |
| 工程检查 | `fbdev` / `nx5` / `nx5-tde` feature 矩阵、格式、严格 Clippy、公开文档构建及 ARM 交叉构建通过 |

物理页检查在同一组件的每次事件循环退出、运行时完成最后提交后读取页。软件截图是期望值，实际值来自只读 framebuffer；读取时不加载第二份 SDK，也不启动 WBC。每次重新运行会请求重绘，连续保留缓冲的局部重绘另由主机测试与常驻 MCP 交互覆盖；本轮没有测量持续动画帧率或扫描输出。

旧常驻 PID 22711 在提交线程空闲后通过 SIGTERM 停止，内核回收其 TDE MMZ 的记录与新验证进程的正常退出窗口分开，不能把信号退出当作 Rust 正常析构。首次打包传输因固件没有 tar 改为逐文件部署，未解包的包已校验归档清理。一次在调整验证程序入口期间并行启动的 feature 检查读到已移除的临时文件而失败；最终固定入口后的完整矩阵通过，两个日志均保留。

## 常驻与恢复

验收后保留交互演示，观测 PID **412**，路径 `/tmp/slint-bridge-holes-20261015/background-holes`，启动脚本 `start.sh`，日志 `live.log`。两区域已复位显示，弹窗关闭、按钮计数为 0；无退出定时器、trace、WBC 或检查插件加载。清理后 RSS **12008 KiB**，VmHWM **16816 KiB**（包含截图峰值），MMZ **12760 KiB / 6 块**，可用内存 **44636 KiB**。

原 `/tmp/slint-bridge-rich-render-2fddc04/showcase` 与 `start.sh` 作为恢复程序保留；`1225593` 及本轮诊断文件共 11 个已按部署工具的 SHA256 归档流程移除。保留策略和环境快照已更新。恢复前重新核对设备、实际 PID、提交空闲及显示所有权，退出演示后用 `sh /tmp/slint-bridge-rich-render-2fddc04/start.sh` 启动原程序。路径均位于 tmpfs，设备重启后需重新部署。

本轮确认的是图片背景中的透明 UI 与两张实际显示页。尚未连接视频解码器或非黑色视频底层，没有新增 WBC、LCD 扫描、evdev 注入、实体手指或 X9 实机验证。NX5 当前底层为黑色，开口看起来是黑色；像素检查确认开口 alpha 为 0。

## 原始证据

本地档案位于 `evidence/background-holes/2026-10-15/`，被 Git 忽略，检出仓库后需另行取得：

- `single-hole-red.log`、`model-red.log`、对应 green 日志、`host-sequence.log`：组件复现与修复。
- `verify.log`、`results.json`：23 阶段真实双页结果及负对照。
- `initial.png`、`01-overlap.png` 至 `07-restored.png`、`mcp-result.json`、`mcp-events.json`、`check-mcp.py`：原生 UI 操作及像素断言。
- `exit.mmz`、`exit.vo`、`kernel-verification-delta.txt`：正常退出与资源检查。
- `feature-matrix-final.log`、`clippy.log`、`cargo-doc.log`、`arm-build-reviewed.log`、`manifest.json`：主机、构建及源码摘要。
- `deploy-*`、`cleanup-final/`：上传、旧恢复程序和诊断文件的归档校验凭据。

重新采集的构建、部署和运行命令见 [双页检查工具说明](../test-app/tools/nx5/README.md#图片背景透明区域的双页检查)。
