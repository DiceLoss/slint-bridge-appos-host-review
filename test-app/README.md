# Slint Bridge 测试工程

这是独立的 Cargo package `slint-bridge-test-app`，通过 `path = ".."` 依赖适配库，按真实应用的方式调用 `slint_bridge::init(Config)`。它与库处于同一 workspace，共用仓库根的 `Cargo.lock` 和 `target/`。

界面、应用逻辑、测试素材及设备工具分别存放：

```text
test-app/
├── Cargo.toml                 # 转发板级 feature，依赖公开库接口
├── build.rs                   # 编译应用和设备回归界面
├── src/
│   ├── lib.rs                 # 生成的界面类型、设备路径配置
│   ├── main.rs                # display：运动、纹理、暂停/恢复
│   ├── bin/showcase.rs        # showcase：六页触屏展示与交互测试
│   ├── showcase.rs            # 模拟数据、跨页状态、回调与动画时钟
│   ├── bin/basic.rs           # basic：最小接入示例
│   ├── bin/init-retry.rs      # NX5 同进程初始化失败与资源清理验证
│   ├── bin/startup-regression.rs # NX5 启动/重试/退出实机验证
│   ├── bin/alpha-regression.rs # NX5 透明 UI、实际页及 WBC 比较
│   └── nx5_observer.rs        # NX5 专用进程内 WBC 观察器
├── ui/
│   ├── app.slint              # 导出测试界面
│   ├── basic.slint            # 不依赖设备尺寸的基础窗口
│   ├── display.slint          # 随窗口尺寸布局的运动/纹理场景
│   ├── showcase.slint         # 展示应用外壳、导航及模态对话框
│   ├── showcase/              # 六个页面、共享控件及模型类型
│   ├── assets/                # 编译时嵌入的图片背景及生成来源
│   ├── startup.slint          # 可拒绝布局与纯色像素验证
│   └── alpha.slint            # 半透明色块与黑底参考色
├── fixtures/textures.rs       # 确定性的纹理素材；底层 TDE 比较也复用它
├── tests/public_api.rs        # 仅使用适配库公开接口的回归
├── tests/showcase.rs          # 展示应用状态、筛选、表单及透明像素回归
├── devices/nx5/               # 当前设备记录与构建/运行说明
└── tools/
    ├── check-feature-matrix.py
    ├── check-showcase.py      # 主机原生 Slint MCP 交互与截图检查
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
| 完整界面展示 | `--bin showcase -- --touch=/dev/input/event0` | 六页导航、表单、列表、动画、透明区域、交互记录 |
| 运动与触摸 | `--bin display -- --touch=/dev/input/event0` | 连续更新；点击暂停、再次点击恢复 |
| 纹理与透明混合 | `--bin display -- --textures` | 大图、透明图和运动组合；是否实际使用硬件加速由板级统计确认 |
| MCP 交互 | `display` 加 MCP 构建和运行环境 | 元素树、截图、点击/拖动、输入事件录制 |
| NX5 合成输出 | `display --pattern --observe=300 --seconds=20` | WBC 黑白样本；需先部署 NX5 观察器插件 |
| NX5 混色负对照 | 上一项加 `--split`，采样改为 `--observe=30` | 应报告混色，不能当作通过 |
| NX5 透明输出 | `--bin alpha-regression -- OUTPUT_DIRECTORY`，需 `nx5` 或 `nx5-tde` | 五阶段实际前台页像素及进程内 WBC 的半透明/参考色比较，需部署专用插件 |
| NX5 初始化清理 | `--bin init-retry`，需 `nx5` 或 `nx5-tde` | 同进程中 framebuffer/输入初始化失败、重试与 UI 线程退出后的 VO/TDE 清理 |
| NX5 窗口启动 | `--bin startup-regression`，仅 `nx5-tde` | 首次显示失败清理、同一组件重试、延迟创建和真实前台页像素 |

`basic` 只使用 `--framebuffer` 和 `--touch` 路径配置。`display` 另外支持 `--textures`、`--pattern`、`--split`、`--observe=N` 和 `--seconds=N`；这些选项使用表中所示的 `--name=value` 或独立开关格式。旋转、按键映射和帧间隔通过 Rust `Config` 配置，当前没有对应命令行参数。

## 六页展示应用 showcase

`showcase` 是独立的触屏应用，使用图片背景、半透明浅色卡片、深色导航和蓝绿色强调色；原来的 `display` 继续用于固定运动/纹理和 WBC 诊断。新界面面向 NX5 的 1024×600 屏幕，最低布局尺寸为 960×600，大屏随窗口扩展。使用英文标签以兼容现有 DejaVu Sans 字体，未引入中文字体依赖。

| 页面 | 可操作内容 | 检查重点 |
| --- | --- | --- |
| Overview | 实时模拟柱图、快捷入口、会话计数 | 布局、渐变、模型更新、导航 |
| Controls | 名称输入、触屏字母/数字键盘、滑块、开关、复选框、下拉框、应用与重置确认 | 双向绑定、输入校验、取消不改值、跨页状态保留 |
| Devices | 18 条模拟设备、名称/房间搜索、触屏搜索键盘、在线筛选、空状态、滚动和详情弹窗 | 列表模型、触摸滚动、筛选后按稳定 ID 选择 |
| Motion | 六个运动图形、暂停/恢复、速度滑块、原生缓动状态切换 | 位置、透明度、裁剪、颜色和圆角动画 |
| Layers | 全屏图片背景、每像素 alpha 纹理、透明度滑块、透明窗口开口 | 图像缩放、混合、关闭底图后的真实透明像素 |
| Interaction lab | 三个点击目标、拖动面板、六项交互练习、最近 24 条操作记录 | 点击/拖动区别、页面回调、清空记录 |

设备数据和图表均为本地模拟，配置仅在本次进程中保存，不改变设备硬件设置。`UI ticks` 是应用动画更新次数，不是 framebuffer 完成数或显示 FPS；静态页面不更新动画数据。练习完成表示对应 UI 操作已发生，不表示硬件测试通过。打开透明区域时不绘制全屏底色，没有连接视频源；实机下层可能仍是黑色，视频合成仍须独立验收。

应用支持 `--framebuffer=PATH`、`--touch=PATH` 和 `--seconds=N`。NX5 构建命令与设备说明相同，将 `--bin display` 换为 `--bin showcase`；运行时不传 `--textures` 或 WBC 参数。必须先按设备流程确认显示所有权、退出已有程序并保留回退，再启动新二进制。构建和主机预览不会切换设备当前版本。

### 主机预览与自动交互检查

主机预览使用同源 Slint 的原生 headless software backend，不打开 framebuffer、不加载厂商 SDK，也不绕过库的设备准入。仅显式开启测试工程的 `showcase-preview` feature 并传 `--preview` 时使用此路径；设备构建不需要该 feature。

```sh
SLINT_EMIT_DEBUG_INFO=1 cargo build --locked --offline \
  -p slint-bridge-test-app --features fbdev,showcase-preview --bin showcase

