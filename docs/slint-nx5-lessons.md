# 从 slint-nx5 历史吸收的 NX5 经验

> 本文记录 `287da8c` 阶段的调查与验证范围。后续部署实测发现当前 NX5 的逐像素 alpha 在 128 达到全不透明，直接输出 0–255 会使半透明偏亮；现行板级编码见 [平台说明](software-platform.md#framebuffer-软件路径)。下文原始结果不改写为该后续修复的证据。

调查日期：2026-10-12。结论基于提交、源码、SDK 头文件/二进制、历史原始档案及本轮回归；历史调查阶段未操作 NX5，本轮 ARM CPU 补测单独记录于文末。没有修改外部依赖。原源码目录保持只读，另在临时 Git 副本中补齐了同一 origin 的历史。

本次比较基线为 Slint Bridge `7a1899fa7d8b2f9461aa5a3e451c2a763ebe7012`。历史比较以该基线为准，第 6、7 节另列本轮实际修改；不把历史实机成绩当作本轮的新验证。调查期间用户明确 UI 下方存在视频层、透明区域必须透出视频；因此基线“不透明 UI”前提已不满足需求，第 6 节记录已吸收的 alpha 输出方案及仍待接入的视频合成契约。

## 来源与可追溯范围

旧源码位于外部目录 `/home/dpower/data/setting-nx5/vendor/slint-nx5`，**不属于本仓库**。本文对旧源码的路径均相对此目录；本仓库源码使用 `../src/` 等相对链接。复查某历史位置可运行 `git -C /home/dpower/data/setting-nx5/vendor/slint-nx5 show <完整提交>:<文件>`，再以 `nl -ba` 查看行号。行号以注明的提交为准。

旧仓库 HEAD 为 `d247651fee87b5722ae22250fa60fe2aa51f2c40`，原目录 `git rev-parse --is-shallow-repository` 返回 `true`，`.git/shallow` 指向 `e229391dcdc2650e5fe83040cc1edbff695e6fc1`，初始只能看到六个提交。随后用独立 bare 副本 `/tmp/slint-nx5-history-ygxzvlsx/repo.git` 补齐到 30 个提交，`--is-shallow-repository=false`，可追溯到根提交 `50d39a8a4c2b17db1e27f629aa0f43582ca7cebc`；原目录及 HEAD 未改。该临时路径不是交付依赖，复查更早提交须自行取得完整历史。

末六个提交及适用边界如下；早期有实质经验的修复链在第 6、8、9 节展开：

| 完整提交 | 可确认的变化 | 证据边界 |
| --- | --- | --- |
| `e229391dcdc2650e5fe83040cc1edbff695e6fc1` | 实际仅更新 README/spec 触摸文档 | 完整历史确认实现来自上一提交 `3b4b1fef5ec27816c990364fd08f8775fda254b4`；浅目录的整树新增显示不代表真实引入 |
| `c4247f65455e7e00d6c9bac19b1d6a42e823bab9` | TDE、共享上传区、保留 RGBA 画布、脏区提交、生命周期替身测试 | 提交说明记录 host/ARM/像素测试通过，同时明确坐标测试仍失败；这些数量不是本次实机结果 |
| `5d569544962478b0739187317b67162c533c57e4` | 格式化 `crates/slint-nx5-core/src/nx5_tde.c` | 本次比较前后 C token 序列相同，没有发现行为变化 |
| `adce9259ed2e0d482e18f3a50649ee427841f6a0` | JPEG 缩放、硬件准入、软件工作内存预算、分阶段资源清理 | 提交说明记载 8 host/9 ARM 测试、50 次拖动和 11 次 framebuffer 比较；本次仅重跑 native 生命周期替身 |
| `f0e9311decc0c98db0099bc6cbed2496596b4504` | Slint 1.18.1、精确 alpha 转换、背景 key、翻页线程、双页损坏历史、触摸初值及计时修复 | 提交说明引用既往 211 帧及页截图；本次可重跑的独立 framebuffer 用例为 9 项 |
| `d247651fee87b5722ae22250fa60fe2aa51f2c40` | 三个 Slint patch 从固定 rev 改为 `master` | 仅 Cargo 清单及 README，未增加 NX5 行为验证 |

已检查末六个提交和下述相关早期修复的父子 diff，核对原始规格与现行源码。旧 README 的 14.94→22.60 / 15.04→22.65 FPS（`d247651fee87b5722ae22250fa60fe2aa51f2c40`，`README.md:33–40`）可追溯到外部 `/home/dpower/data/setting-nx5/evidence/slint-1.18.1/flip-pipeline/` 的日志、`synthetic-results.json`、`touch-results.json` 及 `run-bench.py:24–29`。其中统计成功阻塞提交且页号读回后的 ACK，排除预热；报告明确非光学扫描测量。它们是旧二进制和旧用例档案，不与本项目约 29 FPS 的不同用例直接比较，亦不作为当前二进制的新实测。

## 1. 交替物理页必须修复上一帧损坏，完成后才能推进历史

旧 `f0e9311decc0c98db0099bc6cbed2496596b4504` 的 `crates/slint-nx5-core/src/framebuffer/page_state.rs:14–28` 把本帧和上一已提交帧的 damage 合并；`:35–75` 用相离更新验证交替页完整性及未确认时不推进状态。`framebuffer.rs:418–423` 仅在 worker 返回成功后调用 `committed()`。这直接解释了“只拷本帧脏区”为什么会让隔帧旧内容重新出现。

当前已经吸收并扩展：[完成处理](../src/framebuffer.rs) 才改变 front/历史；[写页逻辑](../src/framebuffer.rs) 在首次使用每一页时全量初始化，之后逐行合并实际区域，避免旧版大 bounding box 把两块相离脏区中间的空隙也复制。还有[预渲染丢弃后的损坏传递](../src/framebuffer.rs)，防止“画布已变但尚未提交”的更新丢失。[回归](../src/tests/framebuffer.rs)覆盖两页在等待期间保持不变、初始 front=1、稀疏区域和丢弃预渲染。

应继续保留这一状态模型。提高吞吐时不能以清空历史、提前交换 front，或只更新一张物理页来降低拷贝量。

## 2. 保留有界翻页并行；不能照搬旧版单页回退及页号确认

旧 `f0e9311decc0c98db0099bc6cbed2496596b4504` 的 `crates/slint-nx5-core/src/framebuffer/flip_worker.rs:13–86` 使用容量 1 的请求/完成队列、64 KiB 栈、sticky error，并在析构时 join。`:98–153` 的测试确认后台阻塞时可继续绘制、失败后拒绝复用、退出前等待 ioctl。

但同一提交的 `framebuffer.rs:299–305` 会在两页不足时退成单页；`:333–351` 初始化清空映射；`:428–464` 用 `FBIOPUT_VSCREENINFO` 加 yoffset 读回来确认页切换。源码注释援引 LGUI3，不构成适用于其他固件的物理退休证明。

当前 [Presentation](../src/presentation.rs) 保留有界 worker，并增加完成/异常退出唤醒；[NX5 提交](../src/backends/nx5/mod.rs) 使用阻塞 PAN，前后校验布局与读回值；[平台边界](software-platform.md#准入边界)明确要求匹配的完成契约、至少两页，且读回仅用于错误检测。旧工程能运行的单页模式、PUT 模式设置及“页号对就写旧页”的推断均不能作为本项目优化方案。

## 3. EndJob 失败不是作业未执行；安全回退必须有明确的像素所有权

旧 `c4247f65455e7e00d6c9bac19b1d6a42e823bab9` 引入的恢复逻辑，在格式化后的 `5d569544962478b0739187317b67162c533c57e4` 中见 `crates/slint-nx5-core/src/nx5_tde.c:60–70, 243–257`：EndJob 失败后 WaitAllDone/Reset 成功之前不交还 CPU 或释放 MMZ。原始 SDK 契约来自 `c4247f65455e7e00d6c9bac19b1d6a42e823bab9` 的 `crates/slint-nx5-core/include/nx5/mpi_tde.h:42–83`：timeout 单位为 10 ms，CancelJob 仅在调用 EndJob 前可能成功。

这项原则值得保留，但旧恢复循环没有上界，调用 Reset 还会影响共享 TDE 状态；同一旧文件 `:239–243` 在绘图构建失败时忽略 CancelJob 返回值。旧测试的 CancelJob 替身始终成功，不能覆盖这一缺口（`f0e9311decc0c98db0099bc6cbed2496596b4504`，`crates/slint-nx5-core/tests/tde_lifetime.c:16–25`）。不能因为替身 PASS 就认为真实 SDK 的所有失败路径已证明安全。

本轮 `287da8c` 阶段采用私有 MMZ 暂存目标，成功等待且完成 cache 同步后才拷回 CPU 画布。后续 `d34f436` 已改为 [CPU/TDE 共享 MMZ](nx5-shared-canvas.md)：成功后不再回拷；已知完成的错误立即禁用 TDE，将剩余回调导向隔离行，再全量软件重绘画布。取消失败或 EndJob 失败仍标记未知 DMA，保留 MMZ/库/TDE 句柄，Rust [拒绝访问画布及提交该帧](../src/backends/nx5/tde/mod.rs)，并[保留板级会话和锁](../src/presentation.rs)。[故障回退回归](../src/backends/nx5/tde/tests.rs)区分安全错误与未知完成状态；后续验证不计入本文原始成绩。

提高效率不能删除 CPU 提交前的同步、把未知状态转成成功的软件帧，或直接引入旧版无限等待/全局 Reset。

## 4. SDK 句柄、MMZ、显示所有者必须一起考虑

旧 `f0e9311decc0c98db0099bc6cbed2496596b4504` 的 `crates/slint-nx5-core/src/framebuffer.rs:182–211` 在 raw fd 完成初始化前调用 VO_Enable，部分错误手动 close 但保留 `self.fb_fd`；`:575–600` 析构无条件 VO_Disable。TDE 作为结构体字段在 Drop 方法后才析构，而旧 `nx5_tde.c:47–57` 又访问 SDK 来释放 MMZ。`crates/slint-nx5-core/build.rs:16–22` 直接链接 VideoDec，没有当前这种“检查所有者后显式加载”的边界。这些是可见代码风险，不是本次复现的设备故障。

当前 [Session](../src/backends/nx5/mod.rs) 检查 VO 所有者、持有独占锁、用 RTLD_NODELETE 保持 SDK 全局描述符缓存；TDE [RTLD_NOLOAD](../src/backends/nx5/tde/native.c) 只借用现有 SDK。[Framebuffer](../src/framebuffer.rs) 先结束 worker，再按 TDE→SDK→映射/文件的顺序销毁；[运行时弱引用](../src/runtime.rs)和错误隐藏窗口使这些析构实际可达。需要测试“应用完整退出”，不能只测试单个 free 函数。

这些当前修复的实机口径应分别查看[退出清理报告](nx5-tde-exit.md)和[会话重试报告](nx5-review-fixes.md)；它们对应当时提交/固件，不是本次新实测。WBC 必须继续走[显示进程内观察器](../test-app/src/nx5_observer.rs)；旧代码未提供“第二进程加载 SDK 无副作用”的依据。

## 5. 已补充保护：MMZ 释放失败后禁止反复分配

旧 JPEG 提交 `adce9259ed2e0d482e18f3a50649ee427841f6a0` 比旧 TDE 核心更细致：`crates/slint-nx5-jpeg/src/nx5_jpeg.c:23–80` 保存每一步已确认的清理进度。`:35–45` 记载 SDK 可能部分 unmap/free 后报告失败，因此把状态记为不确定，不再重试该物理地址；`:98` 在新分配前处理 retained owner。对应 `crates/slint-nx5-jpeg/tests/vgs_lifetime.c:72–106` 覆盖共享 codec 引用、frame/channel 清理失败、部分释放与禁止重复释放/新分配。本次 ASan/UBSan 重跑通过；这验证替身状态机，不能代替真实 SDK 故障，实际语义另以下方二进制控制流核对。

本次还核对两个只读 SDK 二进制：LGUI3 `476e327ebec2886b2b10e7bb93647f4adf814d25` 下的 `c_library/third_party/lib/NX5/codec/libVideoDec.so`，SHA256 `8064b9d16c7c88b6b6d40c66e40b754e392c36ab03803cb1f04c6ed0a331bd82`，与旧适配器随附库相同；`codec_rorate/libVideoDec-1.6.6.so` 的 SHA256 为 `cb50026662c2c59955c775a96f567333e62a8dbc2f60fb21a01c0b5d11385092`，与本项目历史设备 SDK 记录相符。ARM objdump 显示两者的 FY_MPI_SYS_MmzFree 都先 unmap ioctl、再 free ioctl，第一步失败仍执行第二步，任一失败可返回 -1。后者调用地址为 `0x1c46e` / `0x1c482`，失败后继续分支为 `0x1c4b0`；前者为 `0x1bb06` / `0x1bb1a` / `0x1bb48`。这是本地特定二进制的控制流证据，本次没有核对正在运行设备的 SDK 文件或触发实际失败。

比较基线的 [bridge_tde_destroy](../src/backends/nx5/tde/native.c) 对正常已完成作业的 MmzFree 返回值不作处理，并释放 `in_use`。它**没有重试同一地址**，已避开重复释放这一风险；但 SDK 未释放成功时，后续引擎仍可能再次分配并累计未释放资源。基线 native 替身的 free 总是成功，未覆盖此路径。对“未知 DMA”的隔离不能自动覆盖“DMA 已完成但释放结果不明”。本轮先增加故障替身复现创建门闩被重新开放，再修改清理逻辑。

本轮实现逐块尝试清理；任一 MmzFree 失败即记录错误，并保持进程内 TDE 创建门闩关闭。同一地址不重试，其余独立块仍各释放一次；最多遗留这一引擎的分配，后续创建退回软件，避免重试累积。此处 DMA 已确认结束，可以关闭该 TDE 及 SDK 引用；未知 DMA 仍走原来的完整资源/会话隔离。新增“不释放即报错”和“释放后仍报错”替身，分别检查三次创建重试均提前拒绝、释放计数不增长、所有正常清理步骤执行；ASan/UBSan 通过。未在真实 SDK 中人为制造释放错误。

## 6. 通道顺序、预乘和显示 alpha 是三个不同契约

历史先有 `600c5770fcc48f2032838bf56a2f8387ebe7618c` 的 `crates/slint-nx5-core/src/platform.rs:271–281` 为修复红蓝颠倒，在显示前原地交换 R/B；随后 `7049e2072320a7e17d1c52b1437f4a206ff816b3` 同文件 `:282–287` 又补充显示后换回，提交说明明确指向动画中颜色来回翻转。可见修复显示格式却改坏 retained buffer 会造成下一帧问题。`c4247f65455e7e00d6c9bac19b1d6a42e823bab9` 最终改为 ReusedBuffer 和只在提交时转换脏区（`crates/slint-nx5-core/src/platform.rs:220–229, 328–334`），避免反复改变保留画布格式。

`f0e9311decc0c98db0099bc6cbed2496596b4504` 的 `crates/slint-nx5-core/src/framebuffer/present.rs:1–38` 为旧视频叠加路径增加精确反预乘 RGBA→straight BGRA；`:108–137` 遍历所有 channel/alpha 值，并验证半透明红色。本次独立测试通过。

比较基线的 [write_row](../src/framebuffer.rs)和 [NEON](../src/backends/nx5/tde/native.c) 固定输出 alpha=255，纯软件 retained buffer 还只保存 RGB；这与用户明确的“透明 UI 透出下层视频”要求不符。本轮已吸收旧版精确反预乘输出：纯软件与 TDE 都保留 premultiplied RGBA 画布，只在写屏时转换为 straight BGRA，并保留原始 alpha，alpha=0 输出零。只取消 NEON 的 alpha=255 而不调整软件像素类型/通用写页路径是不完整修复。新增回归先在纯软件和 TDE feature 上复现透明背景被写为 alpha=255，再验证七帧移动、透明度变化和消失后的页历史、像素及 padding；Rust/C 分别对全部 65,536 组通道/alpha 组合与整数除法参考比对，另检验未对齐写入和源画布不变。RGB565/XRGB 没有 alpha，仍输出黑底预乘颜色；纯软件画布由 3 增至 4 字节/像素，1024×600 增加 600 KiB，TDE 原本就是 4 字节。应用用法以[平台说明](software-platform.md#framebuffer-软件路径)为准。用户已明确本次先支持透明 UI，视频接入方式未定，因此 VO 所有权流程未改、最终视频合成仍未验证。

LGUI3 参考源码同样是外部目录，当前只读 HEAD 为 `476e327ebec2886b2b10e7bb93647f4adf814d25`：`c_library/src/lgui/lgui_ctrl/lgui_ctrl_vdec.c:17–23, 69–82` 用视频控件计数并绘制零色开口；`c_library/src/lvgl/lv_draw/lv_draw_blend.c:363, 415, 789` 在有视频控件时跳过若干 GPU 绘制路径；`c_library/src/lvgl/lv_misc/lv_color.h:609–626` 的软件混合在透明背景上保留前景 RGB 与 alpha，形成 straight 输出。其 both-translucent 分支 `:651–657` 是 NX5 特例，不是一般 source-over 公式，不能移植到 Slint。旧 `alpha-submit/REPORT.md:64–74` 验证了 framebuffer 的精确反预乘，但明确没有测试实际下层视频和物理面板。

显示契约仍须单独验证。LGUI3 同提交 `c_library/third_party/include/NX5/codec/include/fy_common.h:127–135` 的 ALPHA_S 只有使能、逐像素 alpha、ARGB1555 值和 global alpha；`codec/mpi_vo.h:122–131` 暴露 SetGraphicLayerAlpha，却没有显示 premultiplied/straight 的 getter。旧 `evidence/slint-1.18.1/display-alpha-contract/REPORT.md` 只查到 ARGB 位域，WBC 曾超时，不能证明最终混合公式；不要把 TDE 的预乘选项当成显示图层配置。应使用当前已验证的进程内 WBC 与受控下层颜色，检查 alpha=0/128/255 和多个 RGB 通道的合成结果，并核对 UI/视频共用 SDK 会话的所有权与退出次序，不能放宽 VO 所有者检查后直接另起 SDK 进程。

当前 TDE [上传预乘](../src/backends/nx5/tde/native.c)使用与软件路径相同的截断 `/255`，并对透明目标、奇数裁剪软件回退；这是可以保留的安全边界。旧 `f0e9311decc0c98db0099bc6cbed2496596b4504` 的 `crates/slint-nx5-core/src/nx5_tde.c:166–193, 254–257` 通过扩展偶数区域再回写边缘支持奇数裁剪，复杂度和故障状态更大，不应仅为提高命中率直接移植。

## 7. 已吸收的效率改进：命中时刷新两槽替换次序

旧 `c4247f65455e7e00d6c9bac19b1d6a42e823bab9` 固定一个全屏不透明背景和一个临时上传区。`f0e9311decc0c98db0099bc6cbed2496596b4504` 中 `crates/slint-nx5-core/src/nx5_tde.c:82–140, 148–155` 保持这一分工，并通过 Rust `crates/slint-nx5-core/src/tde.rs:192–229` 的 TextureCacheKey 命中避免再次逐行 memcmp/上传。`tests/tde_lifetime.c:48–63, 78–101` 验证背景复用、透明叠加不驱逐背景、失败上传使元数据失效；本次 native 重跑通过。

当前已经使用稳定 key，且允许[两个来源槽](../src/backends/nx5/tde/mod.rs)。基线的 round-robin 也会让无 key 临时纹理切换 `next_slot`；在背景 A 后绘制 B、C，再回到 A 的场景，背景可能被驱逐而重复上传。旧“一背景＋一上传区”提供了可验证的取舍，但会减少普通纹理的缓存容量，不能直接断言总是更快。

本轮使用真实 Slint Scene 与模拟 accelerator 复现：8 帧固定背景＋逐帧新前景，原实现背景上传 4 次、总上传 5,242,880 字节。最小修改为“命中时也更新两槽替换次序”，保持三块 MMZ 总量，主机回归得到背景 1 次、总上传 2,883,584 字节，下降 45%。这采用最近使用次序，没有引入专用背景槽；计数收益不等于设备 FPS 提升。该回归验证真实 Slint 纹理分派和缓存协议，由替身统计上传并核对命中内容，未使用真实 TDE 作业或测量设备帧率。

## 8. 输入按完整报告分派，文档里的“多点支持”不能替代协议实现

完整历史补齐后，触摸回退链可以精确定位，避免把提交标题中的解释当成已证明的因果：

| 历史改动 | 源码证据和不能重蹈的坑 |
| --- | --- |
| `e3b12b98f90b1914accc54d1664d32d4c48ee7ec` 异步输入线程 | `crates/slint-nx5-core/src/platform.rs:225–248` 创建无界 channel 的生产线程，却在第一次 `poll()->None` 时结束；同提交 `input.rs:140–164` 对空读和非 ABS/KEY 事件均返回 None。线程退出条件不等于设备断开，“零延迟”标题未成立 |
| `10f48d96ee94f2020996abffc841740cf3eb0855` 去掉主线程重复 open；`00894c30cf164066d9c0e2ed564fd81054ae5c94` 撤回线程 | 前者 `platform.rs:207–214`、后者 `:184–193` 确认初始化生命周期变化；提交说明称设备访问冲突，源码还暴露上一行的退出条件问题，不能得出 Linux evdev 一律不能多句柄或多线程的结论 |
| `09cf5228deec5765563ea18cefe8b3ba82e7661c` 按类型合并 | `platform.rs:397–431` 各保存最后一个 Pressed/Moved/Released，再固定按此顺序发出；若跨完整接触序列，会丢失边沿或改变先后，不能当作通用输入优化 |
| `cbbbe0b8e08f4c7eb58b769a83da852ec17f2527`、`da43bb6b6ad6dbca0b72328a50d0c8ce4cc6ac65` 删除额外读取 | 前者 `platform.rs:278–287`、后者 `:300–306` 分别删除绘制后和跳帧分支中的 process_input；标题把重复调用解释为延迟来源，但未提供对应时序测量，而且仍保留上述合并缺陷 |
| `3b4b1fef5ec27816c990364fd08f8775fda254b4` 统一读取与回调 | `input.rs:141–168` 不再每读一个无输出的原始事件就结束本轮读取；`platform.rs:279–287` 删除按类型重排，直接分派 callback。这才是 e229 文档所对应的实现来源 |

例如，旧同步循环的 `while let Ok(Some(...)) = poll()` 会在 SYN 处提前停止积压读取，异步版本则会结束线程；其共因是混淆“本条原始事件不产生指针事件”和“队列已空/线程应退出”。当前有界原始事件批处理避免了这一接口陷阱。

`f0e9311decc0c98db0099bc6cbed2496596b4504` 给 `crates/slint-nx5-core/src/input.rs:126–137` 加入 EVIOCGABS 当前值，并在 `platform.rs:185–189` 先设屏幕大小再 open，修复首个事件缺轴值时使用错误初始坐标的条件。

但旧 `input.rs:180–235` 在 BTN_TOUCH / tracking ID 处立即分派，EV_SYN 分支不检查 code；因此“先 BTN_TOUCH 后坐标”的包能使用上一坐标，SYN_DROPPED 也没有状态重同步。它只有一个 tracking ID，没有 slot 状态；不能将 `e229391dcdc2650e5fe83040cc1edbff695e6fc1` 的 `spec.md:958–961` 中“多点触摸”“一帧只保留最终坐标”理解为完整 slot 协议或跨 SYN_REPORT 合并。`c4247f65455e7e00d6c9bac19b1d6a42e823bab9` 提交说明还明确承认中点映射 511/512 测试失败；不能把失败测试的预期值当成新项目应遵循的协议。

当前 [Decoder](../src/input.rs) 只在 SYN_REPORT 统一分派，抑制未变化的 Move；[SYN_DROPPED](../src/input.rs) 丢弃旧流直到边界并查询设备状态；[读取](../src/input.rs) 批量且有界，区分 WouldBlock/EINTR/断开/短记录。坐标夹到 `[0, extent-1]`，从真实面板和轴范围映射。现有 [输入回归](../src/tests/input.rs)覆盖这些行为；平台明确只支持单点 ABS_X/ABS_Y/BTN_TOUCH，缺少多点是当前范围，不是需要按旧文档补上的功能。

应保留按键/接触边沿顺序，只合并语义允许合并的移动；处理队列不能因触摸“加速模式”无限占用 UI 线程。[触摸实机记录](nx5-touch-response.md)只证明其中注明提交的实体点触，未覆盖新的多点或滑动功能。

## 9. 帧率与依赖来源必须可复现

更早的 `fa957f8cd3e10ff46afc5700bc518719c7083ed6` 改默认 1024×600 为 800×480，`crates/slint-nx5-core/src/platform.rs:101–110` 确认改动；提交声称像素减少约 37% 改善性能，但这同时改变工作负载，不能作为同画质性能收益。`10c32ef4b32d73284cc635f1c1039bf68d339d08` 同文件 `:235–244` 引入触摸每两轮跳一帧，`a1b4e7f2ad972378c90b7d3505ac6cd9833d1aaa` 加 30 轮惯性计数，`ada5d43b1591b24c73a750996a333dfce158743b` 又取消 active touch 跳帧。计数器以事件循环轮次递减，不能保证等于 500 ms；“60fps/120Hz”注释也不能替代成功提交或输入延迟测量。`3b4b1fef5ec27816c990364fd08f8775fda254b4` 最后移除了跳帧计数器。

`f0e9311decc0c98db0099bc6cbed2496596b4504` 把 `crates/slint-nx5-core/src/platform.rs:204–207` 的计时起点移到循环开头；此前 `c4247f65455e7e00d6c9bac19b1d6a42e823bab9` 的同文件 `:235–243` 在工作后才开始计时，几乎总额外 sleep 一个完整帧间隔。不过新旧版本 `:237–246` 仍在触摸/惯性期间关闭 pacing，不宜搬到当前最小渲染间隔契约中。当前行为以[事件循环说明](software-platform.md#事件循环)及 [runtime.rs](../src/runtime.rs)为准，测性能应统计成功完成提交，区分渲染、等待及输入分派。

同一 `f0e9311decc0c98db0099bc6cbed2496596b4504` 固定配对 Slint rev、Rust 1.92 和 fontconfig-dlopen；随后的 `d247651fee87b5722ae22250fa60fe2aa51f2c40` 把 `Cargo.toml:70–73` 改为 master。`README.md:19–30` 指出依赖库 patch 不会传递给消费方。本报告采集时使用同源本地 Slint，路径依赖不提供不可变快照，当时重现实验需另记外部源码提交及 dirty 状态。现行 [Cargo.toml](../Cargo.toml) 和测试工程已统一使用 Slint Git 来源，解析提交由 Cargo.lock 记录；当前获取和更新方式见 [构建依赖](software-platform.md#构建依赖)。

旧 `spec.md` 在 HEAD 仍保留 Rust 1.75/Slint 1.15 构建示例（`d247651fee87b5722ae22250fa60fe2aa51f2c40`，`:1110–1125`）以及 JPEG 未实现占位（`:560–574`），与后续代码不一致。规格描述设计意图，实际 API/ABI/能力必须回到对应清单、实现和测试。

JPEG 的额外经验来自 `adce9259ed2e0d482e18f3a50649ee427841f6a0`：`crates/slint-nx5-jpeg/src/loader.rs:220–260` 预算包括 progressive 系数、lossless 中间结果、输出与输入副本，不能只限制解码输出；`src/hardware.rs:18–80` 仅放行验证过的格式/尺寸，并序列化硬件调用。本项目目前没有独立 JPEG 硬件解码职责，这些应留作将来接入条件，不因历史存在功能就扩大当前库的范围。

## 本次验证和使用方式

2026-10-12 在旧仓库 HEAD `d247651fee87b5722ae22250fa60fe2aa51f2c40` 只读编译源码，产物写到 `/tmp`：

| 验证 | 本次结果 | 能证明什么 |
| --- | --- | --- |
| `rustc --edition=2021 --test tests/framebuffer_host.rs` | 9/9 通过 | 旧页状态、worker 和像素转换的独立主机逻辑 |
| `cc -std=gnu99 -O1 -g -fsanitize=address,undefined -I include/nx5 tests/tde_lifetime.c` | 通过 | 旧 TDE SDK 替身的等待/重置、缓存与分配清理约束 |
| 同参数编译 `tests/vgs_lifetime.c` | 通过 | JPEG SDK 替身的部分清理、有界保留及失败后恢复 |
| 格式化提交前后 C token 比较 | 相同 | `5d569544962478b0739187317b67162c533c57e4` 未改变已检查 token 序列 |
| 只读重算旧 `flip-pipeline/{sync,pipeline}-run{1,2,3}.log` | 6 轮各 211 ACK，全部交替页；均值 14.936878 / 22.601048 FPS，与旧摘要一致 | 历史统计口径可重放；不是新设备测量 |

前两项从旧 `crates/slint-nx5-core` 运行，第三项从旧 `crates/slint-nx5-jpeg` 运行；native 执行设置 `ASAN_OPTIONS=detect_leaks=0`，分配计数由测试断言。复跑命令的原始说明分别位于 `f0e9311decc0c98db0099bc6cbed2496596b4504` 的 `crates/slint-nx5-core/tests/README.md:3–27` 和 `adce9259ed2e0d482e18f3a50649ee427841f6a0` 的 `crates/slint-nx5-jpeg/README.md:97–106`。

未重跑旧 ARM 构建、全部旧 Cargo 测试或历史实机用例，也未用这份文档替代当前工程的 feature 矩阵、原生故障注入和硬件验证。本轮透明 UI 像素链路与缓存改进已实施，验证见下节；最终视频合成在接入方式确定后验证。第 5 项释放失败后禁止再次分配的保护也已补齐并通过故障替身验证。其余项目用于保留已有防线和解释哪些旧捷径不能重新引入。

## 本轮实现验证

本轮修改集中于 [Framebuffer 像素输出](../src/framebuffer.rs)、[两槽纹理缓存](../src/backends/nx5/tde/mod.rs) 和 [native 转换及 MMZ 清理](../src/backends/nx5/tde/native.c)。新增/扩展回归位于 [双页透明 UI](../src/tests/framebuffer.rs)、[缓存替身](../src/backends/nx5/tde/tests.rs)、[native 故障与像素测试](../test-app/tools/nx5/tde/test-native.c)。未改变 SDK/VO 所有权、未知 DMA 的隔离和页提交完成契约。

2026-10-12 验证结果：

- `cargo test --locked --offline --workspace --features nx5-tde`：库 50 项、应用及公开接口 7 项通过；3 项独占显示硬件测试保持 ignored，本轮未运行。
- `python3 test-app/tools/check-feature-matrix.py`：fbdev、nx5、nx5-tde 测试通过，空 feature 和互斥组合按预期拒绝。
- `cargo fmt --all --check` 与 `cargo clippy --locked --offline --workspace --features nx5-tde,slint/mcp --all-targets -- -D warnings`：通过。
- `NX5_SDK_INCLUDE=…/NX5/codec sh test-app/tools/nx5/tde/check-native.sh`：ASan/UBSan 像素穷举、长度/对齐/保护区/源不变、六种作业故障结果、两种 MMZ 释放失败、SDK 搜索路径用例通过。
- 使用 `.cargo/nx5.toml` 的 `nx5-tde,slint/mcp` ARM release 交叉构建通过；纯软件 `nx5` 的 ARM release 构建也通过。
- NX5 #233 的 CPU-only native 回归：像素穷举、NEON/标量尾部、六种作业替身结果及两种 MMZ 释放失败保护均通过，退出码 0。使用 Cortex-A7 / NEON softfp 编译，未传 `--sdk-loading`，没有加载厂商 SDK、发起 DMA、写显示页或接入视频；这是实际 ARM 指令与故障替身验证，不是真实 TDE/MMZ 故障注入。运行前核对 USB1 root、启动 ID `938c34d4-456a-4b1a-b532-678c0f92efd0`、VO 和显示所有者；测试前后均为 `2de9168`、PID 30267。本轮新显示程序未切换上线；两轮 CPU 测试的程序及日志均经部署工具校验归档后清理，当前和恢复程序保留。
- 更新时序图源码、SVG、PNG 和 HTML；语义/碰撞检查通过，并检查渲染图。

本地原始日志在 `evidence/slint-nx5-lessons/2026-10-12/`，需另行取得，不能仅通过检出仓库重放。旧版本的测试和统计仍保留在上一节，不与这些新结果混写。
