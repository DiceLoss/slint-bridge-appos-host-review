# Rust 公共平台层

公共平台层已完成，并增加当前 NX5 固件的真实显示实现。`fbdev` 和未识别 NX5 固件仍返回 `DisplayNotAdmitted`；当前 #233 固件使用已验证的阻塞翻页双页实现。实机结果与运行方法见 [上屏验证](nx5-display.md)。

## 统一 API

入口为 `slint_bridge::init(Config)`，在 UI 线程创建 Slint 组件前调用。`Config` 包含 framebuffer 路径、输入设备、固定旋转和最小帧间隔。默认 framebuffer 为 `/dev/fb0`，默认不打开输入设备，帧间隔为 16667 微秒；该间隔是渲染启动频率上限，不是 60 FPS 性能承诺。NX5 原样使用该配置，不再钳制为 33334 微秒。显示页仍只在提交成功完成后复用。

当前支持一个全屏根窗口、至多一个触摸设备及一个按键设备。根窗口应使用响应式布局；可用 `preferred-width`、`preferred-height` 表达偏好，不能固定成与旋转后的逻辑屏幕矛盾的大小。约束不匹配时返回错误。

`InputDevice::touchscreen(path)` 使用当前 NX5 触摸协议 `ABS_X/ABS_Y + BTN_TOUCH`。轴范围由 ioctl 查询，先归一化到物理屏幕，再逆变换为未旋转的 Slint 逻辑坐标。触摸按下、移动和释放只在 `SYN_REPORT` 后发送，避免把接触事件与尚未收齐的坐标组合。收到 `SYN_DROPPED` 后忽略事件直到报告边界，再用 `EVIOCG*` 查询状态，修复按键及接触状态；依据 [Linux evdev 规范](https://docs.kernel.org/input/event-codes.html#ev-syn)。

`InputDevice::keys(path)` 提供方向、确认、返回等导航键。产品可通过 `InputDevice::Keys { path, bindings }` 和 `KeyBinding::new(code, text)` 指定 Linux 键码到 Slint 特殊键或字符的映射。它处理按下、自动重复和释放，不提供键盘布局或输入法。输入掉线会终止事件循环并返回错误，避免持续轮询失效描述符。

## 事件循环

UI 线程使用 `poll` 等待 evdev 与 `eventfd`。业务线程通过标准 `slint::invoke_from_event_loop()` 投递任务，或调用 `slint::quit_event_loop()` 退出；唤醒信号与受锁保护的任务队列配合，任务执行时不持有队列锁。

显示提交也通过该 `eventfd` 通知完成。阻塞提交由通用工作线程执行，UI 可继续处理事件并提前绘制一帧到保留画布；页面尚未完成交接时，不写任何 framebuffer 页。退出循环会等待最后一笔提交完成。模块分工与实测见 [并行化报告](nx5-parallel.md)。

当前 NX5 #233 通过板级准入提供 10 ms 定时器精度参数。通用等待模块据此先阻塞、再在最后一个节拍内检查期限与事件，以减少粗粒度睡眠造成的迟到；参数为零的默认策略仍只阻塞等待。精确检查会增加 CPU 用量，不参与页面退休判断，见 [调度测量](nx5-scheduling.md)。

每轮更新 Slint 定时器与动画、执行有界数量的业务任务、处理输入并检查窗口约束，再按最小帧间隔绘制。没有动画、重绘请求或定时器时无限等待事件；有动画时按帧期限唤醒，不能仅依赖 Slint 的定时器期限。退出或出错会关闭任务接收并清理未执行任务；后续投递返回 `EventLoopTerminated`。同一组件重新运行循环时会重新开放队列。

## Framebuffer 软件路径

软件渲染使用 Slint `SoftwareRenderer` 的 `ReusedBuffer` 模式。纯软件 feature 保留一份 RGB 图像；`nx5-tde` 保留预乘 RGBA 图像，通过 `TargetPixelBuffer` 对符合条件的纹理操作提交 TDE，其余由软件绘制。显示输出限定 packed true-color：RGB565，或 RGB 三通道各 8 位的 32 位布局，可含 8 位 alpha。通道位置由 framebuffer 位域解析；重叠位域、非标准格式、错误 stride、硬件旋转和不足两页的布局均拒绝。

Slint 在 RGB 图像中完成透明混合。输出 alpha 位固定为不透明，符合当前独占 UI framebuffer、无视频叠加的范围。位域转换按主机原生字节序写入，保留行尾 padding。

两页首次使用时写入完整的软件图像；随后逐行合并本帧与前一已提交帧的实际脏区，跳过不相交区域之间的空隙，修复交替显示页中遗漏的变化。空脏区不触发提交。只写不可见页，提交契约成功返回后才改变前后页身份。若提交结果不确定，缓冲区进入禁止写入状态，后续重绘返回错误。

绘制与提交已分开：同时至多一笔在途提交和一帧已准备画布；页号、初始化标记及历史脏区仅在完成确认后更新。NX5 的布局约束通过板级契约提供，通用工作线程不包含 NX5 驱动调用。

CPU fence 不等于 DMA 缓存一致性。设备专有实现仍须负责缓存维护、垂直消隐锁存和旧页不再被硬件读取的完成条件。

## 准入边界

`src/backends/mod.rs` 将 NX5 分派给固件专有实现。检查 ARM32、`/dev/fb0`、内核版本、fyfb 名称及 SDK/fyfb/vou 文件 SHA256，再获取进程锁、检查 VO 未被占用并加载厂商库。库加载本身具有 SYS/VO 初始化副作用，不能与其他 SDK 进程并用。`test-app/devices/nx5/current-target.json` 仅记录环境，不控制代码准入。

完成提交的回调及内存显示替身均为 crate 私有，应用不能注入回调、构造“已验证”标记或选择公开的无检查显示路径。内存替身用于测试像素、页生命周期和完整事件循环，不能证明物理屏幕行为。

当前 NX5 使用阻塞 `FBIOPAN_DISPLAY`，成功并读回目标页后才复用旧页；不提供单缓冲或忽略错误的回退。页号读回用于错误检测，不作为硬件退休证明。其他设备仍须提供与驱动匹配的实现和验证。TDE 绘制已接入，范围和证据见 [TDE 报告](nx5-tde.md)。

## 构建依赖

Slint 使用用户指定的本地源码树，提交为 `95becfd3d6bc2e840df4956b4004bfef936bfe95`。依赖关闭默认桌面后端，启用 `std`、`renderer-software` 和 `compat-1-18`。`nx5-tde` 额外启用同源软件 renderer 的 `experimental`，ARM 构建额外编译使用外部 SDK 头文件的 C 接口，运行时复用已加载的 SDK 会话。

`fontique` 固定到 Slint 当前使用的 0.11.1，并启用 `fontconfig-dlopen`，避免 ARM 构建依赖主机的 Fontconfig 链接配置。没有系统字体库的设备仍需由应用提供字体，例如设置 `SLINT_DEFAULT_FONT` 或在 `.slint` 中导入字体资源。

Cargo.lock 不固定外部 path 依赖的 Git 提交，构建环境需同时保持指定 Slint 源码。消费方若还有其他库从 crates.io 请求 Slint，应在自己的 workspace 根统一 `[patch.crates-io]` 来源，避免同版本号的不同来源混用；应用的 `slint-build` 也应来自同一树。

## 验证范围

主机测试覆盖以下实际行为：

- 准入拒绝发生在设备访问之前；错误帧间隔、布局及 ABI 尺寸检查。
- 真实 Slint 场景输出 RGB565、32 位像素，四种旋转与触摸变换一致，行尾 padding 保持不变。
- 交替页脏区修复，准备新帧不修改活动页，空帧不提交，失败后不再写页。
- 字体像素渲染、触摸同步、丢事件重同步、按键重复与释放、设备掉线。
- 真实事件循环接收 evdev 管道、跨线程投递、定时器、动画、退出及再次运行；拒绝不适合屏幕的窗口约束。

`test-app/tools/check-feature-matrix.py` 对 `fbdev`、`nx5`、`nx5-tde` 分别运行测试，并确认未选板级、冲突板级组合编译失败，软件依赖树没有桌面或 GPU 后端。结果保存在 [feature-matrix.json](../evidence/software-platform/feature-matrix.json)。

第一阶段三个组合各通过 12 个主机测试，格式检查及严格 Clippy 检查通过。示例已交叉链接为 ARM EABI5 soft-float，最高要求的 GLIBC 符号版本为 2.18，未直接链接 Fontconfig 或厂商库。主机示例运行按预期返回未准入错误。完整构建与依赖检查结果见 [validation.json](../evidence/software-platform/validation.json)。该记录对应第一阶段；后续新增对齐/非对齐像素写入测试及 ARM 实机运行，见 [当前验证记录](../evidence/nx5-display/feature-matrix.json)。

主机测试验证软件行为；设备性能与输出样本单独记录。历史 #22 固件的 [NX5 无撕裂验证结论](nx5-present-validation.md) 保持原始口径。

UI 截图、元素检查及应用层输入交互使用 [Slint 原生 MCP](slint-mcp.md)。MCP 输入直接进入 Slint，截图重新渲染窗口；它们不能替代 evdev 解码、framebuffer 页生命周期、像素转换及 TDE 故障注入测试。

当前 NX5 可使用显式的 `.cargo/nx5.toml` CPU 预设和 NEON RGBA→BGRA 写页路径；未使用该预设时仍有原有像素转换路径。整屏性能及当前驱动提交上限见 [帧率优化报告](nx5-fps.md)。
