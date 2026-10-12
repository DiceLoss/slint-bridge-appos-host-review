# Slint Bridge 测试工程

这是独立的 Cargo package `slint-bridge-test-app`，通过 `path = ".."` 依赖适配库，按真实应用的方式调用 `slint_bridge::init(Config)`。它与库处于同一 workspace，共用仓库根的 `Cargo.lock` 和 `target/`。

界面、应用逻辑、测试素材及设备工具分别存放：

```text
test-app/
├── Cargo.toml                 # 转发板级 feature，依赖公开库接口
├── build.rs                   # 编译 ui/app.slint
├── src/
│   ├── lib.rs                 # 生成的界面类型、设备路径配置
│   ├── main.rs                # display：运动、纹理、暂停/恢复
│   ├── bin/basic.rs           # basic：最小接入示例
│   ├── bin/init-retry.rs      # NX5 同进程初始化失败与资源清理验证
│   ├── bin/startup-regression.rs # NX5 启动/重试/退出实机验证
│   └── nx5_observer.rs        # NX5 专用进程内 WBC 观察器
├── ui/
│   ├── app.slint              # 导出测试界面
│   ├── basic.slint            # 不依赖设备尺寸的基础窗口
│   ├── display.slint          # 当前 1024×600 运动/纹理场景
│   └── startup.slint          # 可拒绝布局与纯色像素验证
├── fixtures/textures.rs       # 确定性的纹理素材；底层 TDE 比较也复用它
├── tests/public_api.rs        # 仅使用适配库公开接口的回归
├── devices/nx5/               # 当前设备记录与构建/运行说明
└── tools/
    ├── check-feature-matrix.py
    └── nx5/                  # WBC、TDE、翻页等硬件诊断及历史证据重放
```

以下命令从仓库根目录执行。根目录不加 package 选择时默认只检查适配库；使用 `-p slint-bridge-test-app`、`--manifest-path test-app/Cargo.toml` 或进入本目录可单独选择测试工程，使用 `--workspace` 检查两者。

## 从最小例子看接入方法

先看 [src/bin/basic.rs](src/bin/basic.rs) 和 [ui/basic.slint](ui/basic.slint)：

```rust
use slint::ComponentHandle;
use slint_bridge_test_app::{BasicWindow, config_from_args};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    slint_bridge::init(config_from_args())?;
    BasicWindow::new()?.run()?;
    Ok(())
}
```

`config_from_args()` 是测试工程的辅助函数，最终产生库的普通 `Config`。业务工程也可以直接构造 `Config`，不依赖本测试工程。

