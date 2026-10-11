# Rust 公共平台层

本文描述当前源码的接口与运行行为。NX5 #233 使用已接受的固件专用阻塞翻页双页实现；`fbdev` 尚无真实显示准入。固件身份不匹配返回 `DisplayNotAdmitted`，读取身份或摘要失败返回 `Io`。约 29 FPS、有限 WBC 样本等实机结果属于对应阶段的观测，不代表所有物理帧的无撕裂认证。

日常构建和运行见 [测试工程](../test-app/README.md) 与 [NX5 设备说明](../test-app/devices/nx5/README.md)。各轮硬件测量由 [文档索引](README.md) 导航；历史报告中的测试数量、PID 和性能数据不定义当前接口。

## 统一 API

入口为 `slint_bridge::init(Config)`，在 UI 线程创建 Slint 组件前调用。公开类型由 [src/lib.rs](../src/lib.rs) 导出：`Config`、`Error`、`Backend`、`Rotation`、`InputDevice`、`KeyBinding`；`selected_backend()` 返回编译期选择，不代表已经通过设备准入。

| `Config` 字段 | 默认值 | 当前约束 |
| --- | --- | --- |
| `framebuffer` | `/dev/fb0` | NX5 固件准入只接受此路径 |
| `inputs` | 空列表 | 至多一个 `Touchscreen` 和一个 `Keys` |
| `rotation` | `Rotation::None` | 固定为不旋转或顺时针 90、180、270 度；90/270 度交换逻辑宽高 |
| `frame_interval` | 16667 微秒 | 非零且可加到单调时钟；约束两次渲染启动的最小间隔 |

帧间隔不是 60 FPS 性能承诺。NX5 原样使用该配置，没有专用 33334 微秒钳制；显示页仍只在提交成功完成后复用。`init()` 在设备初始化前检查帧间隔和输入数量，随后执行板级准入、打开 framebuffer 与输入，最后安装 Slint 平台。

`init()` 返回本库的 `Error`。组件创建及 `run()` 返回 Slint 的 `PlatformError`；循环中的输入、绘制和提交错误会转换为其错误消息。退出前仍会等待在途提交；若 UI 已出错、该提交也失败，最终错误以 `PresentationFailed` 为主并保留原 UI 错误原因。提交成功时仍返回原 UI 错误；重复读取同一提交失败不会重复拼接原因。提交失败使该显示实例保持失败状态，不能通过重新运行循环恢复页所有权。

当前支持一个全屏根窗口、至多一个触摸设备及一个按键设备。根窗口应使用响应式布局；可用 `preferred-width`、`preferred-height` 表达偏好，不能固定成与旋转后的逻辑屏幕矛盾的大小。约束不匹配时返回错误。

