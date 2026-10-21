# 文档索引

文档按用途组织。当前接口和行为以平台说明为准，业务接入看使用指南，应用参数看测试工程，实机命令看设备及工具说明。实验报告保留版本、场景和验证边界，不代表设备实时状态。

## 当前使用

| 文档 | 内容 |
| --- | --- |
| [获取配置与使用](getting-started.md) | 用户业务工程的 Cargo Git 接入、四板配置、构建、部署及依赖更新 |
| [开发与维护](development.md) | 维护者的仓库检查、测试程序构建、MCP 与依赖升级验证 |
| [平台实现说明](software-platform.md) | 公共 API、feature、输入、画布、双页、提交及错误契约 |
| [Slint MCP](slint-mcp.md) | 原生 UI 查询、交互、事件录制和受内存保护的截图 |
| [测试应用](../test-app/README.md) | 页面、参数、素材、测试及业务使用方式 |
| [NX5 设备与工具](../test-app/devices/nx5/README.md) | [部署归档与诊断工具](../test-app/tools/nx5/README.md)、SDK/字体、回归运行前提 |
| [NX1 设备与工具](../test-app/devices/nx1/README.md) | ARM32 构建、`/mnt` 部署、参考 PAN 与 [只读检查和 evdev 验证](../test-app/tools/nx1/README.md) |
| [X9 设备与工具](../test-app/devices/x9/README.md) | [DRM/evdev 与性能工具](../test-app/tools/x9/README.md)、旋转及控制台恢复 |
| [F1 设备与工具](../test-app/devices/f1/README.md) | RISC-V 构建、G2D 路径、[查询、ABI 与离屏验证](../test-app/tools/f1/README.md)；参考 PAN/VSYNC 上屏与性能采样 |
| [随仓库资源](../third-party/README.md) | SDK 头文件、动态库、字体、版权及 SHA256 |

[项目结构图](diagrams/project-structure.html) · [双页时序图](diagrams/runtime-sequence.html)。`diagrams/` 同时保留图源及 SVG/PNG/HTML 导出。

## 机制与实测

| 文档 | 内容 |
| --- | --- |
| [NX1 framebuffer 接入](nx1-framebuffer.md) | 当前内核反汇编、PAN/HWC 限制、CPU 双页适配及实机验证 |
| [NX1 透明合成](nx1-transparency.md) | 低透明度亮色圈、DE2 最终输出预乘、真实下层叠加与会话恢复 |
| [NX1 渲染与内存分析](nx1-performance.md) | 分阶段计时、RSS/PSS 与 ION 内存账、保持画质的纹理复制原型及优化优先级 |
| [NX5 显示完成与合成](nx5-display.md) | #22 准入缺口、#233 双周期、单周期未达成、SDK/WBC 与 alpha 量程 |
| [NX5 性能记录](nx5-performance.md) | TDE、写页、输入分段、图片/缩放、富列表、MCP 增长对照及内存总账 |
| [NX5 MMA 背景验证](nx5-mma-background.md) | 已有源槽共享、内存对照、物理像素与软件回退 |
| [NX5 验证记录](nx5-validation.md) | 启动/退出失败与修复、真实输入、界面、物理双页及资源清理 |
| [X9 接入调查](x9-framebuffer.md) | DRM 完成契约、控制台、Type-A 输入、旋转及实际页验证 |
| [F1 显示接入调查](f1-display.md) | 原应用运行中的只读查询、BGRA8888 与触摸轴、配置时序、参考驱动完成机制及未验证范围 |
| [F1 图片与透明合成优化](f1-optimization.md) | GE 混合、64 位写页、PNG 硬解、SVG 行跨度修复、RSS 来源、富列表应用减量与最小 UI 成本对照 |
| [F1 指令集调查](f1-isa-research.md) | C906FD / RV64GC、玄铁标量扩展、RVV 边界及实机像素微基准 |
| [F1 图片解码调查](f1-image-decoder-research.md) | VE 格式、实际 SDK ABI/吞错问题、有界原生 PNG 与 DMA 完成条件 |
| [F1 showcase 实测](f1-showcase.md) | CPU/G2D 对照、原生 MCP、物理前台页、内存与最终部署 |
| [F1 当前内核镜像分析](f1-kernel-analysis.md) | 固定镜像的 PAN/VSYNC/IRQ 调用链、GE 完成路径、可重放反汇编及旧页退休缺口 |
| [X9 性能与流水线](x9-performance.md) | 初始基线、旋转/写页、全屏与卡片缓存、渲染热点、线程边界、内存总账、分配跟踪及 MCP 保留归因 |
| [Slint 纹理优化对照](slint-optimization.md) | X9 批量混合、NX5 兼容性、Pixel 接入、适配层迁移、Slint 回退、SVG 栅格缓存三板对照及剩余修改审计 |

## 独立专题

- [程序体积分析](nx5-binary-size.md)：ELF 与资源占比、无损压缩及功能保留边界。
- [透明区域与视频接入调查](nx5-video-regions.md)：公共组件方案及尚未验证的真实视频集成。

## 历史与证据

旧的逐次报告已合并到上述主题，重复的常驻 PID、构建命令和恢复叙述不再单独维护。合并后的阶段锚点保留旧报告名，例如 `nx5-display.md#nx5-flip`；设备快照中的报告引用同步指向这些章节，历史数值、版本和时间戳不因此更新。

需要旧文档全文时，从本次整理之前的 Git 提交读取相应路径，例如 `git show 7609a83259b1:docs/nx5-flip.md`。删除文档不删除原始测试档案；`evidence/` 仍被 Git 忽略，需另行取得，重放旧日志不算新实测。当前观测见 [NX5 快照](../test-app/devices/nx5/current-target.json) 和 [X9 快照](../test-app/devices/x9/current-target.json)，执行前重新核对实际设备。

2026-10-19 按用户要求清理本地 `evidence/`：编译出的程序与探针、原始像素帧（含压缩副本）及反汇编记录直接删除，不再归档。
占用从约 5.33 GiB 降至 722 MiB，释放约 4.63 GiB；旧混合压缩包也剔除了编译产物，保留成员的内容已逐项校验。
日志、JSON 摘要、脚本、PNG 截图、翻页事件记录及设备原始镜像/厂商库保留，19218 个未改动散文件通过 SHA256 核对。
历史报告的结论和原始摘要不改写，但其中引用的 ELF、原始像素与反汇编文件已不再完整，不能直接按旧命令重放这些输入；旧包的历史摘要也不代表清理后容器的摘要。
删除清单、旧包与保留成员校验及空间统计位于本地忽略目录 `evidence/_cleanup/2026-10-19/`，汇总见 `summary.json`。