# 自动启动独立主机预览，使用随机 localhost 端口；输出目录必须尚不存在。
python3 test-app/tools/check-showcase.py --output target/showcase-check

# 或保留预览供 MCP 客户端检查。headless 不创建桌面可见窗口。
SLINT_MCP_PORT=19316 target/debug/showcase --preview --seconds=300
```

原生 MCP 地址为 `http://127.0.0.1:19316/mcp`，连接方式见 [MCP 指南](../docs/slint-mcp.md)。自动检查脚本通过原生 MCP 实际操作六页，保存六页及四种附加状态的 PNG、元素树、事件记录和检查摘要；它只启动自己的主机进程，不接受设备 endpoint。截图应进一步人工检查排版，脚本通过不等于像素视觉验收。脚本终止自己创建的主机进程，不作为 NX5 SDK 正常退出验证。

导航 ID 为 `Showcase::nav-overview`、`nav-controls`、`nav-devices`、`nav-motion`、`nav-media`、`nav-lab`（各 ID 均带 `Showcase::` 前缀）；页面控件例如 `Controls::level-slider`、`Devices::search-input`、`Motion::motion-toggle`、`Media::alpha-slider`、`Lab::drag-pad`。弹窗关闭/确认 ID 为 `Showcase::dialog-cancel` / `Showcase::dialog-confirm`。重建页面后重新查询元素句柄。

`tests/showcase.rs` 使用 Slint 内存软件窗口检查表单校验、状态保留、重置取消、键盘编辑、列表筛选、稳定 ID、日志上限及透明开口的实际 RGBA 像素。`tests/showcase_repaint.rs` 另外通过确定性的文字模型更新和独立参考 renderer，验证缓存淘汰后切页标题仍刷新；不会用截图清空被测 renderer 的缓存。默认 feature 矩阵包含这些测试；MCP 交互检查单独运行。两者均不代替 evdev、TDE/DMA、物理双页和扫描输出验证。

界面固定使用 Fluent 标准控件，图标以 SVG 数据嵌入二进制；测试工程启用同源 `i-slint-core` 的图像/SVG 解码能力，不引入桌面或 GPU 后端。图片背景使用编译时嵌入的 [coastal-background.png](ui/assets/coastal-background.png)，保留的源图为 1638×960 PNG；构建脚本在主机将它转换为 1024×600 RGBA PNG 后嵌入，默认面板不再反复缩放背景，且满足 TDE 复制的尺寸/格式条件。其他窗口尺寸仍按 `cover` 适配；生成来源与提示词见 [素材说明](ui/assets/README.md)。动画区复用同一图片，卡片与列表行采用半透明底色，保证动画/滚动时图片实际参与绘制。径向 alpha 叠加图仍由 Rust 生成；Layers 透明开口会隐藏背景图片。无需另行部署素材文件，字体仍按设备说明提供。

## 专用诊断说明

WBC 按无版本库名 `libobserver.so` 通过系统搜索路径加载，部署目录可由 `LD_LIBRARY_PATH` 指定；新插件接收应用配置的 framebuffer 路径，并通过 ioctl 获取采样尺寸，旧插件需重新构建。

