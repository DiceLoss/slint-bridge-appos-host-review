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
| [TDE 写页与格式转换](nx5-tde-writeback.md) | 单次转换能力实测、CPU 转换后 TDE 写页对照、4915200 个实际页像素核验 |
| [共享 MMZ 画布](nx5-shared-canvas.md) | CPU/TDE 共用映射、移除回拷、真实物理页写回和错误整帧重绘 |
| [填充与整体透明度混合](nx5-fill-blend.md) | QuickFill 与 CPU 对照、整体 opacity 缓存、硬件像素及故障验证 |
| [Controls 百分比圆环修复](nx5-controls-ring.md) | 固定整圈改为百分比圆弧，五档/两种状态及保留缓冲回退验证 |
| [图片列表优化与像素复测](nx5-image-optimization.md) | 图片格式/TDE 对照、精确 NEON 混合、缓存淘汰导致的标题漏刷与实际页核验 |
| [图片背景部署与性能复测](nx5-image-background.md) | `41f87b2` 实际 PNG 背景与半透明界面；列表约 20.37 FPS、其余场景 29 FPS、RSS/CPU 和实体输入对照 |
| [展示应用性能实测](nx5-showcase-performance.md) | `308afe5` 动画/列表/透明交互的完成提交帧率、CPU/RSS、实体触摸配对及切页时延 |
| [六页展示应用部署与验证](nx5-showcase.md) | `308afe5` 六页触屏界面、主机/设备 MCP 回归、正常退出资源、实体操作确认及 `f963558` 回退 |
| [透明 UI 量程修复与部署实测](nx5-alpha.md) | 287da8c 的真实合成失败、NX5 0–128 alpha 编码修复、实际页/WBC、帧率和触摸验证 |
| [slint-nx5 历史经验与本次吸收](slint-nx5-lessons.md) | 30 个历史提交、输入/页生命周期/SDK 的教训、透明输出修复、两槽缓存优化及 MMZ 清理保护；区分主机、ARM CPU 测试和待接入的视频合成 |
| [触摸队列修复部署与实测](nx5-touch-response.md) | `2de9168` 常驻切换，10 组实体触摸、35 条 Accepted 事件、分派时延及 `bccf6b9` 回退保留 |
| [自定义路径与动态布局实机验证](nx5-configured-paths.md) | `c03c3bb` 自定义设备路径、真实点触、双页像素、WBC 动态加载及正/负对照 |
| [截图内存清理与复验](nx5-screenshot-memory.md) | 归档清理 41.66 MiB 历史产物；同一 `bccf6b9` 进程十次截图通过，最低可用内存 47.2 MiB |
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
