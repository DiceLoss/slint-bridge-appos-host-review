# NX5 单刷新周期翻页调查

> #233 固件的阶段调查记录，单周期路径尚未实现。现行行为见 [平台说明](software-platform.md)，原始证据为不随 Git 分发的本地档案；本文 PID 和内核地址仅描述当次观测。

本轮没有安全突破 29 FPS。已把两个周期进一步定位为：**先等刷新回调处理提交队列，再等下一次硬件锁存完成**。当前 PAN 的完成通知与驱动更新当前页接近，不存在一个可以直接删掉的整周期尾部等待。

当前匹配的 VOU/fyfb 源码和显示控制器寄存器手册不可用；用户也确认没有这些资料。因此本轮保留现有双 framebuffer 完成契约，没有修改内核、驱动、显示寄存器、时钟或正式 Rust/C 渲染代码。Slint MCP 演示已恢复运行。

## 实测判据与结果

测试使用当前 #233 固件，核对了 `fyfb.ko`、`vou.ko`、`lcm.ko` 的 SHA256，与上一轮相同。通过 Slint MCP 暂停演示，等待提交排空，再暂停它的全部三个线程。独立探针只交替 PAN 两张已有页面，不写像素、不加载 SDK、不写内核内存或寄存器。成功后恢复原页偏移，恢复原进程，再通过 MCP 恢复演示。

128 次提交全部成功，两个物理页交替，没有第三页；观测无丢失，最终偏移恢复成功。共保存 458 条状态变化记录，丢弃 6 条读数发生交叉更新的样本。前 64 次不加入准备延迟，后 64 次两轮扫描 0–15.5 ms 的准备时间；稳定统计排除前 8 次预热。

探针提交线程固定在 CPU0，采样线程在 CPU1；这些是有限时长诊断设置，没有改变正式应用的亲和性。采样会增加负载，因此同时参照此前同驱动纯 PAN 基准的约 29 FPS；本轮用于区分阶段，不把采样时间当作精确中断时刻。

| 指标 | 实测 |
| --- | ---: |
| 连续完成间隔 | 34.48239 ms |
| 纯翻页吞吐 | **29.00031 FPS** |
| 提交到首次观测到待锁存新页，零准备样本 | 平均 17.640 ms |
| 待锁存新页到首次观测到当前页切换，零准备样本 | 平均 17.169 ms |
| 新页待锁存时，旧页仍是驱动当前页 | **128 / 128** |
| 原偏移恢复 | 成功 |
| 暂停前后 MCP 截图像素比较 | 1024×600 完全一致 |

这些是用户态采样时间，不是精确硬件锁存时刻。PAN 返回到首次读到当前页变化的采样差不足 0.9 ms，远小于约 17.24 ms 的刷新周期。不能把首次观测稍晚于返回解释为驱动提前完成，也不能把缓存字段当作光学屏幕测量。

单周期目标按当前约 58 Hz 的实际刷新率设为至少 55 FPS。新数据的验收命令明确失败：

```text
FAIL: 29.000 FPS < required 55.000 FPS
```

当前屏幕实际约 58 Hz，单周期路径即使实现，驱动侧目标也应约为 58 FPS；不因此承诺复杂 UI 能达到相同吞吐。

数据：[结构化结果](../evidence/nx5-single-cycle/results.json)、[原始状态记录](../evidence/nx5-single-cycle/stages.log)、[单周期门槛结果](../evidence/nx5-single-cycle/single-cycle-gate.log)、[匹配环境](../evidence/nx5-single-cycle/environment.txt)。

## 多出来的周期在哪里

当前 `vou.ko` 的 `.text` 函数偏移及调用关系如下，地址仅对应记录的模块版本。

| 阶段 | 函数 | 作用 |
| --- | --- | --- |
| 接收 | `HAL_VO_GraphicUpdate` @ `0x172b4` | PAN 和 SDK 图形更新都汇入这里 |
| 入队 | `HIL_VO_SendSurface` @ `0x1bbbc` → `HIL_VO_UFL_UserSend` @ `0xc330` | 把新描述符放入 UserI 队列；此处没有提前执行 GUI 影子寄存器提交 |
| 等待刷新处理 | `TSKI_VO_FuncHD` @ `0x22474` → `HIL_VO_RunDHD0` / `HILI_VO_CB_VSCH` | 内核显示线程处理完成队列及刷新工作 |
| 准备新页 | `HIL_VO_UFL_SurfaceDisplay` @ `0xce48` → `HAL_VO_IRQ_GuiUpdate` @ `0x182cc` | 从队列取出新页，送入低层更新路径 |
| 锁存后处理 | `irq_shadow_done_handler` @ `0x368c` | 把待锁存描述符转为当前描述符，处理旧描述符与回调 |
| 通知 | `HIL_VO_CheckDHD0` @ `0x1ac88` → `EXT_VO_FB_BufferNotify` → `fyfb_notify` | 唤醒阻塞 PAN，之后适配层才复用旧页 |

`fyfb_fb_pan_display` 自身只有一次 `down()`；之前的 `down_trylock()` 用于消耗旧通知。没有两次显式阻塞等待可直接减少为一次。

当前状态变化为：

```text
提交 B              第一次刷新附近                 下一次刷新
UserI = B     →     影子地址 = B、update_rdy = 1  →  update_rdy 清零
当前页 = A           待锁存 = B、当前页仍为 A         当前页 = B、旧 A 可释放
                     队列已取空                     PAN 完成通知
```