`--split` 与 `--pattern` 配合使用；`--observe=N` 仅在 NX5 构建中启用，在启动约 2 秒后创建观察线程。`--seconds=N` 请求事件循环退出；进程还会等待在途提交和已启动的观察线程结束。正常退出显式等待 WBC，事件循环返回错误时由观察器资源所有者自动等待，保证采集线程结束后才返回应用错误。若观察线程也 panic，清理时记录该错误并保留原 UI 错误；正常退出时观察线程 panic 仍会作为错误返回。

上述清理发生在事件循环正常退出或返回错误的路径。应用没有将 SIGINT/SIGTERM 转换为 `slint::quit_event_loop()` 的信号处理器，不能把信号终止当作同样的 Rust 析构验证；有限时长验证使用 `--seconds=N`。常驻程序切换时的暂停、提交空闲检查和回退流程见 [部署实测](../docs/nx5-touch-response.md#回退)。

观察器的采样结果写入日志，当前应用不会将非零采样结果转换成非零退出码，验收需使用 [check-run.py](tools/nx5/display/check-run.py) 检查日志。

`startup-regression` 是独占 NX5 的专用验证程序，不加入日常显示流程。参数为 `show-error|retry|queued|timer|queued-retry` 加输出目录，可追加 `--framebuffer=PATH`、`--touch=PATH`；`show-error` 保留预期布局错误并返回 1，其余用例应返回 0。它通过公开平台接口运行真实组件，按设备 ioctl 获取实际宽高、stride 和前台页，逐行检查实际像素并保存 TDE 状态；完整退出由外层脚本检查。构建、采集与结果见 [启动实机回归](../docs/nx5-startup.md)。

`init-retry OUTPUT_DIRECTORY [--framebuffer=PATH] [--touch=PATH]` 需要已有的输出目录并独占设备。它连续执行两次 framebuffer 路径错误和两次输入路径错误，然后成功启动并结束一个 UI 线程；在进程仍存活时断言 VO 已关闭、无 TDE MMZ 分配，核对重复失败及 UI 退出后的设备描述符缓存不增长，并保存 `.vo`、`.mmz`、`.fds` 快照。按设备说明构建时将 `--bin display` 换为 `--bin init-retry`；正常应返回 0，不能与显示进程同时运行。

应用要透出下层视频时，可将 `.slint` 根窗口设为 `background: transparent;`，在其上绘制局部 UI；`basic` / `display` 保持原有背景，`showcase` 的 Layers 页可以打开透明区域。透明和半透明像素的输出、无 alpha 格式限制及尚未接入的视频层边界见 [Framebuffer 软件路径](../docs/software-platform.md#framebuffer-软件路径)。库内透明窗口回归使用内存双页验证移动和消失后的 alpha、颜色及行尾 padding。`alpha-regression` 另用于设备上的实际页和 WBC 检查，构建、前提及输出见 [设备工具](tools/nx5/README.md#硬件诊断与证据重放)；它使用现有黑色 VO 底色，不代表视频解码和业务视频接入已经验证。

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

`c03c3bb` 的自定义设备路径、两页实际像素、用户真实点触与 WBC 动态库采样见 [此前路径实机验证](../docs/nx5-configured-paths.md)。`bccf6b9` 的同进程初始化重试、描述符缓存、WBC 与纹理退出回归见 [审查修复报告](../docs/nx5-review-fixes.md)；随后 [清理历史临时产物并完成十次截图复验](../docs/nx5-screenshot-memory.md)，截图 OOM 原始记录保留。此前 `2de9168` 的 [常驻切换和实体触摸验证](../docs/nx5-touch-response.md) 保留原记录；`f963558` 的 [透明 UI 量程修复、WBC 与帧率实测](../docs/nx5-alpha.md) 保留原有范围。当前图片背景结果见 [图片背景复测](../docs/nx5-image-background.md)，首次六页用例结果见 [showcase 部署报告](../docs/nx5-showcase.md)，实际运行版本及回退以 [设备说明](devices/nx5/README.md) 为准。

## 增加设备或用例

新增设备先在库的 `src/backends/` 实现并验证板级提交契约，然后在本工程的 Cargo features 中转发该板级选择。在 `devices/<设备名>/` 记录路径、工具链、运行前提与实测结果；只有设备专有的诊断才进入 `tools/<设备名>/`。

新增 UI 场景放在 `ui/`，由 `ui/app.slint` 导出，应用状态和事件处理写在 `src/`。通用接口回归放在 `tests/`；不要为了外部测试公开 framebuffer 私有页状态或允许调用者伪造完成通知。

库内 `src/tests/` 及 `src/backends/nx5/tde/tests.rs` 保留需要私有实现的单元/故障注入测试。TDE 像素比较只共享本工程的声明式场景和纹理素材，不再把应用的 `main.rs` 编译进库测试。

MCP 不能替代 evdev 解码、物理页所有权、TDE/DMA 失败及实际扫描输出验证；这些底层测试和 NX5 工具继续保留。历史报告入口在 [docs/README.md](../docs/README.md)，原始证据作为本地 `evidence/` 档案保留，不纳入 Git；证据重放需要另行取得对应日志。
