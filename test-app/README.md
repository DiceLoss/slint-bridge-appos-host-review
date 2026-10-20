# Slint Bridge 测试工程

这是独立的 Cargo package `slint-bridge-test-app`，通过 `path = ".."` 依赖适配库，按真实应用的方式调用 `slint_bridge::init(Config)`。它与库处于同一 workspace，共用仓库根的 `Cargo.lock` 和 `target/`。这里使用 path 是为了测试本仓库尚未发布的改动；外部业务应用使用 [Git 接入配置](../docs/getting-started.md#在自己的-rust-应用中接入)，不需要本测试工程或本地适配层目录。

界面、应用逻辑、测试素材及设备工具分别存放：

```text
test-app/
├── Cargo.toml                 # 转发板级 feature，依赖公开库接口
├── build.rs                   # 编译应用和设备回归界面
├── src/
│   ├── lib.rs                 # 生成的界面类型、设备路径配置
│   ├── main.rs                # display：运动、纹理、暂停/恢复
│   ├── bin/showcase.rs        # showcase：九页触屏展示与交互测试
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
│   ├── showcase/              # 九个页面、共享控件及模型类型
│   ├── assets/                # 编译时嵌入的图片背景及生成来源
│   ├── startup.slint          # 可拒绝布局与纯色像素验证
│   └── alpha.slint            # 半透明色块与黑底参考色
├── fixtures/textures.rs       # 确定性的纹理素材；底层 TDE 比较也复用它
├── tests/public_api.rs        # 仅使用适配库公开接口的回归
├── tests/showcase.rs          # 展示应用状态、筛选、表单及透明像素回归
├── tests/showcase_scaling.rs  # PNG/SVG 运行时缩放及保留缓冲像素回归
├── tests/showcase_repaint.rs  # 文字缓存淘汰后的保留缓冲标题刷新回归
├── tests/showcase_rich_repaint.rs # 富列表双向裁剪、选择及屏外修改的保留缓冲回归
├── tests/showcase_rich_memory.rs # 有界条目实例、首末端访问及素材生命周期
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

F1 集成构建选择 `--features f1` 或 `--features f1-g2d`，二者均通过
`slint-bridge` 公开接口使用库；`f1-g2d` 包含 `f1`。
F1 使用已检查固件上的参考 PAN/VSYNC 方式，需先释放原显示所有者；
构建、SD 卡部署和实测范围见 [F1 设备说明](devices/f1/README.md)。

F1 showcase 在 `init()` 前通过 `slint_bridge::try_decode_png` 尝试加载大背景，以降低与显示画布重叠的内存峰值；`Ok(None)` 才使用 Slint 软件加载。缩放页和富列表的小 PNG 保留软件解码，因为完整加载实测更快；SVG 保留 Slint 路径。`--preview` 始终使用软件图片加载。未知硬件完成错误会打印 `F1_PNG_QUARANTINED` 并保留进程，不自动退出或重新提交。

`--bin image-decode` 使用同一个公开接口进行 CPU/VE 校验和计时，需 `f1` 或 `f1-g2d`，运行前提与命令见 [F1 图片解码验证](tools/f1/README.md#png-解码验证)。

| 项目 | 选择方式 |
| --- | --- |
| 通用 framebuffer 软件组合 | `--features fbdev` |
| NX5 软件渲染 | `--features nx5` |
| NX5 TDE 和软件回退 | `--features nx5-tde` |
| NX5 MMA 背景能力 | `--features nx5-mma-background`，运行时 `--background=mma` |
| 背景存储 | `--background=heap`（默认）或 `--background=mma`，映射 `Config.background_storage` |
| framebuffer 路径 | `--framebuffer=/dev/fb0`，默认 `/dev/fb0`，可替换为目标设备路径或设备链接 |
| 触摸设备 | `--touch=/dev/input/event0`，默认不开启输入设备 |
| Slint MCP | 构建加 `slint/mcp`，并按 [MCP 指南](../docs/slint-mcp.md) 设置环境变量 |

本库和测试工程的 `default = []`，必须在构建命令中选择一个板级 feature；`fbdev` 和 `nx5` 互斥。业务模板中的默认 NX5 TDE 仅属于该业务应用，不会改变本测试工程。NX5 使用板级阻塞翻页契约，不比对固定内核版本、驱动名称或文件摘要；`fbdev` 在真实设备运行时仍会拒绝未验证的提交契约。测试工程不绕过这个检查。

## 用例与预期

| 用例 | 运行选择 | 验证内容 |
| --- | --- | --- |
| 最小显示 | `--bin basic` | 从公开接口初始化平台并创建窗口 |
| 完整界面展示 | `--bin showcase -- --touch=/dev/input/event0` | 九页导航、表单、列表、动画、透明区域、交互记录、运行时图片缩放 |
| 运动与触摸 | `--bin display -- --touch=/dev/input/event0` | 连续更新；点击暂停、再次点击恢复 |
| 纹理与透明混合 | `--bin display -- --textures` | 大图、透明图和运动组合；是否实际使用硬件加速由板级统计确认 |
| MCP 交互 | `display` 加 MCP 构建和运行环境 | 元素树、截图、点击/拖动、输入事件录制 |
| NX5 合成输出 | `display --pattern --observe=300 --seconds=20` | WBC 黑白样本；需先部署 NX5 观察器插件 |
| NX5 混色负对照 | 上一项加 `--split`，采样改为 `--observe=30` | 应报告混色，不能当作通过 |
| NX5 透明输出 | `--bin alpha-regression -- OUTPUT_DIRECTORY`，需 `nx5` 或 `nx5-tde` | 五阶段实际前台页像素及进程内 WBC 的半透明/参考色比较，需部署专用插件 |
| NX5 初始化清理 | `--bin init-retry`，需 `nx5` 或 `nx5-tde` | 同进程中 framebuffer/输入初始化失败、重试与 UI 线程退出后的 VO/TDE 清理 |
| NX5 窗口启动 | `--bin startup-regression`，仅 `nx5-tde` | 首次显示失败清理、同一组件重试、延迟创建和真实前台页像素 |

`basic` 支持 `--framebuffer`、`--touch` 路径及 `--rotation=0|90|180|270` 顺时针旋转配置。`display` 另外支持 `--textures`、`--pattern`、`--split`、`--observe=N` 和 `--seconds=N`；这些选项使用表中所示的 `--name=value` 或独立开关格式。共享参数解析支持 `--rotation=0|90|180|270`，默认 0；非法值会在设备初始化前报错退出。按键映射和帧间隔通过 Rust `Config` 配置。X9 独立 CPU 整帧参考导出见 [工具说明](tools/x9/README.md#独立-cpu-参考)。

<a id="六页展示应用-showcase"></a>

## 展示应用 showcase

`showcase` 是独立的触屏应用，使用图片背景、半透明浅色卡片、深色导航和蓝绿色强调色；原来的 `display` 继续用于固定运动/纹理和 WBC 诊断。新界面面向 NX5 的 1024×600 屏幕，最低布局尺寸为 960×600，大屏随窗口扩展。使用英文标签以兼容现有 DejaVu Sans 字体，未引入中文字体依赖。

| 页面 | 可操作内容 | 检查重点 |
| --- | --- | --- |
| Overview | 实时模拟柱图、快捷入口、会话计数 | 布局、渐变、模型更新、导航 |
| Controls | 名称输入、触屏字母/数字键盘、滑块、开关、复选框、下拉框、应用与重置确认 | 双向绑定、百分比圆环、输入校验、取消不改值、跨页状态保留 |
| Devices | 18 条模拟设备、搜索/筛选、纵向列表、横向多屏卡片和详情弹窗 | 双方向滚动、边界、拖动不误触、筛选后按稳定 ID 选择 |
| Motion | 六个运动图形、暂停/恢复、速度滑块、原生缓动状态切换 | 位置、透明度、裁剪、颜色和圆角动画 |
| Layers | 全屏图片背景、每像素 alpha 纹理、透明度滑块、透明窗口开口 | 图像缩放、混合、关闭底图后的真实透明像素 |
| Interaction lab | 三个点击目标、拖动面板、六项交互练习、最近 24 条操作记录 | 点击/拖动区别、页面回调、清空记录 |
| Image scaling | SVG/PNG 并排预览、50%～200% 缩放滑块、100% 复位 | 运行时尺寸与像素变化、非整数比例、透明边缘、放大裁剪、缩小后脏区清理 |
| Rich list | 100 张含渐变阴影 PNG、运行时 SVG 图标及多行文本的卡片，横纵切换及回到起点 | 大范围持续滑动、边界夹紧、拖动误触、按稳定 ID 选中首末项 |
| Clock | 系统本地时间、模拟指针表盘 | 数字布局、秒针更新、时分针角度与页面重入同步 |

Clock 按白色圆盘、黑色数字和时分针、红色秒针绘制，配浅蓝背景及阴影；表盘随可用区域等比例缩放。使用 Slint 文字与 Path，无额外表盘位图。显示设备系统的本地时间（含系统时区），秒针每秒跳动，时分针含秒数偏移；进入页面立即同步，离开页面停止更新时间属性。Motion 的暂停不影响时钟；没有联网校时或设置设备时间的行为，设备 RTC/系统时间不准时显示也会不准。 NX5 的部署、指针及物理页验证见[钟表页实测](../docs/nx5-validation.md#nx5-clock)。

Rich list 首次进入时才加载两套 PNG 底图和五个原始 SVG，并建立 100 项模型。1024×600 面板上可视区为 800×340；纵向总长 11992 px、可滚动 11652 px（约 34.3 屏），横向总长 25588 px、可滚动 24788 px（约 31.0 屏）。两套底图交替使用；每张卡片都含 32×32 SVG 主图标、16×16 SVG 箭头和文本，透明边缘与阴影参与合成。每张卡片限定自身裁剪边界，使 renderer 跳过屏外子树，保留原图像、阴影、文字和命中区域。当前固定 100 个卡片实例，没有列表虚拟化；这是本例的负载范围，不代表无限列表的内存特性。方向切换回到起点，选择状态在离开页面后仍保留；侧栏导航高度为 40 px，以容纳九页。

Controls 预览圆环从顶部顺时针显示配置百分比：0% 只显示底轨，100% 显示完整着色环。关闭输出时仍保留配置进度，并改用灰色；下方线性 Meter 显示关闭后的零输出。修复与设备验证见 [圆环报告](../docs/nx5-validation.md#nx5-controls-ring)。

Devices 默认显示纵向列表，点击 `Horizontal cards` 切为横向卡片，左右连续滑动可查看全部 18 项；点击 `Vertical list` 切回。两种模式共用搜索、在线筛选和详情回调，切换后从对应列表的起点开始。重新进入 Devices 页默认恢复纵向模式。

设备数据和图表均为本地模拟，配置仅在本次进程中保存，不改变设备硬件设置。`UI ticks` 是应用动画更新次数，不是 framebuffer 完成数或显示 FPS；静态页面不更新动画数据。练习完成表示对应 UI 操作已发生，不表示硬件测试通过。打开透明区域时不绘制全屏底色，没有连接视频源；实机下层可能仍是黑色，视频合成仍须独立验收。

应用支持 `--framebuffer=PATH`、`--touch=PATH`、`--rotation=0|90|180|270` 和 `--seconds=N`。X9 使用 `fbdev`、800×1280 物理面板及 `--rotation=90`，逻辑界面为 1280×800；构建与启动见 [X9 设备说明](devices/x9/README.md)。NX5 构建命令与设备说明相同，将 `--bin display` 换为 `--bin showcase`；运行时不传 `--textures` 或 WBC 参数。必须先按设备流程确认显示所有权、退出已有程序并保留回退，再启动新二进制。构建和主机预览不会切换设备当前版本。

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

原生 MCP 地址为 `http://127.0.0.1:19316/mcp`，连接方式见 [MCP 指南](../docs/slint-mcp.md)。自动检查脚本通过原生 MCP 实际操作九页，保存 20 张 PNG（含横向起点/末端/返回，图片 50%、125%、200%、100% 缩放，以及富内容列表横纵两种布局）、元素树、事件记录和检查摘要；它只启动自己的主机进程，不接受设备 endpoint。截图应进一步人工检查排版，脚本通过不等于像素视觉验收。脚本终止自己创建的主机进程，不作为 NX5 SDK 正常退出验证。

导航 ID 为 `Showcase::nav-overview`、`nav-controls`、`nav-devices`、`nav-motion`、`nav-media`、`nav-lab`、`nav-scaling`、`nav-rich-list`、`nav-clock`（各 ID 均带 `Showcase::` 前缀）；页面控件例如 `Controls::level-slider`、`Devices::search-input`、`Motion::motion-toggle`、`Media::alpha-slider`、`Lab::drag-pad`、`Scaling::zoom-slider` 和 `Scaling::zoom-reset`。弹窗关闭/确认 ID 为 `Showcase::dialog-cancel` / `Showcase::dialog-confirm`。横向模式入口为 `Devices::orientation-toggle`，滚动区为 `Devices::horizontal-list`，卡片重复使用 `Devices::device-card`；原生 MCP 只枚举当前可见卡片。重建页面或切换模式后重新查询元素句柄。

自动横向用例从首项连续滑到末项再返回，在 1024×600 下验证累计移动超过四个可视宽度、单次手势至少跨过 60% 可视宽度、两端夹紧、无纵向漂移及拖动不打开详情，并检查末项/筛选后的稳定 ID 和空状态。横向每次手势单独录制，避免长距离遍历超过原生 MCP 的 1024 条事件环形缓冲。

单独完整遍历富内容列表（两种方向均覆盖 100 项，包含真实拖动、滚轮全程、边界和首末项选择，保存六张截图）：

```sh
python3 test-app/tools/check-showcase.py --binary target/debug/showcase \
  --rich-only --output target/showcase-rich-check
```

稳定 ID 为 `RichList::listing`、`rich-item`、`orientation-toggle`、`start-button`（均带 `RichList::` 前缀）。

富列表使用固定尺寸条目，保留每条数据的轻量位置节点，但在 Flickable 中仅为 `ceil(视口长度 / 条目步长) + 2` 项创建完整卡片内容（不超过数据条数）。
纵向步长为 120px、横向为 256px；保留少量预备项，已在可见范围内的卡片保持原绑定和文字缓存，100 条业务数据与稳定选择 ID 不变。
屏外卡片会销毁、重新出现时重建，因此滚动检查必须重新读取当前数据 ID，不能把首项的旧句柄当作永久位置标记；`Client.rich_offset()` 同时检查当前实例的位置和轴向一致性。
背景 PNG 只加载当前方向的两张，纵向像素合计 700 KiB、横向 625.25 KiB；离开富列表会清空背景和图标模型，保留小型业务数据与选择状态。
重新进入从纵向起点开始并重新加载素材；共享渲染缓存和分配器可能延迟实际内存归还，不能把释放模型引用直接等同于 RSS 等量下降。
这是测试应用的 UI/资源管理策略，不改变适配库的事件、GE 或双 framebuffer 契约。

`tests/showcase_rich_memory.rs` 通过公开 Model 记录实际请求的行，检查初次绘制和首末端跳转没有读取全部 100 行，
并覆盖横纵素材切换、离开后释放、重新进入时业务数据/选择保留，以及空模型和单项模型。

`tests/showcase.rs` 使用 Slint 内存软件窗口检查表单校验、状态保留、重置取消、键盘编辑、列表筛选、稳定 ID、日志上限及透明开口的实际 RGBA 像素。`tests/showcase_scaling.rs` 在同一个保留缓冲 renderer 中依次切换 50%→100%→125%→200%→125%→50%→100%，检查原始素材尺寸固定、PNG/SVG 实际色块随缩放变化，并与独立全量重绘逐像素对照，覆盖缩小后的残留清除。`tests/showcase_repaint.rs` 另外通过确定性的文字模型更新和独立参考 renderer，验证缓存淘汰后切页标题仍刷新；不会用截图清空被测 renderer 的缓存。`tests/showcase_rich_repaint.rs` 比较 25 个横纵滚动、奇数/小数偏移、首末端、选中、屏外模型修改及重新进入状态的保留缓冲与独立全量重绘，并验证滚动确实改变画面、屏外修改不改变当前画面。默认 feature 矩阵包含这些测试；MCP 交互检查单独运行。这些检查均不代替 evdev、TDE/DMA、物理双页和扫描输出验证。

`tests/texture_blend.rs` 对预乘 RGBA 纹理建立独立整数混合参考：四种旋转、裁剪偏移、透明/半透明/不透明背景，以及保留缓冲重绘逐像素一致；批量采样覆盖全部 256×256 字节乘积、透明和不透明目标、32 像素批次边界与未对齐尾部。X9 release 测试程序还在实机执行，覆盖实际 NEON 代码；性能和部署验证见 [X9 纹理 NEON 报告](../docs/slint-optimization.md#x9-texture-neon)。

NX5 的私有 `Pixel` 包装另由库内 `sampled_pixel_blending_matches_scalar_with_guards` 检查批量委托、采样顺序和切片保护区；实现及验证范围见 [NX5 Pixel 批量纹理混合](../docs/slint-optimization.md#nx5-pixel-batch)。

界面固定使用 Fluent 标准控件，图标以 SVG 数据嵌入二进制；测试工程启用同源 `i-slint-core` 的图像/SVG 解码能力，不引入桌面或 GPU 后端。背景由 `showcase::configure` 在应用未指定 `wallpaper` 时加载；`.slint` 属性不提前解码图片，因此应用可以在配置前选择背景存储。图片背景使用编译时嵌入的 [coastal-background.png](ui/assets/coastal-background.png)，保留的源图为 1638×960 PNG；构建脚本在主机将它转换为 1024×600 RGBA PNG 后嵌入，默认面板不再反复缩放背景，且满足 TDE 复制的尺寸/格式条件。独立 Image scaling 页另用两份未经构建期变换的 256×160 原始 PNG/SVG，运行时加载后由同一尺寸滑块缩放；它与背景优化分开。其他窗口尺寸仍按 `cover` 适配；生成来源与提示词见 [素材说明](ui/assets/README.md)。动画区复用同一图片，卡片与列表行采用半透明底色，保证动画/滚动时图片实际参与绘制。径向 alpha 叠加图仍由 Rust 生成；Layers 透明开口会隐藏背景图片。无需另行部署素材文件，字体仍按设备说明提供。

## 专用诊断说明

WBC 按无版本库名 `libobserver.so` 通过系统搜索路径加载，部署目录可由 `LD_LIBRARY_PATH` 指定；新插件接收应用配置的 framebuffer 路径，并通过 ioctl 获取采样尺寸，旧插件需重新构建。

`--split` 与 `--pattern` 配合使用；`--observe=N` 仅在 NX5 构建中启用，在启动约 2 秒后创建观察线程。`--seconds=N` 请求事件循环退出；进程还会等待在途提交和已启动的观察线程结束。正常退出显式等待 WBC，事件循环返回错误时由观察器资源所有者自动等待，保证采集线程结束后才返回应用错误。若观察线程也 panic，清理时记录该错误并保留原 UI 错误；正常退出时观察线程 panic 仍会作为错误返回。

上述清理发生在事件循环正常退出或返回错误的路径。应用没有将 SIGINT/SIGTERM 转换为 `slint::quit_event_loop()` 的信号处理器，不能把信号终止当作同样的 Rust 析构验证；有限时长验证使用 `--seconds=N`。常驻程序切换时的暂停、提交空闲检查和回退流程见 [部署实测](../docs/nx5-validation.md#nx5-touch-response)。

观察器的采样结果写入日志，当前应用不会将非零采样结果转换成非零退出码，验收需使用 [check-run.py](tools/nx5/display/check-run.py) 检查日志。

`startup-regression` 是独占 NX5 的专用验证程序，不加入日常显示流程。参数为 `show-error|retry|queued|timer|queued-retry` 加输出目录，可追加 `--framebuffer=PATH`、`--touch=PATH`；`show-error` 保留预期布局错误并返回 1，其余用例应返回 0。它通过公开平台接口运行真实组件，按设备 ioctl 获取实际宽高、stride 和前台页，逐行检查实际像素并保存 TDE 状态；完整退出由外层脚本检查。构建、采集与结果见 [启动实机回归](../docs/nx5-validation.md#nx5-startup)。

`init-retry OUTPUT_DIRECTORY [--framebuffer=PATH] [--touch=PATH]` 需要已有的输出目录并独占设备。它连续执行两次 framebuffer 路径错误和两次输入路径错误，然后成功启动并结束一个 UI 线程；在进程仍存活时断言 VO 已关闭、无 TDE MMZ 分配，核对重复失败及 UI 退出后的设备描述符缓存不增长，并保存 `.vo`、`.mmz`、`.fds` 快照。按设备说明构建时将 `--bin display` 换为 `--bin init-retry`；正常应返回 0，不能与显示进程同时运行。

应用要透出下层视频时，可将 `.slint` 根窗口设为 `background: transparent;`，在其上绘制局部 UI；`basic` / `display` 保持原有背景，`showcase` 的 Layers 页可以打开透明区域。透明和半透明像素的输出、无 alpha 格式限制及尚未接入的视频层边界见 [Framebuffer 软件路径](../docs/software-platform.md#framebuffer-软件路径)。库内透明窗口回归使用内存双页验证移动和消失后的 alpha、颜色及行尾 padding。`alpha-regression` 另用于设备上的实际页和 WBC 检查，构建、前提及输出见 [设备工具](tools/nx5/README.md#硬件诊断与证据重放)；它使用现有黑色 VO 底色，不代表视频解码和业务视频接入已经验证。

showcase 通过 `Config.background_storage` 和 `config.prepare_background(image)` 统一准备背景。默认 `--background=heap` 保持普通图像；NX5 启用 `nx5-mma-background` 后可选 `--background=mma`。旧 `--mma-background` 是同一配置的兼容别名；主机预览只支持普通模式。正式构建与专用故障注入构建分开，见 [MMA 背景说明](devices/nx5/mma-background/README.md)。

### 图片背景中的多个透明区域

独立 `background-holes` 用例使用 [BackgroundWithHoles](ui/components/background-with-holes.slint)，界面位于 [background-holes.slint](ui/background-holes.slint)，动作在 [background_demo.rs](src/background_demo.rs)。它固定为本轮 NX5 的 1024×600 画布，提供两个区域的移动、缩放、显示/隐藏、跨区域按钮及模态弹窗。背景复用构建期准备的海岸图片；区域内未被前景覆盖的像素透明，没有连接视频源。

复用组件时，在应用导出 `BackgroundGeometry`、`BackgroundHole`、`BackgroundRect`，创建窗口后、显示前调用 `background::install(app.global::<BackgroundGeometry>())`。根窗口保持透明，将组件放在前景控件之前：

```slint
BackgroundWithHoles { source: wallpaper; holes: video-regions; }
// 后续兄弟组件正常绘制按钮、字幕和弹窗。
```

`source` 使用提前准备好的图片，按 **1 个源像素对应 1 个逻辑像素** 绘制；区域模型每行包含整数 `x/y/width/height` 和 `visible`，坐标相对于组件。可替换整个模型，也可通过 `VecModel` 原地修改、插入或删除。负尺寸/零尺寸及隐藏行忽略，越界区域裁到图片边界，重叠区域取并集。几何回调只在源尺寸或模型变化时重新分割背景；所有片段共用同一张图片。前景放在组件之上，不参与背景扣除。

当前组件覆盖轴对齐矩形和原尺寸图片；不同画布请先准备对应尺寸的源图，不能直接把每个片段各自 `cover` 缩放。旋转、任意缩放、圆角区域以及与解码器的坐标/生命周期同步不属于本例。组件只使用 Slint 公共 API，没有修改 Slint 源码或 framebuffer 提交逻辑。

主机 `tests/background_holes.rs` 用独立的逐点区域并集判定，核对 22 个连续状态的 RGBA 输出，包含模型原地修改、重叠、越界、整数边界、隐藏恢复、图片替换、前景和弹窗；复用同一渲染缓冲，检查旧像素被修复。实机命令见 [NX5 工具](tools/nx5/README.md#图片背景透明区域的双页检查)，结果见 [实测报告](../docs/nx5-validation.md#nx5-background-holes)。

适配层 CPU 批量混合与旋转分块的像素差分及边界回归位于 `src/tests/texture_renderer.rs`；它使用真实 Slint 绘制入口比较参考输出，不要求扩大库的公开接口。测试工程的 `texture_blend` 继续保留应用级旋转和透明像素检查。

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

NX5 TDE、观察器插件、部署与 MCP 运行命令见 [devices/nx5/README.md](devices/nx5/README.md)。NX5 SDK 头文件/动态库和两设备字体已随 [third-party/](../third-party/README.md) 保存。所有构建均从仓库配置的同一个 Slint Git 来源解析依赖，不指定 `rev`；首次联网准备及更新锁文件的方法见 [平台说明](../docs/software-platform.md#构建依赖)。

启动、退出、路径、输入和物理页的历史验证统一见 [NX5 验证记录](../docs/nx5-validation.md)；显示合成见 [NX5 显示调查](../docs/nx5-display.md)，性能见 [NX5 性能记录](../docs/nx5-performance.md) 和 [Slint 两设备对照](../docs/slint-optimization.md)。当前操作只采用设备说明中的流程，历史 PID 和测试计数不作为当前运行状态。

## 增加设备或用例

新增设备先在库的 `src/backends/` 实现并验证板级提交契约，然后在本工程的 Cargo features 中转发该板级选择。在 `devices/<设备名>/` 记录路径、工具链、运行前提与实测结果；只有设备专有的诊断才进入 `tools/<设备名>/`。

新增 UI 场景放在 `ui/`，由 `ui/app.slint` 导出，应用状态和事件处理写在 `src/`。通用接口回归放在 `tests/`；不要为了外部测试公开 framebuffer 私有页状态或允许调用者伪造完成通知。

库内 `src/tests/` 及 `src/backends/nx5/tde/tests.rs` 保留需要私有实现的单元/故障注入测试。TDE 像素比较只共享本工程的声明式场景和纹理素材，不再把应用的 `main.rs` 编译进库测试。

MCP 不能替代 evdev 解码、物理页所有权、TDE/DMA 失败及实际扫描输出验证；这些底层测试和 NX5 工具继续保留。历史报告入口在 [docs/README.md](../docs/README.md)，原始证据作为本地 `evidence/` 档案保留，不纳入 Git；证据重放需要另行取得对应日志。

### NX5 填充与整体透明度对照

`nx5-tde` 的等尺寸 RGBA 图像现支持整体透明度，透明度变化会重新准备源缓存。已有 Media 展示中的缩放图片仍走软件回退；完整支持边界见 [平台说明](../docs/software-platform.md)，不能用滑块操作或 MCP 截图代替硬件命中检查。

需要比较纯色填充时，在上述设备启动命令前加 `SLINT_BRIDGE_TDE_FILL=1 SLINT_BRIDGE_TRACE=1`；TDE 统计行的 `fill` 显示硬件填充次数。正常运行不设置此变量，保留本板实测更快的 CPU 填充。初次接入见 [填充/混合报告](../docs/nx5-performance.md#nx5-fill-blend)，当前共享 MMZ 路径的填充复测见 [共享画布报告](../docs/nx5-performance.md#nx5-shared-canvas)。

X9 富列表的离屏阶段对照入口为 `examples/x9-render-bench.rs`，构建、参数和测量边界见 [X9 绘制阶段对照](tools/x9/README.md#绘制阶段对照)。它使用同一 Showcase 界面与软件 renderer，通过 0°/90° 对照定位旋转成本，不访问显示设备。
