# Slint 原生 MCP 使用

本页维护当前 UI 查询、交互、事件录制及截图入口。服务来自同源 Slint，不增加自制截图服务器。MCP 软件截图和输入注入不能替代物理 framebuffer、evdev、TDE/DMA 或扫描输出验证；历史实测统一见 [NX5 验证记录](nx5-validation.md) 和 [X9 接入记录](x9-framebuffer.md)。

## 启用及连接

构建启用 `slint/mcp` 并设置 `SLINT_EMIT_DEBUG_INFO=1`，以保留元素 ID 等运行时元数据。两设备的完整构建命令见 [使用指南](getting-started.md#验证并构建示例)。运行时设置 `SLINT_MCP_PORT=9315`；没有该变量时不启动服务。字体、SDK、显示所有权及有限运行时长按各自设备说明配置。

`platform::set_platform()` 安装原生 MCP 钩子，不需要再手动初始化 testing backend；窗口显示且事件循环运行后才开始监听。服务监听设备 localhost，主机通过已核对的 ADB 目标转发：

```sh
# 以下地址为测试环境示例，先核对实际设备。
adb -s 192.168.1.151:5555 forward tcp:19315 tcp:9315
adb -s 192.168.1.143:5555 forward tcp:19316 tcp:9315
```

NX5 使用 `http://127.0.0.1:19315/mcp`，X9 使用 `http://127.0.0.1:19316/mcp`。这是 Streamable HTTP JSON-RPC 接口，不是浏览器页面。先初始化并查询当前进程的工具定义：

```sh
curl --noproxy '*' -sS http://127.0.0.1:19315/mcp \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  --data '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"agent","version":"1"}}}'
curl --noproxy '*' -sS http://127.0.0.1:19315/mcp \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  --data '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
```

## 查询与交互

通过 `tools/call` 调用工具。先 `list_windows`，用返回的 `windowHandle` 查询 `get_window_properties`，再通过 `rootElementHandle` 遍历元素。原样使用返回的句柄，不混用窗口和元素句柄，进程重启后重新查询。

| 用途 | 常用工具 |
| --- | --- |
| 窗口与属性 | `list_windows`、`get_window_properties` |
| UI 树、定位 | `get_element_tree`、`get_element_properties`、`find_elements_by_id`、`query_element_descendants` |
| 图像 | `take_screenshot` |
| 指针与滚动 | `click_element`、`drag_element`、`hover_element`、`move_pointer`、`scroll_element`、`dispatch_pointer_scroll` |
| 语义和键盘 | `invoke_accessibility_action`、`set_element_value`、`dispatch_key_event` |
| 录制 | `start_event_recording`、`stop_event_recording` |

参数以运行进程的 `tools/list` 为准。当前 `find_elements_by_id` 使用 `elementsId`；`query_element_descendants` 已从后代开始，额外添加 `matchDescendants` 会漏掉直接子元素。稳定 ID 和完整检查脚本见 [测试工程](../test-app/README.md#展示应用-showcase)。截图返回 MCP image 块及 PNG base64，保存时解码图像块，不把 base64 打到终端。

事件录制使用 1024 条环形缓冲。长拖动分成短窗口，并检查 `droppedCount`、`unknownEventCount`；非零不能按完整样本验收。时间戳在分派处理后，包含回调耗时；不能把它直接称为触摸传感器到屏幕的时延。性能窗口不同时截图或录制输入。

## 截图前的内存检查

NX5 的 `/run`、`/tmp` 是 tmpfs，文件占物理内存。1024×600 软件截图同时需要两份 RGBA 缓冲，约 4.7 MiB，编码和协议还有额外开销。先按 [归档清理流程](../test-app/tools/nx5/README.md#部署归档清理和截图) 处理已完成产物，保护当前程序、恢复文件、依赖和活动引用。

日常 NX5 截图使用受检查入口，主机输出目录须存在、文件须不存在：

```sh
python3 test-app/tools/nx5/deploy.py --serial 192.168.1.151:5555 screenshot \
  evidence/nx5-deploy/screenshot-01.png
```

工具先检查 16 MiB 最低可用内存，再查询尺寸，并在调用前要求至少 `max(16 MiB, 3 × 两份 RGBA 缓冲)` 余量，字段缺失或不足则拒绝；PNG 直接保存到主机。其他 MCP 客户端也要遵守该检查，顺序截图的检查不能保证并发操作安全。

X9 没有 `MemAvailable`，不套用 NX5 脚本；按 [X9 设备说明](../test-app/devices/x9/README.md#内存与诊断) 检查 MemFree、Shmem 和当前窗口所需余量。NX5 历史 OOM 与清理复验见 [验证记录](nx5-validation.md#nx5-screenshot-memory)。

## 验证边界

MCP 适合 UI 状态、布局、点击/拖动、键盘和可访问语义检查。它不经过真实触摸传感器，也不读取物理前台页；软件重绘甚至可能掩盖保留缓冲漏刷。底层回归与独占设备要求继续由 [NX5 工具](../test-app/tools/nx5/README.md) 和 [X9 工具](../test-app/tools/x9/README.md) 维护。