`InputDevice::touchscreen(path)` 使用当前 NX5 触摸协议 `ABS_X/ABS_Y + BTN_TOUCH`。轴范围由 ioctl 查询，先归一化到物理屏幕，再逆变换为未旋转的 Slint 逻辑坐标。触摸按下、移动和释放只在 `SYN_REPORT` 后发送，避免把接触事件与尚未收齐的坐标组合。收到 `SYN_DROPPED` 后忽略事件直到报告边界，再用 `EVIOCG*` 查询状态，修复按键及接触状态；依据 [Linux evdev 规范](https://docs.kernel.org/input/event-codes.html#ev-syn)。

`InputDevice::keys(path)` 提供方向、确认、返回等导航键。产品可通过 `InputDevice::Keys { path, bindings }` 和 `KeyBinding::new(code, text)` 指定 Linux 键码到 Slint 特殊键或字符的映射。它处理按下、自动重复和释放，不提供键盘布局或输入法。输入掉线会终止事件循环并返回错误，避免持续轮询失效描述符。

## 事件循环

UI 线程通过 [event_wait.rs](../src/event_wait.rs) 使用 `ppoll` 等待 evdev 与 `eventfd`。业务线程通过标准 `slint::invoke_from_event_loop()` 投递任务，或调用 `slint::quit_event_loop()` 退出；唤醒信号与受锁保护的任务队列配合，任务执行时不持有队列锁。

显示提交也通过该 `eventfd` 通知完成。阻塞提交由通用工作线程执行，UI 可继续处理事件并提前绘制一帧到保留画布；页面尚未完成交接时，不写任何 framebuffer 页。退出循环会等待最后一笔提交完成。模块分工与实测见 [并行化报告](nx5-parallel.md)。

销毁整个 `Framebuffer` 时，先等待提交线程结束，再释放保留画布和 TDE 资源，随后释放板级 SDK 会话，最后解除显示映射并关闭 framebuffer 文件。提交返回错误或线程 panic 时也保持该清理顺序。

平台只弱持有根窗口，避免窗口保存的 Slint 上下文与平台形成强引用环。事件循环出错时，在等待提交完成后隐藏根窗口，释放可见窗口对组件的强引用，并保留原始错误；同一组件仍可再次运行循环。应用释放组件等所有者、UI 线程退出后，平台及 framebuffer 才会析构，不能只检查直接销毁 framebuffer 的路径。

提交 `949ceb9` 的 [启动实机回归](nx5-startup.md) 已验证首次显示失败清理、同一组件重试及任务/定时器延迟创建窗口，每个用例运行两轮；十次退出均无 TDE MMZ 泄漏回收，成功呈现的十帧全幅像素检查通过。正常 WBC 与纹理运行也通过。输入 EIO 和原三项 ignored 硬件测试的历史结果仍见 [177c18a 报告](nx5-tde-exit.md)，不计入本轮重测。未知 DMA 完成状态仍按原生 TDE 安全策略保留资源。

当前 NX5 #233 通过板级准入提供 10 ms 定时器精度参数。通用等待模块据此先阻塞、再在最后一个节拍内检查期限与事件，以减少粗粒度睡眠造成的迟到；参数为零的默认策略仍只阻塞等待。精确检查会增加 CPU 用量，不参与页面退休判断，见 [调度测量](nx5-scheduling.md)。

每轮更新 Slint 定时器与动画、执行有界数量的业务任务、处理输入并检查窗口约束，再按最小帧间隔绘制。没有动画、重绘请求或定时器时无限等待事件；有动画时按帧期限唤醒，不能仅依赖 Slint 的定时器期限。退出或出错会关闭任务接收并清理未执行任务；后续投递返回 `EventLoopTerminated`。同一组件重新运行循环时会重新开放队列。

## 窗口创建与显示失败

允许在 `slint::run_event_loop()` 启动后，由排队任务或 Slint 定时器创建首个根窗口。尚无窗口时，循环仍处理任务、定时器并持续读走输入，暂不分派窗口事件或渲染；存在未执行任务时不进入无限等待。窗口出现后按原有双页路径绘制，平台仍只保存窗口弱引用。

Slint 在调用板级 `set_visible(true)` 前已保留组件的可见引用。布局约束不匹配时，适配层在返回原始错误前调用 `hide()`，使首次 `show()`/`run()` 失败也恢复为不可见并释放该引用。回滚期间只阻止 Slint 为“最后一个窗口关闭”临时请求的自动退出代理；安装时保存的公共代理继续工作，因此不清除用户主动退出请求，也不丢弃启动任务。修正同一组件的约束后可重新显示并运行。

```mermaid
flowchart LR
    S[show 已保留组件引用] --> V{布局满足面板约束?}
    V -- 是 --> D[显示窗口]
    V -- 否 --> H[hide 回滚可见引用]
    H --> Q[抑制本次回滚的自动退出]
    Q --> E[返回原始布局错误]
    E --> R[修正同一组件后重试]
```

[生命周期回归](../src/tests/framebuffer_lifecycle.rs) 验证首次显示失败后 TDE → SDK 析构与映射释放；[启动回归](../src/tests/runtime.rs) 验证连续失败后的隐藏状态、修正后实际像素、已排队任务、显式退出，以及任务积压/定时器中的延迟窗口创建和循环内显示失败重试。新增用例均先在 `177c18a` 上复现失败，再验证修复通过。

## Framebuffer 软件路径

软件渲染使用 Slint `SoftwareRenderer` 的 `ReusedBuffer` 模式。纯软件 feature 保留一份 RGB 图像；`nx5-tde` 保留预乘 RGBA 图像，通过 `TargetPixelBuffer` 对符合条件的纹理操作提交 TDE，其余由软件绘制。显示输出限定 packed true-color：RGB565，或 RGB 三通道各 8 位的 32 位布局，可含 8 位 alpha。通道位置由 framebuffer 位域解析；重叠位域、非标准格式、错误 stride、硬件旋转和不足两页的布局均拒绝。

纯软件路径在 RGB 图像中完成透明混合；`nx5-tde` 路径在预乘 RGBA 图像中完成软件或 TDE 混合。写入 framebuffer 时输出 alpha 位固定为不透明，符合当前独占 UI framebuffer、无视频叠加的范围。位域转换按主机原生字节序写入，保留行尾 padding。

两页首次使用时写入完整的软件图像；随后逐行合并本帧与前一已提交帧的实际脏区，跳过不相交区域之间的空隙，修复交替显示页中遗漏的变化。空脏区不触发提交。只写不可见页，提交契约成功返回后才改变前后页身份。若提交结果不确定，缓冲区进入禁止写入状态，后续重绘返回错误。

绘制与提交已分开：同时至多一笔在途提交和一帧已准备画布；页号、初始化标记及历史脏区仅在完成确认后更新。NX5 的布局约束通过板级契约提供，通用工作线程不包含 NX5 驱动调用。

CPU fence 不等于 DMA 缓存一致性。设备专有实现仍须负责缓存维护、垂直消隐锁存和旧页不再被硬件读取的完成条件。

## 准入边界

[src/backends/mod.rs](../src/backends/mod.rs) 将 NX5 分派给 [src/backends/nx5/mod.rs](../src/backends/nx5/mod.rs)。检查 ARM32、`/dev/fb0`、内核版本、fyfb 名称及 SDK/fyfb/vou 文件 SHA256，再获取进程锁、检查 VO 未被占用并加载厂商库。库加载本身具有 SYS/VO 初始化副作用，不能与其他 SDK 进程并用。[current-target.json](../test-app/devices/nx5/current-target.json) 仅保存带时间戳的环境观测，不控制代码准入，也不实时更新设备状态。

完成提交的回调及内存显示替身均为 crate 私有，应用不能注入回调、构造“已验证”标记或选择公开的无检查显示路径。内存替身用于测试像素、页生命周期和完整事件循环，不能证明物理屏幕行为。

当前 NX5 要求 1024×600 ARGB8888、stride 4096、虚拟高度 1200 和完整两页容量。提交线程调用阻塞 `FBIOPAN_DISPLAY`，成功并读回目标页后才复用旧页；不提供单缓冲或忽略错误的回退。页号读回用于错误检测，不作为硬件退休证明。其他设备仍须提供与驱动匹配的实现和验证。TDE 绘制范围由 [tde/mod.rs](../src/backends/nx5/tde/mod.rs) 与 [native.c](../src/backends/nx5/tde/native.c) 实现，说明及历史像素比较见 [TDE 报告](nx5-tde.md)。

## 构建依赖

两个 package 均要求 Linux、Rust 1.92 或更新版本，默认 features 为空，必须显式选择 `fbdev` 或 `nx5`；`nx5-tde` 包含 `nx5`。`fbdev,nx5` 和 `fbdev,nx5-tde` 组合编译失败。根 workspace 的默认成员只有库；检查测试工程使用 `-p slint-bridge-test-app`，检查两者使用 `--workspace`。

Slint 使用用户指定的本地源码树 `/home/dpower/data/GitHub/slint-1.18.1`，当前核对的 HEAD 为 `95becfd3d6bc2e840df4956b4004bfef936bfe95`。依赖关闭默认桌面后端，启用 `std`、`renderer-software` 和 `compat-1-18`。`nx5-tde` 额外启用同源软件 renderer 的 `experimental`，ARM 构建额外编译使用外部 SDK 头文件的 C 接口，运行时复用已加载的 SDK 会话。

`fontique` 固定到 Slint 当前使用的 0.11.1，并启用 `fontconfig-dlopen`，避免 ARM 构建依赖主机的 Fontconfig 链接配置。没有系统字体库的设备仍需由应用提供字体，例如设置 `SLINT_DEFAULT_FONT` 或在 `.slint` 中导入字体资源。

Cargo.lock 不固定外部 path 依赖的 Git 提交，构建环境需同时保持指定 Slint 源码。消费方若还有其他库从 crates.io 请求 Slint，应在自己的 workspace 根统一 `[patch.crates-io]` 来源，避免同版本号的不同来源混用；应用的 `slint-build` 也应来自同一树。

## 验证范围

主机测试覆盖以下实际行为：

- 准入拒绝发生在设备访问之前；错误帧间隔、布局及 ABI 尺寸检查。
- 真实 Slint 场景输出 RGB565、32 位像素，四种旋转与触摸变换一致，行尾 padding 保持不变。
- 从前台页 0 或 1 启动的交替页脏区修复，准备新帧不修改活动页，空帧不提交，失败后不再写页。
- 字体像素渲染、触摸同步、丢事件重同步、按键重复与释放、设备掉线。
- 真实事件循环接收 evdev 管道、跨线程投递、定时器、动画、退出及再次运行；拒绝不适合屏幕的窗口约束。
- UI 错误与在途提交失败同时发生时保留两个原因，修正 UI 后也不能恢复失败显示实例。
- 在提交成功、失败及 panic 三种情况下直接销毁 `Framebuffer`，检查提交、TDE 和 SDK 释放顺序，并确认映射与文件在最后解除。
- 安装真实 Slint 平台并运行组件，在正常退出、首次显示失败和 PTY 实际 `read()` 返回 EIO 后检查 UI 线程退出确实触发 TDE/SDK 析构；不是手动销毁平台替代应用生命周期。
- 首次显示失败后重试同一组件；延迟创建首个窗口时处理任务积压和定时器，并验证实际呈现的像素。

[test-app/tools/check-feature-matrix.py](../test-app/tools/check-feature-matrix.py) 对三个 feature 分别执行 workspace 测试，确认未选板级、冲突组合编译失败，并检查库的 `fbdev` 普通依赖树没有桌面或 GPU 后端。脚本将结果输出到标准输出，不自动写入 `evidence/`。

2026-10-11 核对当前源码的测试分布如下；计数随测试增减，以执行结果为准：

| Feature | 库内主机测试 | 独立工程公共接口测试 | WBC 生命周期测试 | 默认忽略的实机测试 |
| --- | ---: | ---: | ---: | ---: |
| `fbdev` | 30 | 2 | 0 | 0 |
| `nx5` | 30 | 2 | 3 | 1 |
| `nx5-tde` | 33 | 2 | 3 | 3 |

```sh
# 从仓库根执行；不访问真实显示设备。
python3 test-app/tools/check-feature-matrix.py
cargo fmt --all --check
cargo clippy --locked --offline --workspace --features nx5-tde,slint/mcp --all-targets -- -D warnings
cargo doc --locked --offline --workspace --features nx5-tde --no-deps
```

公共接口测试在 [test-app/tests/public_api.rs](../test-app/tests/public_api.rs)；私有回归在 [src/tests/](../src/tests/)、显示/等待模块内及 [TDE 测试](../src/backends/nx5/tde/tests.rs)。三个实机测试分别测量阻塞翻页、TDE 像素/回退和实际演示的双页像素；需单独部署并独占设备，不能把主机的 ignored 计为通过。执行命令见 [TDE 报告的构建与接入](nx5-tde.md#构建与接入)。

[Framebuffer 析构回归](../src/tests/framebuffer_lifecycle.rs) 使用内存文件、受控提交回调及 TDE 资源替身，不访问真实硬件。[WBC 生命周期回归](../test-app/src/nx5_observer.rs) 使用真实 Slint 应用/定时器、返回错误的平台及受控观察线程，验证错误退出等待和 panic 处理；它不替代设备 WBC 采样。

此前 ARM 构建记录为 EABI5 soft-float、最高 GLIBC 符号版本 2.18、未直接链接 Fontconfig 或厂商库；这是对应二进制的检查结果，换工具链或依赖后需重新核对。历史原始记录存放在被 Git 忽略的 `evidence/`，检出仓库后需另行获取。

主机测试验证软件行为；设备性能与输出样本单独记录。历史 #22 固件的 [NX5 无撕裂验证结论](nx5-present-validation.md) 保持原始口径。

UI 截图、元素检查及应用层输入交互使用 [Slint 原生 MCP](slint-mcp.md)。MCP 输入直接进入 Slint，截图重新渲染窗口；它们不能替代 evdev 解码、framebuffer 页生命周期、像素转换及 TDE 故障注入测试。

当前 NX5 可使用显式的 `.cargo/nx5.toml` CPU 预设和 NEON RGBA→BGRA 写页路径；未使用该预设时仍有原有像素转换路径。整屏性能及当前驱动提交上限见 [帧率优化报告](nx5-fps.md)。
