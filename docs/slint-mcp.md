# 设备端 Slint MCP

启用及调用说明对应当前源码；实机工具清单、截图和计数来自 2026-10-11 的接入记录，早于独立测试工程整理。当前测试分布见 [平台说明](software-platform.md)。本文的原始证据为不随 Git 分发的本地档案。

`c03c3bb` 阶段的 7 项原生工具、实体触摸及截图见 [路径实机报告](nx5-configured-paths.md)。`bccf6b9` 的首次截图 OOM 见 [审查修复报告](nx5-review-fixes.md)；此后归档清理历史临时产物，在同一进程中十次截图通过，最低可用内存 47.2 MiB，见 [截图内存复验](nx5-screenshot-memory.md)。下方初次接入阶段的 16/18 计数保留历史口径。 当前 `2de9168` 的实体输入与原生 MCP 对照录制、受保护截图及回退文件见 [触摸部署报告](nx5-touch-response.md)。

已在当前 NX5 上启用本地 Slint 1.18.1 源码树自带的 MCP 服务，并通过其原生工具完成 UI 查询、交互和截图。没有新增自制 MCP 服务，也没有修改外部 Slint 源码。

NX5 专用的 30 FPS 钳制已移除。`Config::frame_interval` 原样传入运行时，默认仍为 16667 微秒；`display` 示例定时器也使用该配置。页面所有权、最多一次提交和最多一帧预渲染的约束保持不变。实际显示吞吐仍受当前驱动约 29 FPS 的完成节拍约束。

## 启用及连接

MCP 作为开发工具通过构建参数启用，不加入默认 Cargo features。构建使用同一个 Slint 源码树，并保留 UI 元数据：

```sh
SLINT_EMIT_DEBUG_INFO=1 \
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
cargo --config .cargo/nx5.toml build --locked --offline --release \
  --target arm-unknown-linux-gnueabi --features nx5-tde,slint/mcp -p slint-bridge-test-app --bin display
```

将 `target/arm-unknown-linux-gnueabi/release/display` 按 [NX5 设备说明](../test-app/devices/nx5/README.md) 通过部署脚本上传为新的 `/tmp/slint-bridge-next/display` 后，在没有其他显示所有者的情况下，通过 DPower Terminal 的 USB1 root 会话启动：

```sh
SLINT_MCP_PORT=9315 \
SLINT_DEFAULT_FONT=/run/slint-bridge-display/DejaVuSans.ttf \
LD_LIBRARY_PATH=/system/app/clib \
/tmp/slint-bridge-next/display --textures --touch=/dev/input/event0
```

这里沿用当前设备已部署的字体及 SDK 路径。日常 MCP 会话不启用持续的 `SLINT_BRIDGE_TRACE`；需要性能采样时，配合 `--seconds=120` 运行有限时长并保存日志。`/run` 是临时文件系统，重启后需重新部署。

当前 Slint 的 `platform::set_platform()` 已负责安装 MCP 初始化钩子。本适配层不需要再手动初始化 testing backend；窗口显示且事件循环运行后，服务才开始监听。没有 `SLINT_MCP_PORT` 时不启动服务。

服务只监听设备的 `127.0.0.1:9315`。主机通过现有 ADB 连接转发：

```sh
adb -s 192.168.1.151:5555 forward tcp:19315 tcp:9315
```

Agent 使用 Streamable HTTP endpoint `http://127.0.0.1:19315/mcp`。这是 JSON-RPC 接口，不是浏览器页面。可以交给支持 Streamable HTTP 的 MCP 客户端，也可以直接调用原生协议：

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

随后通过 `tools/call` 调用返回的工具。先调用 `list_windows`，再用返回的 `windowHandle` 调用 `get_window_properties`，从其 `rootElementHandle` 遍历 UI。句柄字段是字符串，窗口句柄和元素句柄不能混用；进程重启后重新查询。