本轮同步读取了 DHD 的更新位、地址寄存器，以及驱动的当前/待锁存描述符和 UserI 队列计数。128 次都看到“地址寄存器已经是 B，但驱动当前页仍是 A”的中间状态。这直接说明为什么不能用地址读回、新页已入队或第一次中断作为旧页可写的依据。

反汇编摘录见 [driver-excerpts.txt](../evidence/nx5-single-cycle/driver-excerpts.txt)。

## 已排除的用户态捷径

- **SDK 零超时提交**：`FY_MPI_VO_UpdateGraphicLayer` 仍经过 `HLD_VO_GraphicsUpdate` → `HAL_VO_GraphicUpdate` → 同一队列。更早返回只代表没有继续等，不等于第一周期已经安全释放旧页。
- **改变 PAN 的 activate 标志**：当前 fyfb 回调没有按这些标志选择不同完成路径。
- **改为等待一次 VSYNC**：当前 fyfb 自定义 ioctl 回调不提供有效的 VSYNC 等待语义，返回成功不足以证明等待或页退休。
- **缩小队列深度**：本次实际队列中最多只有一个新页；额外等待来自处理阶段，而非积压了多帧。
- **并发发起更多提交或提前改写旧页**：会改变现有单次提交及两页所有权约束，不能作为满足本任务条件的提速结果。

已检查的 SDK 图形层 API 只有更新、获取/释放帧及图层配置等入口，没有一个经核实、可从用户态提前准备影子地址并返回可靠完成凭证的公开接口。

## 真正单周期所需的驱动改动

可行的设计方向是将图形页的准备工作提前到第一次刷新之前，而保留真实锁存后的完成通知。下列内容是待实现和验证的驱动设计，不是已可部署的补丁：

1. 在 NX5 VOU 的图形提交路径接收新页后，立即安排只处理 GUI 提交的工作，把页描述符和影子寄存器准备好，避免等下一次刷新回调才开始取队列。
2. 保留 UFL 描述符引用、队列状态、锁和单次在途提交规则。不能从用户态直接调用内部函数，也不能简单额外调用整个 `HIL_VO_RunDHD0`；后者还包含刷新计数、视频通道等周期性工作。
3. 硬件锁存完成后再切换当前页、释放旧页，并发出与该请求对应的完成通知。失败、丢中断或结果未知时不得返回可复用成功。
4. 核实写入截止时间及影子寄存器的锁存语义。错过当前边界时允许落到下一周期，不能为了报出高 FPS 而提前确认。

这需要匹配源码或等价的可审查驱动实现，以及硬件锁存/旧页不再被读取的可靠定义。本轮缺少这些条件，不能用未经验证的机器码热补丁或 MMIO 旁路代替。

边界应保持：

- **通用层**：仍管理 retained 画布、两个 framebuffer 页、脏区、后台提交与完成通知；不加入 NX5 寄存器、IRQ 或周期计数。
- **NX5 适配层**：验证新驱动版本并提供 `PresentationContract::complete`。可以继续沿用阻塞 PAN ABI，但仍需验证新驱动的完成语义。原调查提出的固定摘要准入已于 2026-10-12 从源码移除，现行要求见 [平台说明](software-platform.md#准入边界)。
- **驱动层**：负责提前准备影子状态、实际锁存、旧页退休和错误传播。

验收需要同时证明：纯翻页接近一个刷新周期；两页不同内容及快速复用不破坏旧帧；实际输出无混色；错误与退出路径不错误释放页面；整屏渲染、TDE 和软件回退仍正确。现有约 29 FPS 的 WBC 有限采样不能单独认证未来约 58 FPS 的每个物理输出帧。

## 观测方式和排除的数据

探针是固件专用诊断代码，使用只读 ARM 页表查询定位内核数据，通过 `/dev/kmem` 的缓存线性映射读取当前驱动状态。MMIO 仅只读；不把这些私有字段纳入生产完成契约。

最初通过 `/dev/mem` 的非缓存别名读取内核普通 RAM，出现与已完成 PAN 不一致的旧字段。该版本的数据已标为 [stages-uncached-invalid.log](../evidence/nx5-single-cycle/stages-uncached-invalid.log)，不用于结论。改为内核缓存视图后，状态转移与更新位、PAN 返回相互一致。内核虚拟内存读接口的参考实现见 [Linux v4.9 `drivers/char/mem.c`](https://github.com/torvalds/linux/blob/v4.9/drivers/char/mem.c)；具体字段偏移来自本机匹配的 VOU 二进制。

检查器要求缓存读模式，拒绝缺失提交、没有恢复原页、出现第三页或非缓存旧数据；[负向重放结果](../evidence/nx5-single-cycle/negative-replay.json) 保留了这些拒绝。

## 复跑

```sh
sh test-app/tools/nx5/single-cycle/build.sh
python3 test-app/tools/nx5/single-cycle/analyze.py evidence/nx5-single-cycle/stages.log
# 当前应退出 1：尚未达到单周期目标。
python3 test-app/tools/nx5/single-cycle/analyze.py evidence/nx5-single-cycle/stages.log --min-fps 55
```

实机操作仍通过 DPower Terminal 的 USB1 root 会话：先由 MCP 暂停当前 UI，确认提交排空，再 STOP 当前进程；探针要求 `NX5_STOPPED_OWNER=<实际 PID>`，并检查该进程所有线程处于停止态。只有探针成功且 `restored=1` 才恢复进程，然后通过 MCP 恢复 UI。不得沿用其他启动周期的 PID 或内核地址。

本轮保持同一个 UI 进程 PID 1338，没有重启设备或重新加载 SDK；两轮诊断后 MCP 截图均与各自暂停前逐像素一致。最终运行状态见 [环境记录](../test-app/devices/nx5/current-target.json)。
