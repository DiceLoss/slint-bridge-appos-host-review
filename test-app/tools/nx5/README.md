# NX5 设备工具

这些工具验证 Slint MCP 不覆盖的硬件路径。目录内的 `build.sh`、`build-observer.sh` 和 `check-native.sh` 会定位仓库根；诊断输出继续放在根 `target/nx5-*`，历史证据继续从根 `evidence/nx5-*` 读取。

`target/` 和 `evidence/` 均不纳入 Git。下方证据重放命令需要已有的本地档案；它们不采集新数据，也不证明当前二进制已通过实机测试。当前进程内 WBC 插件的构建、固定部署路径和判读命令见 [NX5 设备说明](../../devices/nx5/README.md)。`present/` 中的独立 SDK 探针属于 #22 旧环境，不能直接套用于正在运行 UI 的 #233 设备。

| 目录 | 用途 | 对应记录 |
| --- | --- | --- |
| `display/` | 进程内 WBC 采样、混色负对照、输入 EIO/TDE 退出、窗口启动回归与日志检查 | [最新实机回归](../../../docs/nx5-startup.md) |
| `tde/` | SDK 搜索路径/会话复用、原生故障注入及 TDE 输出结果重放 | [TDE 验证](../../../docs/nx5-tde.md) · [三项实机补测](../../../docs/nx5-hardware.md) |
| `present/` | 早期板级准入调查与原始观测判读 | [早期准入记录](../../../docs/nx5-present-validation.md) |
| `flip/` | 阻塞 PAN 周期与实际刷新时序分析 | [翻页分析](../../../docs/nx5-flip.md) |
| `single-cycle/` | 队列、待锁存与当前页状态诊断 | [单周期调查](../../../docs/nx5-single-cycle.md) |
| `fps/` | 已保存的整屏优化证据重放 | [帧率记录](../../../docs/nx5-fps.md) |
| `parallel/` | 已保存的并行渲染/翻页证据重放 | [并行记录](../../../docs/nx5-parallel.md) |
| `scheduling/` | 定时等待探针及已保存调度证据重放 | [调度记录](../../../docs/nx5-scheduling.md) |

UI 截图、元素查询和交互直接使用 [Slint MCP](../../../docs/slint-mcp.md)。不要把硬件专有工具放入通用测试入口，也不要默认批量执行设备探针；部分工具要求暂停显示、独占设备或特定固件。

`display/run-startup.sh` 驱动专用 `startup-regression` 应用，保存首次显示失败、重试和延迟创建的资源及内核记录；`display/check-startup.py` 检查窗口状态、真实像素、任务执行及 TDE 清理，拒绝旧画面或缺失证据。步骤见 [启动实机回归](../../../docs/nx5-startup.md)。

`display/run-regression.sh` 在设备上执行有限时长用例并保存日志、退出码和资源快照；`display/inject-read-error.c` 只用于诊断进程内的一次 evdev EIO。`display/check-exit.py` 核对活动 TDE 分配、退出后的 MMZ、连续内核记录及 EIO/WBC 清理顺序。构建、部署、预期失败和证据判据见最新实机回归报告。原始寄存器读数不能替代实际 WBC 帧、驱动计数及分配状态。

从仓库根重放现有证据：

```sh
python3 test-app/tools/nx5/present/test_analyze.py
python3 test-app/tools/nx5/tde/check-results.py
python3 test-app/tools/nx5/single-cycle/analyze.py evidence/nx5-single-cycle/stages.log
```

下列判据目前应失败，表示没有达成单周期目标，不能改为放宽验收：

```sh
python3 test-app/tools/nx5/single-cycle/analyze.py evidence/nx5-single-cycle/stages.log --min-fps 55
```

`NX5_SDK_INCLUDE=<codec头文件目录> sh test-app/tools/nx5/tde/check-native.sh` 在主机运行原生故障注入和像素转换边界检查，并用替身 SDK 验证：从两个临时目录（含空格路径）加载无版本库名 `libVideoDec.so`，TDE 在显示会话加载前拒绝自行加载、加载后复用同一会话，构造函数只运行一次，引用正确释放。该检查不加载真实 SDK，也不访问设备。
