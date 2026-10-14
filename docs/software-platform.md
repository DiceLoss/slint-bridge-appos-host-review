# Rust 公共平台层

本文描述当前源码的接口与运行行为。NX5 使用板级阻塞翻页双页实现，不按固定固件版本或文件摘要准入；`fbdev` 尚无真实显示准入。目标或显示所有权不符合要求返回 `DisplayNotAdmitted`，SDK 加载和设备 I/O 失败返回 `Io`。约 29 FPS、有限 WBC 样本等实机结果属于对应阶段的观测，不代表所有物理帧的无撕裂认证。

日常构建和运行见 [测试工程](../test-app/README.md) 与 [NX5 设备说明](../test-app/devices/nx5/README.md)。各轮硬件测量由 [文档索引](README.md) 导航；历史报告中的测试数量、PID 和性能数据不定义当前接口。

## 统一 API

入口为 `slint_bridge::init(Config)`，在 UI 线程创建 Slint 组件前调用。公开类型由 [src/lib.rs](../src/lib.rs) 导出：`Config`、`Error`、`Backend`、`Rotation`、`InputDevice`、`KeyBinding`；`selected_backend()` 返回编译期选择，不代表已经通过设备准入。

| `Config` 字段 | 默认值 | 当前约束 |
| --- | --- | --- |
| `framebuffer` | `/dev/fb0` | 可配置任意 framebuffer 设备路径或设备链接 |
| `inputs` | 空列表 | 至多一个 `Touchscreen` 和一个 `Keys` |
| `rotation` | `Rotation::None` | 固定为不旋转或顺时针 90、180、270 度；90/270 度交换逻辑宽高 |
| `frame_interval` | 16667 微秒 | 非零且可加到单调时钟；约束两次渲染启动的最小间隔 |

帧间隔不是 60 FPS 性能承诺。NX5 原样使用该配置，没有专用 33334 微秒钳制；显示页仍只在提交成功完成后复用。`init()` 在设备初始化前检查帧间隔和输入数量，随后执行板级准入、打开 framebuffer 与输入，最后安装 Slint 平台。

`init()` 返回本库的 `Error`。组件创建及 `run()` 返回 Slint 的 `PlatformError`；循环中的输入、绘制和提交错误会转换为其错误消息。退出前仍会等待在途提交；若 UI 已出错、该提交也失败，最终错误以 `PresentationFailed` 为主并保留原 UI 错误原因。提交成功时仍返回原 UI 错误；重复读取同一提交失败不会重复拼接原因。提交失败使该显示实例保持失败状态，不能通过重新运行循环恢复页所有权。

每个平台实例只允许创建一次全屏根窗口；即使释放该窗口，也不能在同一平台上创建第二个。重试和再次运行复用同一组件，不允许嵌套运行事件循环。输入至多一个触摸设备及一个按键设备。根窗口应使用响应式布局；可用 `preferred-width`、`preferred-height` 表达偏好，不能固定成与旋转后的逻辑屏幕矛盾的大小。约束不匹配时返回错误。