示例提供稳定元素 ID `App::pause-area` 和 `App::status`。`find_elements_by_id` 参数名是 `elementsId`，不是 `elementId`。本地版本的 `query_element_descendants` 已从递归后代开始，直接传类型/ID 过滤即可；额外添加 `matchDescendants` 会增加一次后代层级，漏掉直接子元素。

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "find_elements_by_id",
    "arguments": {
      "windowHandle": {"index": "1", "generation": "1"},
      "elementsId": "App::pause-area"
    }
  }
}
```

上面的句柄只是本次单窗口运行的例子，实际请求以查询结果为准。截图工具 `take_screenshot` 返回 MCP image 内容块及 PNG 的 base64 数据；保存到文件时解码该 image 块，不把 base64 作为终端文本展示。

## 截图前的内存检查

本板 `/run` 和 `/tmp` 均为 tmpfs，文件内容占用物理内存；各挂载点的 `df` 剩余容量不等于系统可用内存。1024×600 软件截图同时需要两份像素缓冲，合计约 4.7 MiB，编码和协议处理还需额外内存。部署新诊断程序或截图前，在已核对的设备上检查：

```sh
awk '/^MemAvailable:|^Shmem:|^SwapTotal:/' /proc/meminfo
df -k /run /tmp
du -k /run/slint-* /tmp/slint-*
```

先把已完成测试的普通文件取回主机并核对摘要，再清理设备副本；归档时跳过指向 framebuffer 和输入设备的链接。用当前进程的 `exe`、`maps`、`fd`、启动配置核对依赖，保留运行程序、字体、SDK、活动日志、设备链接及独占锁，并保留一份必要恢复程序。不要累积多轮测试二进制，也不要把 `/run` 整体清空。

日常截图统一使用 [部署工具](../test-app/tools/nx5/README.md#部署归档清理和截图) 的受检查入口，输出目录须存在、文件须不存在：

```sh
python3 test-app/tools/nx5/deploy.py --serial 192.168.1.151:5555 screenshot \
  evidence/nx5-deploy/screenshot-01.png
