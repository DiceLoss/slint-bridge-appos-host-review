# 文档索引

当前接口、默认值、运行约束和源码对应关系集中在 [平台实现说明](software-platform.md)；应用用法集中在 [独立测试工程](../test-app/README.md)。新增或修改行为时，同步更新这两个入口及相关命令。下方实测报告保留各阶段的数据，不能把其中的旧实现、测试计数或 PID 当作当前状态。

## 日常入口

- [项目 README](../README.md)：目录、库的公开接口与 Cargo features。
- [独立测试工程](../test-app/README.md)：用例、`.slint` 界面、应用代码及多设备选择方式。
- [平台实现说明](software-platform.md)：事件循环、输入、双 framebuffer 和板级契约。
- [Slint MCP](slint-mcp.md)：原生 UI 检查和交互工具、连接步骤及覆盖限制。
- [NX5 设备说明](../test-app/devices/nx5/README.md) 与 [当前环境](current-environment.md)。
- [项目结构图](diagrams/project-structure.html) 与 [运行时序图](diagrams/runtime-sequence.html)。

## NX5 实测与调查记录

这些报告记录各轮实验的条件、结果及限制。根 `evidence/` 是被 Git 忽略的本地档案，包含原始日志、截图和摘要；仅检出仓库不会获得这些文件，报告中的证据链接和离线重放命令需另行取得对应档案。诊断工具在 `test-app/tools/nx5/`，重放旧日志不等于重新验证当前二进制。

| 记录 | 内容 |
| --- | --- |
| [自定义路径与动态布局实机验证](nx5-configured-paths.md) | `c03c3bb` 自定义设备路径、真实点触、双页像素、WBC 动态加载及正/负对照 |
| [审查修复与会话重试](nx5-review-fixes.md) | `bccf6b9`：同进程初始化重试、VO/描述符清理、WBC 正负对照及纹理退出；记录 MCP 截图 OOM 与恢复限制 |
| [三项 TDE/翻页实机回归](nx5-hardware.md) | `a9fd3c8` 重建，三项 ignored 测试逐项独占通过；退出资源检查和界面/MCP 恢复 |
| [窗口启动实机回归](nx5-startup.md) | `949ceb9` 的首次显示失败清理、同一组件重试、延迟创建、实际前台像素与退出资源检查 |
| [TDE 退出修复回归](nx5-tde-exit.md) | 平台/窗口引用环修复，正常与 EIO 退出回归、三项硬件测试和常驻界面恢复 |
| [2cd98fa 实机回归](nx5-regression.md) | 提交 2cd98fa 的三项硬件测试、WBC 错误退出与双页采样；首次记录 TDE 清理问题 |
| [早期准入](nx5-present-validation.md) | 早期固件的观测、负对照与证据缺口 |
| [上屏验证](nx5-display.md) | 当前设备双页显示及 WBC 合成输出采样 |
| [TDE](nx5-tde.md) | 纹理加速、像素比较与软件回退 |
| [整屏帧率](nx5-fps.md) | CPU/NEON 和写页性能 |
| [并行化](nx5-parallel.md) | UI 渲染与后台翻页的重叠 |
| [调度](nx5-scheduling.md) | 内核定时精度及等待策略 |
| [翻页机制](nx5-flip.md) | 实际刷新时序和两周期提交 |
| [单周期调查](nx5-single-cycle.md) | 队列与影子锁存观测；尚未实现安全单周期路径 |

硬件诊断的用途和运行前提见 [NX5 工具索引](../test-app/tools/nx5/README.md)。目录整理不会把历史结果改写为新测试结果。