也可先安装平台并投递创建/显示窗口的任务，再调用 `slint::run_event_loop()`；窗口尚未创建时任务和定时器仍可执行。首次显示因布局约束失败会回滚可见状态，修正同一组件后可以重试。运行行为与回归范围见 [窗口创建与显示失败](../docs/software-platform.md#窗口创建与显示失败)。

选择设备由编译期 feature 决定；运行时路径由参数决定：

| 项目 | 选择方式 |
| --- | --- |
| 通用 framebuffer 软件组合 | `--features fbdev` |
| NX5 软件渲染 | `--features nx5` |
| NX5 TDE 和软件回退 | `--features nx5-tde` |
| framebuffer 路径 | `--framebuffer=/dev/fb0`，默认 `/dev/fb0`，可替换为目标设备路径或设备链接 |
| 触摸设备 | `--touch=/dev/input/event0`，默认不开启输入设备 |
| Slint MCP | 构建加 `slint/mcp`，并按 [MCP 指南](../docs/slint-mcp.md) 设置环境变量 |

必须选择一个板级 feature；`fbdev` 和 `nx5` 互斥。NX5 使用板级阻塞翻页契约，不比对固定内核版本、驱动名称或文件摘要；`fbdev` 在真实设备运行时仍会拒绝未验证的提交契约。测试工程不绕过这个检查。

## 用例与预期

| 用例 | 运行选择 | 验证内容 |
| --- | --- | --- |
| 最小显示 | `--bin basic` | 从公开接口初始化平台并创建窗口 |
| 运动与触摸 | `--bin display -- --touch=/dev/input/event0` | 连续更新；点击暂停、再次点击恢复 |
| 纹理与透明混合 | `--bin display -- --textures` | 大图、透明图和运动组合；是否实际使用硬件加速由板级统计确认 |
| MCP 交互 | `display` 加 MCP 构建和运行环境 | 元素树、截图、点击/拖动、输入事件录制 |
| NX5 合成输出 | `display --pattern --observe=300 --seconds=20` | WBC 黑白样本；需先部署 NX5 观察器插件 |
| NX5 混色负对照 | 上一项加 `--split`，采样改为 `--observe=30` | 应报告混色，不能当作通过 |
| NX5 初始化清理 | `--bin init-retry`，需 `nx5` 或 `nx5-tde` | 同进程中 framebuffer/输入初始化失败、重试与 UI 线程退出后的 VO/TDE 清理 |
| NX5 窗口启动 | `--bin startup-regression`，仅 `nx5-tde` | 首次显示失败清理、同一组件重试、延迟创建和真实前台页像素 |

`basic` 只使用 `--framebuffer` 和 `--touch` 路径配置。`display` 另外支持 `--textures`、`--pattern`、`--split`、`--observe=N` 和 `--seconds=N`；这些选项使用表中所示的 `--name=value` 或独立开关格式。旋转、按键映射和帧间隔通过 Rust `Config` 配置，当前没有对应命令行参数。

WBC 按无版本库名 `libobserver.so` 通过系统搜索路径加载，部署目录可由 `LD_LIBRARY_PATH` 指定；新插件接收应用配置的 framebuffer 路径，并通过 ioctl 获取采样尺寸，旧插件需重新构建。

`--split` 与 `--pattern` 配合使用；`--observe=N` 仅在 NX5 构建中启用，在启动约 2 秒后创建观察线程。`--seconds=N` 请求事件循环退出；进程还会等待在途提交和已启动的观察线程结束。正常退出显式等待 WBC，事件循环返回错误时由观察器资源所有者自动等待，保证采集线程结束后才返回应用错误。若观察线程也 panic，清理时记录该错误并保留原 UI 错误；正常退出时观察线程 panic 仍会作为错误返回。

观察器的采样结果写入日志，当前应用不会将非零采样结果转换成非零退出码，验收需使用 [check-run.py](tools/nx5/display/check-run.py) 检查日志。

`startup-regression` 是独占 NX5 的专用验证程序，不加入日常显示流程。参数为 `show-error|retry|queued|timer|queued-retry` 加输出目录，可追加 `--framebuffer=PATH`、`--touch=PATH`；`show-error` 保留预期布局错误并返回 1，其余用例应返回 0。它通过公开平台接口运行真实组件，按设备 ioctl 获取实际宽高、stride 和前台页，逐行检查实际像素并保存 TDE 状态；完整退出由外层脚本检查。构建、采集与结果见 [启动实机回归](../docs/nx5-startup.md)。

`init-retry OUTPUT_DIRECTORY [--framebuffer=PATH] [--touch=PATH]` 需要已有的输出目录并独占设备。它连续执行两次 framebuffer 路径错误和两次输入路径错误，然后成功启动并结束一个 UI 线程；在进程仍存活时断言 VO 已关闭、无 TDE MMZ 分配，核对重复失败及 UI 退出后的设备描述符缓存不增长，并保存 `.vo`、`.mmz`、`.fds` 快照。按设备说明构建时将 `--bin display` 换为 `--bin init-retry`；正常应返回 0，不能与显示进程同时运行。

基础窗口适合新设备接入检查；运动/纹理场景使用窗口实际尺寸显示分辨率、安排绘图区和限制运动范围。默认不打开触摸设备，检查暂停/恢复时需指定 `--touch`，或使用 MCP 注入交互。

MCP 使用 Slint 自带功能，不另建截图或输入服务器。稳定元素 ID 为 `App::pause-area` 和 `App::status`。先发现窗口与句柄，再执行以下流程：

1. 获取窗口属性、元素树及运行截图。
2. 开始事件录制，点击暂停区，确认状态为 `PAUSED` 且帧计数停止。
3. 再点击恢复，确认状态为 `RUNNING` 且帧计数增加。
4. 停止录制，核对事件是否被接受及是否丢失，再截图检查结果。

工具 schema、完整连接方式和原生功能列表见 [MCP 指南](../docs/slint-mcp.md)。

## 主机检查与独立构建

```sh
# 单独运行外部公共接口测试，不访问真实显示设备。
cargo test --locked --offline -p slint-bridge-test-app --features fbdev

# 库内回归和独立工程一起运行。
cargo test --locked --offline --workspace --features nx5-tde
python3 test-app/tools/check-feature-matrix.py

# 单独检查 WBC 错误退出与线程清理，使用主机替身，不加载 SDK。
cargo test --locked --offline -p slint-bridge-test-app --features nx5 \
  --bin display nx5_observer::tests

# 独立工程的 ARM 软件构建。
cargo build --locked --offline --release -p slint-bridge-test-app \
  --features nx5 --target arm-unknown-linux-gnueabi --bin basic
```

NX5 部署与测试后的归档清理统一使用 [deploy.py](tools/nx5/README.md#部署归档清理和截图)，保留当前程序、一份恢复版本及其依赖；截图子命令会即时检查设备可用内存。

NX5 TDE、观察器插件、部署与 MCP 运行命令见 [devices/nx5/README.md](devices/nx5/README.md)。所有构建仍使用仓库指定的同一份 Slint 1.18.1 源码。

[此前启动实机回归](../docs/nx5-startup.md) 验证了 `949ceb9` 的五个启动用例各两轮、10 帧全幅实际像素和完整进程清理，另有 WBC 与纹理运行。三项 ignored TDE/翻页测试已在 [2026-10-12 补测](../docs/nx5-hardware.md) 中全部通过；EIO 的历史结果仍见 [前一轮报告](../docs/nx5-tde-exit.md)。

`c03c3bb` 的自定义设备路径、两页实际像素、用户真实点触与 WBC 动态库采样见 [此前路径实机验证](../docs/nx5-configured-paths.md)。`bccf6b9` 的同进程初始化重试、描述符缓存、WBC 与纹理退出回归见 [审查修复报告](../docs/nx5-review-fixes.md)；随后 [清理历史临时产物并完成十次截图复验](../docs/nx5-screenshot-memory.md)，截图 OOM 原始记录保留。当前 `2de9168` 已完成 [常驻切换和实体触摸验证](../docs/nx5-touch-response.md)，`bccf6b9` 程序及启动脚本作为回退保留。

## 增加设备或用例

新增设备先在库的 `src/backends/` 实现并验证板级提交契约，然后在本工程的 Cargo features 中转发该板级选择。在 `devices/<设备名>/` 记录路径、工具链、运行前提与实测结果；只有设备专有的诊断才进入 `tools/<设备名>/`。

新增 UI 场景放在 `ui/`，由 `ui/app.slint` 导出，应用状态和事件处理写在 `src/`。通用接口回归放在 `tests/`；不要为了外部测试公开 framebuffer 私有页状态或允许调用者伪造完成通知。

库内 `src/tests/` 及 `src/backends/nx5/tde/tests.rs` 保留需要私有实现的单元/故障注入测试。TDE 像素比较只共享本工程的声明式场景和纹理素材，不再把应用的 `main.rs` 编译进库测试。

MCP 不能替代 evdev 解码、物理页所有权、TDE/DMA 失败及实际扫描输出验证；这些底层测试和 NX5 工具继续保留。历史报告入口在 [docs/README.md](../docs/README.md)，原始证据作为本地 `evidence/` 档案保留，不纳入 Git；证据重放需要另行取得对应日志。
