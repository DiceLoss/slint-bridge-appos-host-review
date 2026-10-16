# 文档索引

文档按用途组织。当前接口和行为以平台说明为准，业务接入看使用指南，应用参数看测试工程，实机命令看设备及工具说明。实验报告保留版本、场景和验证边界，不代表设备实时状态。

## 当前使用

| 文档 | 内容 |
| --- | --- |
| [获取配置与使用](getting-started.md) | 两个 Git 仓库、新机器环境、独立业务工程及 NX5/X9 构建配置 |
| [平台实现说明](software-platform.md) | 公共 API、feature、输入、画布、双页、提交及错误契约 |
| [Slint MCP](slint-mcp.md) | 原生 UI 查询、交互、事件录制和受内存保护的截图 |
| [测试应用](../test-app/README.md) | 页面、参数、素材、测试及业务使用方式 |
| [NX5 设备与工具](../test-app/devices/nx5/README.md) | [部署归档与诊断工具](../test-app/tools/nx5/README.md)、SDK/字体、回归运行前提 |
| [X9 设备与工具](../test-app/devices/x9/README.md) | [DRM/evdev 与性能工具](../test-app/tools/x9/README.md)、旋转及控制台恢复 |
| [随仓库资源](../third-party/README.md) | SDK 头文件、动态库、字体、版权及 SHA256 |

[项目结构图](diagrams/project-structure.html) · [双页时序图](diagrams/runtime-sequence.html)。`diagrams/` 同时保留图源及 SVG/PNG/HTML 导出。

## 机制与实测

| 文档 | 内容 |
| --- | --- |
| [NX5 显示完成与合成](nx5-display.md) | #22 准入缺口、#233 双周期、单周期未达成、SDK/WBC 与 alpha 量程 |
| [NX5 性能记录](nx5-performance.md) | TDE、写页、输入分段、图片/缩放、富列表及与 X9 的 MCP 内存增长对照 |
| [NX5 验证记录](nx5-validation.md) | 启动/退出失败与修复、真实输入、界面、物理双页及资源清理 |
| [X9 接入调查](x9-framebuffer.md) | DRM 完成契约、控制台、Type-A 输入、旋转及实际页验证 |
| [X9 性能与流水线](x9-performance.md) | 初始基线、旋转/写页、全屏与卡片缓存、渲染热点、线程边界、内存总账、分配跟踪及 MCP 保留归因 |
| [Slint 纹理优化对照](slint-optimization.md) | X9 批量混合、NX5 兼容性及 Pixel 接入三轮结果 |

## 独立专题

- [程序体积分析](nx5-binary-size.md)：ELF 与资源占比、无损压缩及功能保留边界。
- [透明区域与视频接入调查](nx5-video-regions.md)：公共组件方案及尚未验证的真实视频集成。

## 历史与证据

旧的逐次报告已合并到上述主题，重复的常驻 PID、构建命令和恢复叙述不再单独维护。合并后的阶段锚点保留旧报告名，例如 `nx5-display.md#nx5-flip`；设备快照中的报告引用同步指向这些章节，历史数值、版本和时间戳不因此更新。

需要旧文档全文时，从本次整理之前的 Git 提交读取相应路径，例如 `git show 7609a83259b1:docs/nx5-flip.md`。删除文档不删除原始测试档案；`evidence/` 仍被 Git 忽略，需另行取得，重放旧日志不算新实测。当前观测见 [NX5 快照](../test-app/devices/nx5/current-target.json) 和 [X9 快照](../test-app/devices/x9/current-target.json)，执行前重新核对实际设备。
