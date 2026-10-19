# F1 指令集与低内存渲染优化调查

日期：2026-10-19。对象：ArtInChip D211 F1，序列号
`fd_f1_r88r30_spinor-20c31b4578`。本报告区分芯片文档、编译器生成结果、
本机观测与待测优化；编译成功不等于硬件支持，指令减少也不等于整帧加速。

## 结论

F1 应按 **RV64GC + 玄铁标量扩展** 优化，不能直接套用 NEON 或标准 RVV。
D21x 手册将 CPU 标为 C906 / RV64IMAFDC，系统框图标为 C906FD；
本机 `/proc/cpuinfo` 报告 `rv64gcxthead`，未报告 `v`。
本轮纯 CPU 实机探针已确认 `revw` / `srriw` 可执行；像素转换微基准中，
64 位 SWAR 加四像素展开比该次 GCC 标量对照耗时低约 53%。这不是整帧
渲染收益。实现优先评估不依赖厂商指令的 Rust SWAR，避免全局开启 ISA
feature；成对访存仍只是候选。这些方案都不要求新增整帧缓存。
硬件图片解码另行调查，不以 CPU 指令结论推断 VE 的能力。

## 核心型号与 SIMD 边界

| 证据 | 内容与边界 |
| --- | --- |
| D21x 用户手册 v2.0.4，p.18、p.43、p.47 | C906，RV64IMAFDC，600 MHz；框图写 C906FD。未声明标准 V、旧版向量或 P 扩展。来源：[ArtInChip 手册](https://aicdoc.artinchip.com/out/downloads/D21x_User_Manual.pdf)。 |
| C906 核心手册，§2.2.3 p.5 | VPU 是配置选项，配置后才有向量整型/浮点执行能力；因此不能从“C906”名称推出一定有向量。来源：[玄铁官方 OpenC906 手册，固定提交](https://github.com/XUANTIE-RV/openc906/blob/b0c06eb1f8b3bae663bd8b87eac89ff48e68a57f/doc/%E7%8E%84%E9%93%81C906%E7%94%A8%E6%88%B7%E6%89%8B%E5%86%8C(openc906)_20240627.pdf)。 |
| 本机只读观测 | Linux 5.10.44，boot ID `dac34b8d-d45f-4fb9-a5fc-ba8902eda615`；单 hart，`rv64gcxthead`，600 MHz，I/D cache 各 32 KB、cache line 64 B，MemTotal 57,952 KiB，无 swap。这里是系统报告值，不是独立测量缓存容量。 |

本机快照为 `evidence/f1-render-isa/2026-10-19/resident-before.txt`，属于
本地忽略档案，检出仓库后须另行取得；采集时 MemAvailable 为 11,396 KiB，
是当时运行负载下的值，不能作为后续程序固定可用预算。

手册内部存在缓存规格冲突：D21x p.43 写 D-cache 16 KB，p.18 与 p.47
写 32 KB；本机报告为 32 KB。本报告不把该冲突隐藏为已核实硬件参数。
原始 D21x PDF 位于忽略目录 `evidence/f1-display-20261019/source/`，
SHA256 为 `ac3b4eb899a0233325a919a340a8a44cbde7964e78a2fb8109da67c96419f213`；
取得该档案或下载对应版本后才能复核，在线 URL 可能更新。

**RVV 0.7.1 与 RVV 1.0 不可混用。** 玄铁官方 `XTheadVector` 规范基于
旧 V 0.7.1，列出 C906V 等向量型号；它与标准 V 使用重叠但不兼容的编码，
`vsetvl`、载入/存储、掩码及状态位等有差异。规范中的 `XTheadVector 1.0`
是厂商扩展版本号，不是标准 RVV 1.0。不能为本 F1 直接添加 Rust `+v`、
`+zve*`，或复制其他 C906V 平台的向量代码。
来源：[玄铁 XTheadVector 规范](https://github.com/XUANTIE-RV/thead-extension-spec/blob/5a75db2696881a812a04e6e00347d3920dae03f2/xtheadvector.adoc)。

本次未发现 D211 的一手资料声明 P/packed-SIMD 扩展。`xthead` 是扩展集合的
旧名称，不等于任意玄铁指令全部可用；`XTheadMac` 的乘累加、`XTheadBb`
的位操作也不等同 NEON 多通道 SIMD。D21x 外设名 `DSPK` 是音频输出，不能
作为 CPU 带 DSP 指令的证据。扩展分类见
[玄铁 ISA 总览](https://github.com/XUANTIE-RV/thead-extension-spec/blob/5a75db2696881a812a04e6e00347d3920dae03f2/intro.adoc)。

## 可直接比较的优化候选

下表说明候选机制；已完成的纯 CPU 测量单列于下一节，不能外推整机收益。

| 候选 | 作用 | 条件与验证 |
| --- | --- | --- |
| 64 位 SWAR | 一次用整数移位、掩码处理两个 32 位像素，减少循环与访存指令 | 使用普通 RV64I，不依赖 V；核对对齐、长度、行尾、别名及透明通道，保留尾部处理。不要把普通字节数组强制转成未对齐的 `u64` 引用。 |
| `revw` + `srriw` | 低 32 位字节倒序 + 循环右移，可实现 RGBA/BGRA 红蓝交换 | 数值 `0xAABBGGRR` 先右旋 24 位再 `revw` 得 `0xAARRGGBB`；单独 `revw` 会同时换掉 alpha/green，不能替代红蓝交换。[REVW](https://github.com/XUANTIE-RV/thead-extension-spec/blob/5a75db2696881a812a04e6e00347d3920dae03f2/xtheadbb/revw.adoc)、[SRRIW](https://github.com/XUANTIE-RV/thead-extension-spec/blob/5a75db2696881a812a04e6e00347d3920dae03f2/xtheadbb/srriw.adoc)。 |
| `extu` / `addsl` | 位域提取、移位加法可缩短通道处理和地址计算 | 由编译器选择通常比手写逐像素 FFI 更简单；调用边界应覆盖整行或一段像素。[XTheadBb](https://github.com/XUANTIE-RV/thead-extension-spec/blob/5a75db2696881a812a04e6e00347d3920dae03f2/xtheadbb.adoc)、[XTheadBa](https://github.com/XUANTIE-RV/thead-extension-spec/blob/5a75db2696881a812a04e6e00347d3920dae03f2/xtheadba.adoc)。 |
| `ldd` / `sdd` | 一条指令向两个普通寄存器载入/存储，共 16 字节 | 它不是 128 位 SIMD，也不保证一次总线事务或原子性；不能越过实际缓冲边界。只对本进程已拥有的内存测量。[LDD 语义](https://github.com/XUANTIE-RV/thead-extension-spec/blob/5a75db2696881a812a04e6e00347d3920dae03f2/xtheadmempair/ldd.adoc)、[成对访存](https://github.com/XUANTIE-RV/thead-extension-spec/blob/5a75db2696881a812a04e6e00347d3920dae03f2/xtheadmempair.adoc)。 |

SWAR 和小规模展开应在普通缓存内存、实际写页映射分别测；1024×600×4
的单帧为 2,457,600 字节，远大于 L1 cache，不能只用热缓存小数组推导整屏
收益。减少一次整帧中间复制即可省去同等大小的额外读写；但不能为省内存
破坏物理双页保护、既有 DMA 所有权或完成契约。

## 纯 CPU 实机指令探针与像素微基准

同一 F1 上，先让有 180 秒退出期限的显示应用正常结束，再运行独立探针。
探针禁用 core dump、隔离 SIGILL，输出 `THEAD_REVW_SRRIW_OK`，确认
`revw` / `srriw` 在当前用户态可执行且组合结果正确。没有测试 V 指令，
也没有测试 `ldd` / `sdd`；本次生成清单中后二者出现次数均为 0。

输入为伪随机 32 位像素，长度 1..129 和 614,400 的输出逐字节对照标量
结果通过。每组时间为五轮的中位数，表中单位为每次转换的毫秒：

| 实现 | 1,024 像素 | 65,536 像素 | 614,400 像素 |
| --- | ---: | ---: | ---: |
| GCC 标量 | 0.018959 | 1.274286 | 11.966433 |
| `revw` + `srriw` | 0.012074 | 0.835106 | 7.839833 |
| 64 位 SWAR | 0.010386 | 0.725126 | 6.747341 |
| 64 位 SWAR，四像素展开 | 0.008697 | 0.598171 | 5.629475 |

证据是 `evidence/f1-render-isa/2026-10-19/pixel-bench.log` 和
`pixel-bench-codegen.json`，均为本地忽略档案，复核须另行取得。表格只对应
该次日志中的四个变体，不包含随后新增、尚未计入本表的对齐变体。

这是 **C 微基准、普通 malloc 缓存内存**：三个独立缓冲合计约 7.03 MiB，
分别用于输入、输出和正确性对照，不是建议加入正式应用的缓存。没有写
framebuffer、操作 DMA 或测量扫描输出；也未据此测得应用 RSS 改善。结果支持
优先实现和验证 Rust SWAR，但还须以实际写页路径及整机时间/内存对照决定
是否采用，不能将 5.629 ms 称作 F1 的整帧渲染时间。

## 工具链实际检查

本节主机检查读取 `/opt/F1` 工具链并编译了仅含字节倒序/通道交换的微型
文件，未修改项目构建配置。临时文件放在 `/tmp`，不归档固件或
反汇编。可用以下命令复核工具链属性：

```sh
/opt/F1/bin/riscv64-unknown-linux-gnu-gcc -v
/opt/F1/bin/riscv64-unknown-linux-gnu-gcc -Q --help=target -mcpu=c906
rustc -vV
rustc --print target-cpus --target riscv64gc-unknown-linux-gnu
rustc --print target-features --target riscv64gc-unknown-linux-gnu
```

| 工具 | 本机结果 | 实施含义 |
| --- | --- | --- |
| T-HEAD GCC 10.2.0，V2.0.1 B20210512 | 配置默认 `rv64gcxthead` / `lp64d`；`--help=target` 输出 `rv64imafdc_xtheadc`；`-mcpu=c906` 可接受；`-mrev/-mrevw/-msrriw/-mext/-mldd` 默认开启 | 无须仅因缺 NEON 就换整套工具链。先确认候选循环已有何种生成结果。 |
| 同一 GCC，`-O3 -mcpu=c906` 微型函数 | `__builtin_bswap32/64` 生成 `revw/rev`；普通红蓝掩码交换生成 `extu` | 这些是主机代码生成验证；硬件是否执行、速度多少须分别实测。 |
| Rust 1.98.1，LLVM 22.1.8 | `target-cpus` 无 C906；`target-features` 有 `xtheadbb` 等 LLVM codegen 项，没有 `xtheadvector` | 不使用猜测的 `target-cpu=c906`，不能把宿主机 `native` 配置用于交叉目标。 |
| Rust `-C target-feature=+xtheadbb` 微型函数 | LLVM 可生成 `th.revw/th.rev`，但 rustc 明确警告该 feature 未知且不稳定，透传给 backend | 不据此全局开启一组未验证扩展。若收益明确，可用厂商 GCC 编译局部 C/汇编实现，并经固定 ABI 接入。 |

现代命名的 `th.*` 与旧 GCC 的无前缀 mnemonic 不同，不能把新 LLVM 输出的
文本汇编直接交给旧汇编器并假定兼容。`-mvlen=128`、向量 multilib 目录存在
或工具链能接受向量参数，都只说明工具链可服务其他配置，不证明 F1 有 VPU。
项目当前 ABI/解释器保持 [F1 构建说明](../test-app/devices/f1/README.md#构建)
中的配套关系；不因微型基准更换全程序 ABI。LLVM 对玄铁标量扩展的定义可
核对 [LLVM 22.1.8 源码](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.8/llvm/lib/Target/RISCV/RISCVFeatures.td)。

## Linux 上的探测与验证边界

官方 Luban 固定提交 `13fa021285a307ca345a6b05e0da371b635f18eb` 的
[`Kconfig`](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/arch/riscv/Kconfig#L365)
有 `CONFIG_VECTOR`，默认关闭；所查 D211 demo88 NAND
[defconfig](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/arch/riscv/configs/d211_demo88_nand_defconfig)
未启用它。[`switch_to.h`](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/arch/riscv/include/asm/switch_to.h#L68)
仅在配置启用并且 `has_vector` 为真时保存/恢复向量状态。
这些是参考源码，不能单凭同为 Linux 5.10.44 就视为当前固件配置。
本机 `device-owners-config.txt` 的查询返回 `/proc/config.gz` 不存在，
因此本轮没有拿到当前固件的 `CONFIG_VECTOR` 配置。

参考 [`cpufeature.c`](https://github.com/artinchip/luban/blob/13fa021285a307ca345a6b05e0da371b635f18eb/source/linux-5.10/arch/riscv/kernel/cpufeature.c#L66)
从设备树 ISA 字符串构建 HWCAP，并用 V 位决定 `has_vector`；因此应把
`cpuinfo`/`AT_HWCAP` 当作软件公布能力，再结合手册和隔离指令探针确认。
现代 [`riscv_hwprobe`](https://docs.kernel.org/6.14/arch/riscv/hwprobe.html)
有独立 `XTheadVector` 项，但不能假定这个旧厂商内核已实现新接口。

候选标量指令可在独立短生命周期进程中以已知输入输出核验，捕获/隔离
SIGILL，关闭 core dump，不访问显示设备。`th.sxstatus`、`misa` 和机器标识
CSR 需要相应特权；Linux root 进程仍运行于 U-mode，不能通过读写它们
“打开优化”。特别是 `th.sxstatus` 的 U-mode 访问会产生非法指令，见
[官方 CSR 权限](https://github.com/XUANTIE-RV/thead-extension-spec/blob/5a75db2696881a812a04e6e00347d3920dae03f2/xtheadsxstatus.adoc)。

向量试探指令若 SIGILL，只能证明当前环境不可执行；若成功，也不能单独
证明内核在进程切换、信号和异常路径上正确保存向量上下文。因此本轮不能
用改 CSR、改设备树字符串或全局 `+v` 代替平台支持验证。缓存管理指令也不能
替代既有 DMA 同步 ioctl。最终应同时报告像素一致性、写页/渲染时间分布、
RSS/PSS 与 MemAvailable，只有实际热点收益且内存不增加才纳入实现。

本轮已采用普通 RV64 的四像素批处理写页，未启用新的全局 ISA feature；
整机前后对照与内存结果见 [F1 实测](f1-optimization.md#f1-scalar-png)。