`InputDevice::touchscreen(path)` 使用当前 NX5 触摸协议 `ABS_X/ABS_Y + BTN_TOUCH`。轴范围由 ioctl 查询，先归一化到物理屏幕，再逆变换为未旋转的 Slint 逻辑坐标。触摸按下、移动和释放只在 `SYN_REPORT` 后发送，避免把接触事件与尚未收齐的坐标组合。收到 `SYN_DROPPED` 后忽略事件直到报告边界，再用 `EVIOCG*` 查询状态，修复按键及接触状态；依据 [Linux evdev 规范](https://docs.kernel.org/input/event-codes.html#ev-syn)。

`InputDevice::keys(path)` 提供方向、确认、返回等导航键。产品可通过 `InputDevice::Keys { path, bindings }` 和 `KeyBinding::new(code, text)` 指定 Linux 键码到 Slint 特殊键或字符的映射。它处理按下、自动重复和释放，不提供键盘布局或输入法。自定义按键映射要求 Linux key code 小于 768、彼此不重复，且映射文本非空；不满足时返回 `InvalidConfig`。输入掉线会终止事件循环并返回错误，避免持续轮询失效描述符。

每轮对每个输入设备最多进行 8 次非阻塞读取，每次缓冲最多 32 条原始 evdev 记录；这不是 Slint 事件数量或采样频率上限。仍有数据时，描述符保持可读，后续轮次继续处理。该批次限制与业务任务的 2 ms 预算分别执行，避免把输入或任务积压一次性处理到底。

## 事件循环

UI 线程通过 [event_wait.rs](../src/event_wait.rs) 使用 `ppoll` 等待 evdev 与 `eventfd`。业务线程通过标准 `slint::invoke_from_event_loop()` 投递任务，或调用 `slint::quit_event_loop()` 退出；唤醒信号与受锁保护的任务队列配合，任务执行时不持有队列锁。

显示提交也通过该 `eventfd` 通知完成。阻塞提交由通用工作线程执行，UI 可继续处理事件并提前绘制一帧到保留画布；页面尚未完成交接时，不写任何 framebuffer 页。退出循环会等待最后一笔提交完成，并丢弃尚未提交的预渲染帧。再次运行时先按组件的当前状态重绘；未提交帧的脏区保留到后续两页修复中，避免旧画面闪回或局部更新丢失。模块分工与实测见 [并行化报告](nx5-parallel.md)。

销毁整个 `Framebuffer` 时，先等待提交线程结束，再释放保留画布和 TDE 资源，随后释放板级 SDK 会话，最后解除显示映射并关闭 framebuffer 文件。提交返回错误或线程 panic 时也保持该清理顺序。若 TDE 的 DMA 完成状态未知，则原生层保留相关 MMZ 和库引用，通用层同时保留板级 SDK 会话及独占锁至进程退出，避免关闭 SYS 或重新初始化显示影响未确认的硬件作业。

```mermaid
flowchart LR
    E[退出或出错] --> W[等待在途提交完成]
    W --> D[丢弃未提交预渲染帧并保留脏区]
    D --> R[再次运行时重绘当前状态]
    R --> P[后续两页提交修复遗漏脏区]
```

平台只弱持有根窗口，避免窗口保存的 Slint 上下文与平台形成强引用环。事件循环出错时，在等待提交完成后隐藏根窗口，释放可见窗口对组件的强引用，并保留原始错误；同一组件仍可再次运行循环。应用释放组件等所有者、UI 线程退出后，平台及 framebuffer 才会析构，不能只检查直接销毁 framebuffer 的路径。

提交 `949ceb9` 的 [启动实机回归](nx5-startup.md) 已验证首次显示失败清理、同一组件重试及任务/定时器延迟创建窗口，每个用例运行两轮；十次退出均无 TDE MMZ 泄漏回收，成功呈现的十帧全幅像素检查通过。正常 WBC 与纹理运行也通过。三项 ignored TDE/翻页测试已于 2026-10-12 [独立补齐并通过](nx5-hardware.md)。输入 EIO 的历史结果仍见 [177c18a 报告](nx5-tde-exit.md)，未在这次补测中重跑。未知 DMA 完成状态仍按原生 TDE 安全策略保留资源。

NX5 后端提供 10 ms 定时器精度参数，依据 #233 内核的实测分辨率；该参数不是内核版本检查。通用等待模块据此先阻塞、再在最后一个节拍内检查期限与事件，以减少粗粒度睡眠造成的迟到；参数为零的默认策略仍只阻塞等待。精确检查会增加 CPU 用量，不参与页面退休判断，见 [调度测量](nx5-scheduling.md)。

每轮更新 Slint 定时器与动画、执行一批业务任务、处理输入并检查窗口约束，再按最小帧间隔绘制。业务任务每批最多 128 个，并在每个回调返回后检查 2 ms 执行时间预算；到限后先处理输入，未开始的任务按 FIFO 留在队列。每轮至少允许一个任务执行，避免无进展；回调执行时不持有队列锁。单个回调不能被抢占，因此 2 ms 是批次让出阈值，不是触摸延迟的硬性上限，耗时业务仍应放在工作线程。

渲染结束后立即返回事件循环，先处理期间到达的输入，再开始 framebuffer 写入。写页采用约 1 ms 的软预算，每累计约 8192 像素对应的完整行检查时间；至少完成一行，不在行内中断。未写完时继续下一轮输入处理，不睡眠、不重新渲染保留画布；整页完成后才提交后台 PAN。单次 Slint 渲染与输入回调仍在 UI 线程同步执行，预算不构成硬实时保证。自动 evdev 对照及验证范围见 [输入延迟优化](nx5-input-latency.md)。

任务积压时等待超时设为零，不因 `eventfd` 通知已读走而睡眠；`eventfd` 只用于唤醒，任务仍由受锁保护的队列保存。没有未完成写页、待执行任务和等待期限时才无限等待事件；期限由 Slint 定时器，以及可继续绘制时的动画／重绘请求共同决定。预渲染容量已满时，先完成该帧的写页与提交才能继续绘制，输入和业务任务仍可唤醒 UI。退出或出错会关闭任务接收并清理未执行任务；后续投递返回 `EventLoopTerminated`。同一组件重新运行循环时会重新开放队列。

2026-10-12 的队列延迟回归使用真实事件循环、内存画布和 evdev 管道：排入 64 个各休眠 2 ms 的回调，在首个回调内送入完整触摸包，记录 Slint 点击回调执行时间。修复前主机等待 64 个回调、约 131.5 ms；修复后等待 1 个回调、约 2.1 ms，全部任务仍按 FIFO 完成。相同隔离回归在 NX5 #233 上通过，观测约 7.3 ms；板上的睡眠精度和线程调度会影响该数值。另验证清空唤醒后续投递、批次剩余任务以及回调跨线程重入/退出。此实验不加载厂商 SDK、不操作物理触摸设备或显示页，不代表触摸到屏幕扫描的端到端延迟；该隔离回归阶段未切换常驻显示程序；后续已完成 [新版部署和实体触摸验证](nx5-touch-response.md)。原始日志、构建与部署档案位于本地 `evidence/nx5-touch-wake/2026-10-12/`，需另行取得。

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

软件渲染使用 Slint `SoftwareRenderer` 的 `ReusedBuffer` 模式。纯软件在堆中保留一份预乘 RGBA 图像（4 字节/像素）；ARM `nx5-tde` 初始化成功时改用 SDK 分配的 cached MMZ 画布。`FY_MPI_SYS_MmzAlloc_Cached` 同时返回物理地址与 CPU 虚拟映射；Slint 软件绘制访问虚拟映射，TDE 使用同一块内存的物理地址，不再另存堆画布或在每次硬件操作后拷回。作业前同步 CPU 缓存，阻塞 `EndJob` 成功并完成作业后缓存同步后才恢复 CPU 访问。画布通过 `TargetPixelBuffer` 分派符合条件的硬件操作，其余由软件绘制；初始化不可用时仍使用堆画布。显示输出限定 packed true-color：RGB565，或 RGB 三通道各 8 位的 32 位布局，可含 8 位 alpha。通道位置由 framebuffer 位域解析；重叠位域、非标准格式、错误 stride、硬件旋转和不足两页的布局均拒绝。

两条路径均在预乘 RGBA 画布中混合。写入含 8 位 alpha 的 framebuffer 时，RGB 精确反预乘为 straight RGB，通用格式保留原 alpha；alpha=0 写零，alpha=255 直接复制 RGB 通道，其余使用共享倒数表，结果等于 `min(255, (channel × 255 + alpha / 2) / alpha)` 的整数除法。NX5 当前板级显示契约使用 0–128 的逐像素 alpha 量程：写页时按 `round(alpha × 128 / 255)` 编码（等于 `(alpha + 1) / 2` 的整数除法），0 为全透明、128 为不透明。该量程来自实际 WBC 色块比较，不能从 8 位 alpha 位域推断；Slint 画布与 TDE 运算仍使用标准 0–255 alpha。NX5 的 BGRA 位域路径只在目标页交换红蓝并编码 alpha，不修改源画布；不透明和全透明连续区域分别使用独立的 16 像素 NEON 快路径，避免半透明计算的寄存器压力拖慢它们。半透明区域对 8 个像素复用 Q16 倒数，按颜色通道执行向量乘法、舍入收窄和饱和；同一组 alpha 相同时只查表一次。输出 alpha 为 128 或 255 量程时均逐字节等于原整数算法，短尾部及其他量程保留标量路径。算法和实测见 [NEON 反预乘优化](nx5-neon-unpremultiply.md)。RGB565 或无 alpha 的 RGB888 输出保留预乘 RGB，相当于黑底合成，不能透出下层视频。位域转换按主机原生字节序写入，保留行尾 padding。

应用需要透明区域时，在 `.slint` 根窗口设置 `background: transparent;`，并避免用不透明全屏背景覆盖开口；局部颜色 alpha 也会保留。透明 UI 通过独占设备的 `alpha-regression` 验证实际页与黑色 VO 底层上的 WBC 合成；视频解码和业务视频层尚未接入。视频接入方式未定，当前 SDK/VO 独占所有权、初始化和关闭流程保持原有约束。实际视频集成仍须验证非黑色视频底层、图层配置及共用会话生命周期；新增设备/固件还须重新核验 alpha 量程。历史背景见 [slint-nx5 经验](slint-nx5-lessons.md#6-通道顺序预乘和显示-alpha-是三个不同契约)。

两页首次使用时写入完整的软件图像；随后逐行合并本帧与前一已提交帧的实际脏区，跳过不相交区域之间的空隙，修复交替显示页中遗漏的变化。空脏区不触发提交。只写不可见页，提交契约成功返回后才改变前后页身份。若提交结果不确定，缓冲区进入禁止写入状态，后续重绘返回错误。

分段写页保存本帧脏区、两帧合并区域及下一行位置，不增加画布或物理页。期间保留画布不可改写，输入回调只更新应用状态，后续帧再绘制这些变化；整页写完才执行提交前的内存屏障。退出或错误若放弃部分写入，则将后台页标为未初始化，并保留该帧脏区，保证再次运行时重建后台页及修复另一页。

绘制与提交已分开：同时至多一笔在途提交和一帧已准备画布；页号、初始化标记及历史脏区仅在完成确认后更新。NX5 的布局约束通过板级契约提供，通用工作线程不包含 NX5 驱动调用。

CPU fence 不等于 DMA 缓存一致性。设备专有实现仍须负责缓存维护、垂直消隐锁存和旧页不再被硬件读取的完成条件。

## 准入边界

[src/backends/mod.rs](../src/backends/mod.rs) 将 NX5 分派给 [src/backends/nx5/mod.rs](../src/backends/nx5/mod.rs)。检查 ARM32，再获取进程锁、检查 VO 未被占用并加载厂商库。不读取内核版本、构建时间或 framebuffer 驱动名称，也不计算 SDK、fyfb、vou 文件的 SHA256；兼容重编译不再因这些元数据变化被拒绝。库加载本身具有 SYS/VO 初始化副作用，不能与其他 SDK 进程并用。[current-target.json](../test-app/devices/nx5/current-target.json) 仅保存带时间戳的环境观测，不控制代码准入，也不实时更新设备状态。

SDK 按无版本库名 `libVideoDec.so` 交给系统动态加载器查找，可在启动进程时用 `LD_LIBRARY_PATH=/任意/SDK目录` 指定位置；代码不再要求 `/system/app/clib`。若 SDK 文件自带版本后缀，在该目录提供 `libVideoDec.so` 链接即可，后端不限制实际文件的版本名称。首次加载后若 SDK 构造函数已启用 VO，就接管该次引用；若库已驻留且 VO 未启用，或构造函数不自动启用，则调用一次 `VO_Enable()`。初始化后续失败和正常析构均只配对调用一次 `VO_Disable()`；库映像通过 `RTLD_NODELETE` 保持驻留，使 SDK 的 SYS/VB/MMZ 描述符缓存可被后续会话复用，避免重复加载后遗失缓存并累积描述符。这不保留正常会话的 VO 引用或独占锁；锁竞争返回 `DisplayNotAdmitted`，其他锁调用错误仍为 `Io`。VO 检查必需符号与调用结果，TDE 以同一库名及 `RTLD_NOLOAD` 复用会话，不会另行初始化 SDK。调用者仍需提供兼容 `codec/vd/VoApi.h`、TDE 头文件和阻塞 PAN 完成语义的 SDK/驱动；符号存在和布局通过并不能自动证明 ABI 或完成语义兼容。

完成提交的回调及内存显示替身均为 crate 私有，应用不能注入回调、构造“已验证”标记或选择公开的无检查显示路径。内存替身用于测试像素、页生命周期和完整事件循环，不能证明物理屏幕行为。

NX5 打开 `Config::framebuffer` 指定的设备，通过 `FBIOGET_FSCREENINFO` / `FBIOGET_VSCREENINFO` 获取实际分辨率、stride、像素位域、虚拟尺寸和显存容量，使用前述通用布局校验，不限定 1024×600 或固定 stride。尺寸由设备当前模式决定，要求至少两页完整容量；两页偏移为 `0` 与实际 `yres`。初始化布局传给提交线程；每次 PAN 前核对尺寸、stride 和像素格式，读回后再次核对尺寸/格式与目标页，布局变化或无效偏移会终止提交。提交线程调用阻塞 `FBIOPAN_DISPLAY`，成功并读回目标页后才复用旧页；不提供单缓冲或忽略错误的回退。页号读回用于错误检测，不作为硬件退休证明。其他设备仍须提供与驱动匹配的实现和验证。TDE 不再以 1024×600 限制画布大小，仍检查尺寸溢出、SDK 对齐及作业完成状态；SDK 不支持或无法分配时使用既有软件回退。TDE 绘制范围由 [tde/mod.rs](../src/backends/nx5/tde/mod.rs) 与 [native.c](../src/backends/nx5/tde/native.c) 实现，说明及历史像素比较见 [TDE 报告](nx5-tde.md)。使用 `nx5-tde` 构建时，板级像素类型保持 Slint 预乘 RGBA 语义，对圆角矩形等操作内部的连续同色混合使用 CPU 批量路径；Cortex-A7/NEON 预设启用 8 像素向量混合，尾部及无 NEON 构建使用同一精确整数算法，不改变 RGB 的 `/255` 向下取整或 alpha 的原有舍入。不透明填充保留 Slint 的批量填充。此 CPU 路径不提交 DMA，不依赖 TDE 作业成功；图片 TDE 复制/混合及未知 DMA 隔离仍按原有契约执行。

`nx5-tde` 的等尺寸 RGBA 纹理支持整体透明度。上传时按 Slint 顺序计算：straight RGBA 先合并逐像素/整体 alpha，再预乘 RGB；预乘 RGBA 直接缩放四个通道。Cortex-A7 预设使用 NEON；其余构建使用相同整数规则。准备后的缓存键包含透明度，透明度变化会重新上传，同一图像与透明度可复用。SDK 的全局 alpha 开关不符合该预乘语义，因此不使用；混合仍限定不透明目标和偶数矩形，单次 RGB 容差为 1/255。

`SLINT_BRIDGE_TDE_FILL=1` 在 `nx5-tde` 构建中显式开启 QuickFill 对照路径：纯色窗口背景和无圆角/边框的不透明整数矩形可进入硬件，至少 32768 像素、至多 16 个裁剪区域；背景清除可保留预乘 alpha。其他值或未设置时使用 CPU 批量填充，因为共享 MMZ 后重新测量的 22 组面积/形状/颜色对照仍均为 CPU 更快；全屏 CPU 写 MMZ 约 1.69 ms，TDE 含提交和同步约 2.86 ms。硬件填充直接写共享画布，同样遵守完成确认、失败回退及未知 DMA 隔离，不增加 MMZ 块。`SLINT_BRIDGE_TRACE` 的 TDE 行末新增 `fill` 次数；`software` 仍只计未加速的纹理回调。初次接入记录见 [填充与整体透明度实测](nx5-fill-blend.md)，共享画布复测见 [MMZ 实测](nx5-shared-canvas.md)。

共享画布上的已知完成错误可能留下部分硬件结果：立即禁用 TDE，将当帧剩余软件回调导向一行预分配隔离区；本次 renderer 调用返回后，以 `NewBuffer` 全量软件重绘同一 MMZ 画布，再恢复原 repaint 模式并提交完整脏区。画布持有 SDK/MMZ 所有权，禁用加速不会提前释放它。未知 DMA 完成时，后续回调同样不能访问画布，整帧返回错误，不写物理页；保留原有 MMZ、SDK 会话及显示锁隔离。该策略不分配整帧备份。

已知 DMA 完成后的 MMZ 释放失败会永久关闭进程内 TDE 创建门闩，后续使用软件路径；同一地址不重复释放，避免部分释放后重复分配累积资源。未知 DMA 仍保持原有整帧失败与会话隔离。

## 构建依赖

两个 package 均要求 Linux、Rust 1.92 或更新版本，默认 features 为空，必须显式选择 `fbdev` 或 `nx5`；`nx5-tde` 包含 `nx5`。`fbdev,nx5` 和 `fbdev,nx5-tde` 组合编译失败。根 workspace 的默认成员只有库；检查测试工程使用 `-p slint-bridge-test-app`，检查两者使用 `--workspace`。

Slint 使用用户指定的本地源码树 `/home/dpower/data/GitHub/slint-1.18.1`，当前核对的 HEAD 为 `22804ff0f0b5d21270798cb4fec75e69c47f70f6`。依赖关闭默认桌面后端，启用 `std`、`renderer-software` 和 `compat-1-18`。`nx5-tde` 额外启用同源软件 renderer 的 `experimental`，ARM 构建额外编译使用外部 SDK 头文件的 C 接口，运行时复用已加载的 SDK 会话。

`fontique` 固定到 Slint 当前使用的 0.11.1，并启用 `fontconfig-dlopen`，避免 ARM 构建依赖主机的 Fontconfig 链接配置。没有系统字体库的设备仍需由应用提供字体，例如设置 `SLINT_DEFAULT_FONT` 或在 `.slint` 中导入字体资源。

Cargo.lock 不固定外部 path 依赖的 Git 提交，构建环境需同时保持指定 Slint 源码。该版本还修复文字布局缓存淘汰时未通知重绘依赖的问题：否则持续动画后切页可能保留旧标题，而完整截图仍显示新标题。展示应用的保留缓冲回归见 `test-app/tests/showcase_repaint.rs`。缓存容量限制仍保持不变。

消费方若还有其他库从 crates.io 请求 Slint，应在自己的 workspace 根统一 `[patch.crates-io]` 来源，避免同版本号的不同来源混用；应用的 `slint-build` 也应来自同一树。

## 验证范围

主机测试覆盖以下实际行为：

- 不受支持的后端在设备访问前拒绝；错误帧间隔、布局及 ABI 尺寸检查。
- framebuffer 与触摸设备使用配置路径，布局支持不同分辨率与行尾 padding，触摸轴映射到实际面板尺寸；翻页拒绝错误偏移及运行中的布局变化。
- 真实 Slint 场景输出 RGB565、32 位像素，四种旋转与触摸变换一致，行尾 padding 保持不变。
- 从前台页 0 或 1 启动的交替页脏区修复，准备新帧不修改活动页，空帧不提交，失败后不再写页。
- 字体像素渲染、触摸同步、丢事件重同步、按键重复与释放、设备掉线。
- 真实事件循环接收 evdev 管道、跨线程投递、定时器、动画、退出及再次运行；拒绝不适合屏幕的窗口约束。
- UI 错误与在途提交失败同时发生时保留两个原因，修正 UI 后也不能恢复失败显示实例。
- 在提交成功、失败及 panic 三种情况下直接销毁 `Framebuffer`，检查提交、TDE 和 SDK 释放顺序，并确认映射与文件在最后解除。
- 安装真实 Slint 平台并运行组件，在正常退出、首次显示失败和 PTY 实际 `read()` 返回 EIO 后检查 UI 线程退出确实触发 TDE/SDK 析构；不是手动销毁平台替代应用生命周期。
- 首次显示失败后重试同一组件；延迟创建首个窗口时处理任务积压和定时器，并验证实际呈现的像素。
- 正常退出和布局错误后的再次运行不提交旧预渲染画面；丢弃帧的局部脏区在两页间完整修复。
- SDK 构造函数自动/非自动启用、库驻留与重复获取，以及 framebuffer/输入初始化失败后的 VO 引用配对；未知 DMA 时保留 SDK 会话。
- framebuffer 文件锁竞争返回 `DisplayNotAdmitted`，非竞争锁错误保留原 I/O 分类。

原生 [check-native.sh](../test-app/tools/nx5/tde/check-native.sh) 用主机替身 SDK 验证无版本库名在不同目录下的搜索、`RTLD_NOLOAD` 会话复用及引用释放，同时检查 TDE 故障处理与像素转换边界；不代表新源码已部署或通过实机验证。

[test-app/tools/check-feature-matrix.py](../test-app/tools/check-feature-matrix.py) 对三个 feature 分别执行 workspace 测试，确认未选板级、冲突组合编译失败，并检查库的 `fbdev` 普通依赖树没有桌面或 GPU 后端。脚本将结果输出到标准输出，不自动写入 `evidence/`。

2026-10-12，`bccf6b9` 阶段记录的测试分布如下。此后的队列延迟修复为各板级 feature 新增 3 项库内主机回归；当前计数以执行结果为准：

| Feature | 库内主机测试 | 独立工程公共接口测试 | WBC 加载/生命周期测试 | 默认忽略的实机测试 |
| --- | ---: | ---: | ---: | ---: |
| `fbdev` | 37 | 2 | 0 | 0 |
| `nx5` | 40 | 2 | 4 | 1 |
| `nx5-tde` | 44 | 2 | 4 | 3 |

`nx5-tde` 还运行启动诊断程序中的一项 Linux framebuffer UAPI 布局测试。

```sh
# 从仓库根执行；不访问真实显示设备。
python3 test-app/tools/check-feature-matrix.py
cargo fmt --all --check
cargo clippy --locked --offline --workspace --features nx5-tde,slint/mcp --all-targets -- -D warnings
cargo doc --locked --offline --workspace --features nx5-tde --no-deps
```

公共接口测试在 [test-app/tests/public_api.rs](../test-app/tests/public_api.rs)；私有回归在 [src/tests/](../src/tests/)、显示/等待模块内及 [TDE 测试](../src/backends/nx5/tde/tests.rs)。SDK/显示专用测试覆盖阻塞翻页、TDE 像素/回退与调用链、演示组件的内存双页对照、填充面积/形状耗时，以及共享画布到真实 framebuffer 的转换写页；需单独部署并独占设备，不能把主机的 ignored 计为通过。另有仅在 ARM 编译的 `neon_writeback_boundaries` ignored 测试，只执行 CPU 转换、不加载 SDK，可在显示程序运行时单独执行。执行命令见 [TDE 报告的构建与接入](nx5-tde.md#构建与接入)。

[Framebuffer 析构回归](../src/tests/framebuffer_lifecycle.rs) 使用内存文件、受控提交回调及 TDE 资源替身，不访问真实硬件。[WBC 生命周期回归](../test-app/src/nx5_observer.rs) 使用真实 Slint 应用/定时器、返回错误的平台及受控观察线程，验证错误退出等待和 panic 处理，并通过子进程与替身动态库确认 `LD_LIBRARY_PATH` 加载及配置路径传递；它不替代设备 WBC 采样。

此前 ARM 构建记录为 EABI5 soft-float、最高 GLIBC 符号版本 2.18、未直接链接 Fontconfig 或厂商库；这是对应二进制的检查结果，换工具链或依赖后需重新核对。历史原始记录存放在被 Git 忽略的 `evidence/`，检出仓库后需另行获取。

主机测试验证软件行为；设备性能与输出样本单独记录。历史 #22 固件的 [NX5 无撕裂验证结论](nx5-present-validation.md) 保持原始口径。

UI 截图、元素检查及应用层输入交互使用 [Slint 原生 MCP](slint-mcp.md)。MCP 输入直接进入 Slint，截图重新渲染窗口；它们不能替代 evdev 解码、framebuffer 页生命周期、像素转换及 TDE 故障注入测试。

独立测试工程另提供七页 `showcase`，用法与验证范围集中在 [应用说明](../test-app/README.md#展示应用-showcase)。其 `showcase-preview` feature 仅为应用提供同源 Slint headless 软件预览；正常设备运行仍调用本库 `init(Config)`。标准控件所需的 SVG 解码依赖由测试工程引入，不改变库的 features、默认值、提交契约或设备准入。

当前 NX5 可使用显式的 `.cargo/nx5.toml` CPU 预设和 NEON RGBA→BGRA 写页路径；未使用该预设时仍有原有像素转换路径。生产写页没有使用 TDE：单次格式转换与 CPU 转换后 TDE 搬运只存在于独立诊断探针，所测正确路径更慢，见 [TDE 写页调查](nx5-tde-writeback.md)。整屏性能及当前驱动提交上限见 [帧率优化报告](nx5-fps.md)。
