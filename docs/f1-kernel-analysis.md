# F1 当前内核镜像的显示与 GE 完成路径

调查日期：2026-10-19。对象是设备 `fd_f1_r88r30_spinor-20c31b4578`
先前只读采集的 FIT / Image，Linux 5.10.44 #1，构建于
2026-05-22 02:04:02 UTC。**本报告是离线二进制分析，不是翻页实测**。
未访问设备、停止原应用、发起 PAN / VSYNC、映射物理显示页或写显示寄存器。
原始采集范围和本机布局见 [显示调查](f1-display.md)。
同轮 [离屏 G2D 正向实测](f1-display.md#f1-offscreen-ge) 单独记录成功作业和
缓存交替访问结果，不与本报告的静态分析或未测试故障混为一谈。

## 结论与证据等级

当前镜像已经能直接追踪到 `FBIOPAN_DISPLAY → fb_ops → DE 更新` 和
`WAITFORVSYNC → DE 等待 → IRQ`，不再仅凭官方参考源码推测这两条路径。
**普通物理页 PAN 在提交寄存器后直接返回；VSYNC 等待的是共享布尔标志，
没有目标页或提交序号；所定位的 DE IRQ 没有处理 CFG_DONE bit 3。**
现有接口的这条调用链仍不能提供本库要求的旧页退休确认，F1 显示准入应保持关闭。

本报告使用三种口径：

- **二进制核验**：当前 Image 的字节、指针、指令、分支及数据流可直接重放。
  文中的完整地址均是该 Image 的虚拟地址；简写地址统一省略前缀 `0xffffffff`。
- **结构解释**：以 Linux / ArtInChip 参考源码和寄存器手册给机器代码命名，
  不等于恢复了原始符号表、编译配置或厂商补丁。函数名均为分析名称。
- **未知**：硬件 shadow 生效和旧页最后一次总线读取的关系、现场错误恢复，
  以及未审计调用路径的行为。本轮不以等待多次中断或固定延时填补这些缺口。

## 输入与可重放方法

| 输入 | SHA256 |
| --- | --- |
| `evidence/f1-display-20261019/kernel.mtd` | `28573f4b908e29ef7cecc8b0eea7dec76f0fcd86138eb9034e3ee9d397d2820d` |
| `evidence/f1-display-20261019/unpacked/Image` | `754e2d6f05664522b16c6d60f5f6dc48d3e2eee1bf788938cf994a6823c86573` |

FIT 的 kernel 节点声明 `arch=riscv`、`compression=lzma`，load / entry
均为物理地址 `0x40000000`。Image 长度为 5,848,932 字节；头部 text_offset
是 `0x200000`，image_size 是 `0x5eb230`。不能据 text_offset 直接把
Image 反汇编基址设为 `0xffffffff80200000`，也不能把 FIT 的物理 load
地址当作所有绝对代码指针的虚拟地址。

本镜像的虚拟基址确认为 `0xffffffff80000000`：头部代码包含这一虚拟常数；
以此基址解码，运行日志的 DE / RGB / panel component ops 地址分别落到
文件偏移 `0x42fb00` / `0x42f388` / `0x42fd48`，内含有效 bind/unbind
代码指针，且各代码中的绝对字符串交叉引用与实际字节相符。以
`0xffffffff80200000` 为基址时这些位置是指令数据，不是对应指针表。
本报告未恢复完整 kallsyms / System.map，未将只存在于错误消息中的函数名
当作全量符号。

从仓库根运行 [离线重放脚本](../test-app/tools/f1/analyze-kernel.py)：

```sh
python3 test-app/tools/f1/analyze-kernel.py
```

脚本先核对两个固定 SHA256，拒绝对其他镜像套用地址；使用
`/opt/F1/bin/riscv64-unknown-linux-gnu-objdump`（GNU Binutils 2.32）
按 RV64 / compressed 指令解码，输出 `metadata.json` 及六段完整反汇编：
`head.disasm`、`fb-core.disasm`、`fb-driver.disasm`、`de-hardware.disasm`、
`de-driver.disasm`、`ge-normal.disasm`。输出默认位于
`evidence/f1-kernel-20261019/`，被 Git 忽略；复核者须先取得上述原始档案。
这只是固定镜像调查脚本，不是后端准入或现场固件完整性检查。

## 从 component ops 恢复 DE 表

运行日志中的 `bound 18a00000.de (ops 0xffffffff8042fb00)` 指向
**component framework 的 bind/unbind 表**，不是 `de_funcs`。
该位置两个 64 位值分别为 `8017c67e` 和 `8017bd3c`。
`8017c67e` 分配 1200 字节对象，将其保存至全局 `80594e10`，然后
在 `8017c860–8017c94e` 写入函数指针；这是可直接追踪的实际 DE 表。

| 对象内偏移 | 指针值 | 识别依据 |
| ---: | --- | --- |
| +24 | `8017c48c` | 设置时序、使能 timing IRQ；错误消息引用 `aic_de_timing_enable` |
| +40 | `8017c36e` | VSYNC ioctl 包装函数从该偏移取指针；清/等共享标志 |
| +72 | `8017cd60` | PAN 从该偏移读取 layer；函数从对象 +304 的数组复制 92 字节 |
| +80 | `8017d83e` | PAN / update-layer ioctl 调用；配置更新 0→1 包围单层更新 |
| +88 | `8017d796` | 多层列表更新；同样以配置更新 0→1 包围循环 |

在 `8017c7d2–8017c7e6`，bind 将 `8017bf74` 放入 IRQ handler 参数
并注册；上下文是同一 DE 对象。handler 的错误分支引用
`DE underflow error (irq status: %#x)`。这些证据把操作表、共享状态和
硬件中断连到同一个组件，而非靠相邻代码地址猜函数身份。

## PAN 的实际调用链

`fb-core.disasm` 中 `80172fe2` 的 ioctl 分派计算 `cmd - 0x4600`，
读取 `8042d788` 的 32 位跳转表。表索引 6（`FBIOPAN_DISPLAY=0x4606`）
为 `801731a2`：复制 160 字节参数后，在 `801731ca` 调用 `80172cde`。
后者检查步长和虚拟尺寸，在 `80172d24–80172d42` 从 `info+656` 取
fb_ops，再从表 +80 取 PAN 函数。

`fb-driver.disasm` 中 `8017aa4e–8017aa56` 将 `805694e0` 装入
`info+656`。该表 +80 的值为 `80179eb4`，因而 PAN 身份不只由
`GE Bitblt FB error` 字符串推断。

`80179eb4–80179fde` 的二进制行为如下：

1. 从 `info+728` 取板级对象，从对象 +144 取 DE 表；在 `80179ef6`
   读取 DE+72 并调用，取得 layer 软件缓存。
2. 若 `var+144`（reserved[0]）非零，设置 DMA-BUF 标志 512，调用
   DE+80 并保留返回值。这不是本次普通物理页路径。
3. 普通路径在 `80179f38–80179f5c` 计算
   `xoffset*bpp/8 + yoffset*ALIGN_8B(xres*bpp/8)`，加 framebuffer
   起始地址作为图层地址。当前二进制还把板级对象 +40 的 flags 放入 layer，
   与参考源码直接写 0 的表达不同，不能宣称两份源码完全相同。
4. rotate / HSBC 条件另走 GE 辅助路径；普通显示路径到达 `80179fba`，
   调用 DE+80 后，在 `80179fc2` **强制 `a0=0`** 并返回。
   因此这个普通分支既不等待显示完成，也丢弃该更新函数的错误值。
5. fb core 在 `80172d46–80172d4c` 更新 `info` 中的软件 x/y offset
   后返回；它没有追加 VSYNC 或针对页地址的等待。yoffset 读回不能证明扫描页。

DE 的更新函数 `8017d83e–8017d878` 明确执行：

```text
DE regs+8 = 0                 @ 8017d856 → 8017b768
update_one_layer(comp, layer) @ 8017d85e → 8017cea6
DE regs+8 = 1                 @ 8017d86a → 8017b768
return update_one_layer 的返回值
```

`8017b768–8017b770` 只有 `fence w,o`、地址加 8 和 `sw`，无读取、
轮询或等待。`8017cea6` 的 UI 物理地址分支在 `8017d178` 取地址，
通过 `8017d272 → 8017babe` 写 UI 配置，在 `8017d29e` 复制缓存后返回。
`8017babe–8017bb2c` 将大小、位置、stride、地址写到每个 rectangle 的
`0xc0 + rect*16` 至 `0xcc + rect*16`，同样没有完成轮询。
寄存器 +8 被解释为 `DE_CONFIG_UPDATE`，依据下文列出的官方手册。
这一写路径和 `get_layer` 的内存复制也直接说明：layer 查询是软件缓存，
不是 shadow 扫描地址读取。

## WAITFORVSYNC、IRQ 与 CFG_DONE

`metadata.json` 从 `80569180` 起解码 48 字节的 ioctl 项，首项是：

```text
cmd=0x40044620, handler=80179602, description="Wait for vsync"
```

`8017a63a–8017a688` 分派器逐项完整比较 command 并跳转 handler；
`80179602` 仅从板级对象 +144 取 DE 表，再跳到表 +40。
它没有读取用户参数。bind 已确认该槽为 `8017c36e`。

`8017c36e–8017c41c` 禁止本地中断，清零 DE 对象 +252 的 32 位标志，
恢复中断，再等待同一标志非零。代码中等待预算为 **10 个 tick**；
按参考源码它对应 `msecs_to_jiffies(100)`，但本报告不以参考配置独立证明
当前内核 HZ。等待结果为 0 时返回 `-110`，负值直接传递，正值或标志
已置位时返回 0；按参考等待原语解释，负值分支包含信号中断。
没有读取目标物理页、配置更新寄存器、配置完成状态或提交计数。

IRQ `8017bf74–8017bfe0` 的全部功能是：

- 调用 `8017bc58` 读取 DE+`0x1d8`，调用 `8017bc66` 将原始 status
  写回同一寄存器清状态。
- `status & 1` 为真时，将对象 +252 置 1，唤醒对象 +256 的等待队列。
- `status & 4` 为真时记录 underflow 错误，随后返回 IRQ_HANDLED。

所检查的 handler 没有 `status & 8` 分支。`8017c5d6–8017c5ea`
时序初始化只请求使能 mask 1 和 mask 4；其 helper `8017bc46` 操作
DE+`0x1d4`。官方手册将这些位分别命名为 VBLANK、UNDERFLOW、CFG_DONE。
因此**当前这条驱动完成路径没有使用 CFG_DONE**；这里没有宣称整份镜像中
其他模块绝不读写这个寄存器，也没有假定启动固件留下的 IRQ enable 值。

同一个共享标志可被后续等待者再次清零；返回值只表明等待过程中处理过
VBLANK。PAN 和这个等待之间没有本次提交的身份传递。进一步推论是：
在 IRQ 延迟处理、配置 latch 边界或其他显示配置更新存在时，不能单凭两次
ioctl 的调用顺序证明目标页已生效且旧页已退休。本轮不声称已经实测到撕裂。

## GE normal 填充的补充核验

`ge-normal.disasm` 中 `80177af4` 的分派器在全局对象 +48 加锁，
明确比较 `0x40744702`（当前 ABI 的 116 字节 FILLRECT），分支到
`80177bac`。该分支完成参数复制和绘图配置，在 `80177e3c` 调用
`801769e6`，把返回值保存在 s0，释放 DMA 映射及锁后于 `80178144`
恢复至 a0 返回，未把完成错误强制改为成功。

`801769e6–80176abc` 同时交叉引用当前 Image 中 `ge timeout` 和
`ge error status:%08x`，行为与参考 `ge_start_and_wait` 对应：

- 清对象 +96 的 status；写 GE 中断使能，再启动 GE。
- 等待 +96 非零，预算为 100 个 tick，随后关闭 GE 中断。
- status 为 0 时打印 timeout，调用 reset assert/deassert 路径，返回 `-110`。
- status 非零但 bit 0 未置位时返回 `-22`；bit 0 置位则返回 0。

IRQ `80176ad6` 从 GE+4 读取并清硬件状态，保存到 +96，然后唤醒
+80 的等待队列；probe 在 `80176c72` 将它注册为 IRQ handler。
这补足了当前镜像中“normal fill ioctl 包含等待且传递结果”的软件证据。
**它仅要求 finish bit 0，并不额外拒绝同时出现的其他错误位**；不能把该判断
描述为检查了全部硬件错误，也不能从 reset 调用推出所有未知 DMA 都已安全退休。
本报告不包含硬件错误注入、故障后重用或缓存同步实测，离屏成功测试应另按其
具体缓冲区、尺寸、轮数及缓存阶段记录。

## 参考资料与仍需补足的范围

结构命名依据为官方 [Luban 固定提交 `13fa0212`](https://github.com/artinchip/luban/tree/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/drivers/video/artinchip)：
`disp/aic_fb.c`、`disp/aic_fb_ioctl.c`、`disp/aic_de.c`、`disp/hw/de_hw.c`，
以及 `ge/ge_normal.c`。本地副本分别在 `evidence/f1-display-20261019/source/`
和 `evidence/f1-implementation-20261019/`。这些是第一方参考资料，
**并非当前固件的源代码证明**；例如本镜像 ioctl 表在 TO_DMABUF_FD 后即
结束，未包含参考版本后面的 DMA coherent 查询/更新项。

寄存器解释依据为已归档的 [D21x 用户手册](https://aicdoc.artinchip.com/out/downloads/D21x_User_Manual.pdf)
Version 2.0.4，SHA256 `ac3b4eb899a0233325a919a340a8a44cbde7964e78a2fb8109da67c96419f213`：
10.1.3.1 p.311 的 preload/shadow，10.1.5.3 p.321 的 CONFIG_UPDATE，
10.1.5.63–64 p.347–348 的 timing interrupt 和 CFG_DONE。
PDF 的在线内容可更新，复核时应核对归档摘要。

仍需建立针对该硬件/固件的“本次配置生效且不再读取旧页”契约，以及超时、
underflow 和配置并发下停止写两页的恢复策略。当前镜像未发现这条已审计
接口链能提供该结果；本轮也不证明只能修改内核才能实现它。
准入之外的离屏 GE 和缓存验证可继续推进，显示接管及物理扫描验证须另行开展。