```

入口先检查 16 MiB 最低可用内存，再查询实际窗口尺寸，并在调用原生 `take_screenshot` 前立即检查 `max(16 MiB, 3 × 两份 RGBA 像素缓冲)` 的余量；字段缺失或不足均拒绝。图像直接保存到主机。归档清理也由同一工具维护，校验完成后再删设备副本，保留规则见 `deploy-retain.json`，不再依赖一次性清理命令。

此检查覆盖部署脚本发出的顺序截图。其他 MCP 客户端仍应在截图前执行同样的设备内存检查，避免并发截图或其他进程占用使检查后的余量失效。当前实测峰值增加约 5 MiB；16 MiB 是本板当前尺寸的保守操作余量，不是任意场景的保证。最新实测和历史清理清单见 [截图内存报告](nx5-screenshot-memory.md)。

## 本地 Slint 版本的 18 个工具

以下清单来自设备的 `tools/list` 响应，而非仅根据文档推断。

| 用途 | 原生工具 |
| --- | --- |
| 窗口发现与尺寸 | `list_windows`、`get_window_properties` |
| UI 树、元素属性与定位 | `get_element_tree`、`get_element_properties`、`find_elements_by_id`、`query_element_descendants` |
| 彩色 UI 截图 | `take_screenshot` |
| 点击、拖动与悬停 | `click_element`、`drag_element`、`hover_element`、`move_pointer` |
| 滚轮 | `scroll_element`、`dispatch_pointer_scroll` |
| 可访问语义操作 | `invoke_accessibility_action`、`set_element_value` |
| 键盘 | `dispatch_key_event` |
| 记录与读取输入事件 | `start_event_recording`、`stop_event_recording` |

本次实际执行了其中 16 个工具。演示没有文本输入、滑块或语义动作处理器，因此没有声称验证 `set_element_value`、`invoke_accessibility_action` 的控件效果。

## 接入阶段的实机结果

- 服务返回 `slint-mcp-embedded`，protocol `2025-06-18`；窗口为 1024×600，元素树共 29 项。
- 原生 ID 查询定位暂停区与状态文本；点击后状态变为 `PAUSED`，后续查询帧计数不变；恢复后状态为 `RUNNING`，帧计数继续增加。
- 点击、拖动、悬停、移动、滚轮与按键注入产生 74 条录制事件，丢失和未知事件计数均为 0。未设置处理器的按键/滚轮不据此认定为业务操作成功；录制结果保留 Accepted/Rejected 区别。
- 通过 `take_screenshot` 获得并查看运行、暂停状态的 1024×600 PNG，UI 文本与纹理正常。
- 操作与截图会占用 UI 线程；包含主动暂停的帧率窗口不能作为连续运行性能。恢复后可继续观察到 29.00 FPS，TDE 错误为 0。
- 移除钳制后的 WBC 回归采到黑白各 150 帧，300 帧无混色。原先每个 UI tick 切色的图案在新的更新频率下发生奇偶混叠（300 帧全白，测试正确判失败）；现仅将诊断颜色保持两个 tick，没有恢复运行时限帧。
- 混色负对照检出 30/30 帧，说明保留的 WBC 检查仍能拒绝坏图案。普通 `fbdev`、`nx5`、`nx5-tde` feature 矩阵通过；当时启用 MCP 的 `nx5-tde` 库内主机测试为 26 通过、3 个实机测试默认忽略，格式与严格 Clippy 检查通过。独立工程整理后的测试数量另见平台说明。

证据：[工具及 schema](../evidence/nx5-mcp/tools-list.json)、[实际调用汇总](../evidence/nx5-mcp/mcp-verification.json)、[录制事件](../evidence/nx5-mcp/final-events.json)、[暂停截图](../evidence/nx5-mcp/final-screen-paused.png)、[运行截图](../evidence/nx5-mcp/final-screen-running.png)、[显示日志](../evidence/nx5-mcp/validation-run.log)。同目录的 `*.request.json` 保存原生请求。

硬件样本另见 [WBC 回归](../evidence/nx5-mcp/pattern.log) 和 [被正确拒绝的单色测试](../evidence/nx5-mcp/pattern-default-cadence.log)。

## 对原有验证方法的替代范围

| 现有任务 | MCP 能否替代 | 处理 |
| --- | --- | --- |
| 看 UI 内容、位置、尺寸和交互结果 | 可以 | 使用元素树、属性、截图和点击等原生工具 |
| `display --capture` 用于日常 UI 截图 | 可以接管 UI 检查用途 | 删除示例分支以及 `libobserver.so` 的 `bridge_wbc_capture` 导出；不再为看 UI 加载 WBC 截图插件 |
| 检查 Slint 收到的输入事件 | 可以 | 使用原生事件录制；结合元素状态确认业务效果 |
| evdev 二进制解码、SYN_DROPPED、旋转坐标和设备掉线 | 不可以 | 保留 [输入回归](../src/tests/input.rs) 与 [事件循环回归](../src/tests/runtime.rs)；MCP 注入绕过 Linux evdev 解码 |
| 两页不被提前改写、脏区修复、stride/像素格式、提交失败 | 不可以 | 保留 framebuffer 与 presentation 回归；MCP 不读取物理页也不注入提交故障 |
| TDE 硬件像素比较、软件回退、未知 DMA 完成和资源生命周期 | 不可以 | 保留 Rust/C 的 TDE 比较及故障注入测试 |
| 实际合成输出、混色检测、驱动翻页节拍与像素时钟 | 不可以 | 保留 `--observe`、WBC 采样/负对照、翻页及调度探针 |

Slint MCP 的截图最终调用 `Window::take_snapshot()`。当前软件 renderer 会重新绘制到独立的 RGBA 缓冲区，再编码 PNG；它没有经过本适配层的 TDE `TargetPixelBuffer`、双 framebuffer 写页或物理显示扫描。因而漂亮的 MCP 截图不能证明 TDE、显示页或屏幕没有问题。

`test-app/tools/nx5/present/wbc-capture.c` 仍作为历史硬件输出探针保留，不再编译进日常显示插件。它的硬件观测含义与 MCP 截图不同；显示运行时仍不能启动另一个加载 SDK 的采集进程。旧报告和历史证据保留原始测试口径。

MCP 提供观察和交互能力，并不会自动替代底层断言。此次没有删除覆盖不同路径的回归测试，也没有另写一套 UI 截图或输入服务器。
