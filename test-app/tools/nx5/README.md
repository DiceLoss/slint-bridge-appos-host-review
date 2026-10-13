# NX5 设备工具

这些工具验证 Slint MCP 不覆盖的硬件路径。目录内的 `build.sh`、`build-observer.sh` 和 `check-native.sh` 会定位仓库根；诊断输出继续放在根 `target/nx5-*`，历史证据继续从根 `evidence/nx5-*` 读取。

`target/` 和 `evidence/` 均不纳入 Git。下方证据重放命令需要已有的本地档案；它们不采集新数据，也不证明当前二进制已通过实机测试。当前进程内 WBC 插件的构建、动态库搜索路径和判读命令见 [NX5 设备说明](../../devices/nx5/README.md)。`present/` 中的独立 SDK 探针属于 #22 旧环境，不能直接套用于正在运行 UI 的 #233 设备。

## 部署、归档清理和截图

主机入口为 [deploy.py](deploy.py)，使用 Python 3.11+ 和 ADB；[deploy-device.sh](deploy-device.sh) 在已有 root 终端中执行清单采集与清理，[deploy-control.sh](deploy-control.sh) 共用事务目录检查与 `flock` 锁。设备需提供 `flock`，当前 #233 固件已验证可用。当前板 ADB 为 UID 2000，脚本沿用 ADB 传输、root 串口执行的方式，不修改设备权限或认证配置。每次显式指定 ADB 目标，并重新核对设备身份及显示所有者。已有 UI 可以保持运行；一轮部署完成后再启动新的测试。

[deploy-retain.json](../../devices/nx5/deploy-retain.json) 是维护中的保留策略，和观测快照 `current-target.json` 分开：`current_binary` 与 `recovery_binary` 必须是两个存在的普通文件，`keep` 列出启动脚本、字体、库目录、许可证及其他需要保留的文件或目录。新版本通过验证并切换后，把原当前程序设为恢复版本，再更新当前路径及依赖；不能只修改 PID 或根据文件时间猜测恢复版本。首次接入或重启后若这些文件不存在，需要先恢复已验证的部署及恢复文件；此维护流程会拒绝在缺少恢复程序时清理。

脚本仅管理 `/run/slint-bridge-*/`、`/tmp/slint-bridge-*/` 下的普通文件。保留策略、活动进程的可执行文件/映射/文件描述符、`SLINT_DEFAULT_FONT`、`LD_LIBRARY_PATH` 及 `.lock` 文件均受保护。保留目录中的符号链接会递归解析，目录外的依赖目标也受保护；清理前重新解析，归档后被改指的目标同样不能删除。目录项和符号链接不作为删除候选，不读取设备节点内容。保留目录的全部内容均会保留，因此测试日志和探针应放在单独的测试目录，不要放入被整体保留的 SDK 目录。

从仓库根执行以下流程。`--work` 使用 `inspect` 本次输出的真实目录，`--archive` 每轮必须是新的主机目录：

```sh
# 1. 主机创建小型控制目录，输出 root 终端需要执行的 inventory 命令。
python3 test-app/tools/nx5/deploy.py --serial 192.168.1.151:5555 inspect

# 2. 执行上一步输出的 root 命令后，在主机归档候选文件并核对 SHA256。
python3 test-app/tools/nx5/deploy.py --serial 192.168.1.151:5555 prepare \
  --work /tmp/nx5-deploy-本次32位ID \
  --retain test-app/devices/nx5/deploy-retain.json \
  --archive evidence/nx5-deploy/run-01

# 3. 执行 prepare 输出的 root cleanup 命令后，上传到新的版本目录。
# 此处 source 是已 strip 的部署副本；目标路径必须不存在。
python3 test-app/tools/nx5/deploy.py --serial 192.168.1.151:5555 upload \
  --plan evidence/nx5-deploy/run-01/plan.json \
  target/nx5-deploy/display /tmp/slint-bridge-next/display

```

若仅在测试结束后归档清理、不上传文件，则以 `finish` 替代上面的 `upload`，二者只执行一个：

```sh
python3 test-app/tools/nx5/deploy.py --serial 192.168.1.151:5555 finish \
  --plan evidence/nx5-deploy/run-01/plan.json
```

两者都会逐文件取回控制目录并校验 SHA256，在同一设备锁内重新核对目录成员和摘要后才删除本轮控制文件；记录保存在计划旁的独立 `control-<ID>/` 目录。`upload` 只准备新程序路径，现有显示和恢复版本保持原样。完成后按设备显示所有权流程启动新版本。后续部署和测试结束后的清理都重复同一流程，未被保留或使用的旧测试副本会成为下一轮候选。

每份清单只能尝试一次 `prepare`。完整归档通过后才发布 `plan.ready`；其摘要覆盖启动 ID、保留路径、当前/恢复程序摘要和删除清单，清理回执绑定这份计划。清单采集或计划传输中断、摘要不符、重复准备及错误回执均会拒绝后续操作。root 清理还会重新核对启动 ID、当前/恢复程序摘要、实时活动引用及每个候选文件摘要；文件变化、引用变化或重启会使计划失败。上传前要求 `MemAvailable` 至少容纳整个新文件再加 16 MiB 余量，发布前再检查余量，并在发布命令内复核启动 ID 和清理回执；新文件以不可覆盖的方式放到目标路径。ADB 操作有 60 秒超时，并额外核对远端退出码，兼容本板旧 adbd。

失败的主机归档保留用于排查。准备或清理中断后，先确认原命令及其子进程已退出，再用 `discard` 校验归档剩余控制文件，重新从 `inspect` 开始；已删除的文件仍有本次主机归档，不重放部分完成的旧计划。内核会在全部锁持有进程退出后释放 `flock`，保留的空锁文件不表示正在运行，不应删除它。旧版本留下的同名锁目录会被拒绝，须在核对旧命令已全部退出后由 root 对该空目录执行 `rmdir`。

`discard` 不依赖设备端脚本完整，即使首次脚本传输或控制目录删除中断仍可恢复；归档失败、校验不符或控制目录变化时保留设备文件，用新的主机归档目录重试。设备重启后旧计划失效；若控制目录已随 tmpfs 消失，保留主机档案，恢复所需部署文件后开始新事务。以下命令不会应用旧清理计划，也不会删除已上传的程序。上传发布成功但收尾失败时可单独重试 `finish`；其他未使用的上传副本通过下一轮正常归档清理处理：

```sh
python3 test-app/tools/nx5/deploy.py --serial 192.168.1.151:5555 discard \
  --work /tmp/nx5-deploy-本次32位ID --archive evidence/nx5-deploy/aborted-01
```

日常截图使用同一入口，仍调用 Slint 原生 MCP，不新增截图服务器。输出目录须存在、文件须不存在；PNG 只保存在主机：

```sh
python3 test-app/tools/nx5/deploy.py --serial 192.168.1.151:5555 screenshot \
  evidence/nx5-deploy/screenshot-01.png
```

该命令先检查 16 MiB 最低可用内存，再查询实际窗口尺寸，并在发送 `take_screenshot` 前立即要求至少 `max(16 MiB, 3 × 两份 RGBA 像素缓冲)` 的余量。内存字段缺失或不足时拒绝截图。`--port` 默认 19315，自动转发到设备 localhost 9315。此检查覆盖脚本发出的顺序截图；直接调用其他 MCP 客户端时仍须遵守 [内存检查说明](../../../docs/slint-mcp.md#截图前的内存检查)。

部署工具回归不访问硬件，使用临时文件和模拟 `/proc` 执行实际清理 shell，覆盖归档损坏、当前/恢复依赖链接、链接改指、活动引用、设备链接、重启、摘要变化、计划截断、错误回执、SIGKILL 后锁释放、缺失/截断辅助脚本的恢复、低内存拒绝，以及只读控制文件的非交互清理：

```sh
python3 -m unittest discover -s test-app/tools/nx5 -p 'test_deploy.py' -v
sh -n test-app/tools/nx5/deploy-device.sh
sh -n test-app/tools/nx5/deploy-control.sh
```

2026-10-12 实机验证完成日志归档清理、校验上传约 5.8 MB 程序副本、受内存检查保护的原生截图，以及下一轮归档移除该非活动副本；PID 3972、当前程序和恢复文件未变。原始记录在 `evidence/nx5-deploy-script/2026-10-12/`，需另行取得；此前手工清理与十次截图的证据保持在 [内存报告](../../../docs/nx5-screenshot-memory.md)。

同日脚本审查修复后，29 项主机回归通过；在 #233 固件上另行验证目录外依赖保护、归档后链接改指拒绝、两轮归档清理、小型文件上传，以及缺失辅助脚本、空目录和仅余清理凭据的 `discard` 恢复。当前/恢复程序摘要和显示进程保持不变，本次测试文件已归档移除。记录在 `evidence/nx5-deploy-review/2026-10-12/`，需另行取得。重启、SIGKILL、传输损坏及计划截断使用主机故障回归验证；本轮未重启设备或触发实机 OOM，也未重复显示硬件及截图验证。

## 展示应用性能采样

`display/profile-showcase.py` 通过 Slint **原生 MCP** 驱动已启动的六页应用，依次测静态页、动画、列表往返滑动、两种透明混合状态和结束静态页。动态页预热约 8 秒、测量约 60 秒，测量期间不截图、不录制 MCP 事件。它使用已有 `SLINT_BRIDGE_TRACE` 的每 150 次成功提交日志，丢弃跨阶段开始边界的首批，不修改运行时或应用；MCP 手势节奏包含在测量负载中。

只读 `display/observe-performance.c` 同时按约 1 秒间隔记录指定进程的 RSS/VmHWM、CPU ticks、系统可用内存，并读取真实 evdev 时间戳；不独占或注入输入、不访问 framebuffer、不加载 SDK。它仅为自身 evdev 句柄设置 `CLOCK_REALTIME`，便于与原生 MCP 毫秒时间戳配对。

```sh
# 构建观察器，不需要厂商 SDK。
mkdir -p target/nx5-showcase-performance
/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi/bin/arm-linux-gnueabi-gcc \
  -std=c11 -O2 -Wall -Wextra -Werror \
  test-app/tools/nx5/display/observe-performance.c \
  -o target/nx5-showcase-performance/observe-performance
```

先按本文件的归档流程部署观察器。核实设备、显示所有者和恢复文件后，结束原显示进程，再以原字体、SDK 和设备参数启动**同一展示二进制**，设置 `SLINT_BRIDGE_TRACE=1`、`SLINT_MCP_PORT=9315` 和有限时长 `--seconds=900`，将日志写入新的管理目录下的 `trace.log`。原常驻 `start-live.sh` 会清除 trace，不能直接用它启动性能窗口。启动下一进程前须确认旧 PID 目录消失、VO 释放、独占锁无持有者；仅 `/proc/PID/exe` 消失不充分。

同一管理目录准备文本 `phase` 文件，允许已核验的 ADB 用户写入（本板为 UID 2000；只调整这个临时标记文件），然后由 root 启动观察器：`observe-performance EVDEV ACTUAL_PID 900 PHASE_FILE > resources.log`。其中 PID 必须来自本次启动并核对 `/proc/PID/exe`，不从历史快照复制。主机配置原生 MCP 端口转发后：

```sh
python3 test-app/tools/nx5/display/profile-showcase.py \
  --serial 192.168.1.151:5555 --remote /tmp/slint-bridge-perf-本次目录 \
  --output evidence/本次目录/automatic
```

该命令不会上传、启动或停止显示进程；`--remote` 下须已存在 `trace.log`、`phase` 和 `resources.log`，输出目录须不存在。它在结束后保持应用运行，采样结束仍需按设备流程恢复关闭 trace 的常驻程序并归档清理。

实体输入另开短窗口：MCP 设置好起始 Motion 页后，启动新观察器日志和原生事件录制，再让用户操作；从录制开始到结束不注入 MCP 输入。确认结束并松手后，停止录制并保存其 JSON 内容，停止只读观察器。每个窗口不超过 1024 条 MCP 事件；溢出会被分析器拒绝，不将环形记录覆盖当作 UI 输入丢失。以下触摸分析限定当前无旋转的 1024×600 NX5 轴映射，并用事件坐标检查该前提；结果是内核 `SYN_REPORT` 到 Slint 处理完成，包含回调，不是 LCD 端到端延迟。

```sh
python3 test-app/tools/nx5/display/analyze-showcase-performance.py \
  --phases evidence/本次目录/automatic/phases.json \
  --resources evidence/本次目录/automatic/resources.log \
  --touch-raw evidence/本次目录/physical.log \
  --touch-mcp evidence/本次目录/physical-mcp.json \
  --output evidence/本次目录/results.json

python3 -m unittest discover -s test-app/tools/nx5/display \
  -p 'test_showcase_performance.py' -v
```

仅汇总自动窗口时同时省略两个 `--touch-*` 参数。分析器测试覆盖阶段边界批次、缺失批次、实际采样间隔/内核时钟频率、SYN_REPORT 配对和不完整记录拒绝。首次实测结果见 [展示应用性能报告](../../../docs/nx5-showcase-performance.md)。

## 硬件诊断与证据重放

| 目录 | 用途 | 对应记录 |
| --- | --- | --- |
| `display/` | 进程内 WBC 采样、混色负对照、输入 EIO/TDE 退出、窗口启动回归与日志检查 | [最新实机回归](../../../docs/nx5-startup.md) |
| `tde/` | SDK 搜索路径/会话复用、原生故障注入及 TDE 输出结果重放 | [TDE 验证](../../../docs/nx5-tde.md) · [三项实机补测](../../../docs/nx5-hardware.md) |
| `present/` | 早期板级准入调查与原始观测判读 | [早期准入记录](../../../docs/nx5-present-validation.md) |
| `flip/` | 阻塞 PAN 周期与实际刷新时序分析 | [翻页分析](../../../docs/nx5-flip.md) |
| `single-cycle/` | 队列、待锁存与当前页状态诊断 | [单周期调查](../../../docs/nx5-single-cycle.md) |
| `fps/` | 已保存的整屏优化证据重放 | [帧率记录](../../../docs/nx5-fps.md) |
| `parallel/` | 已保存的并行渲染/翻页证据重放 | [并行记录](../../../docs/nx5-parallel.md) |
| `scheduling/` | 定时等待探针及已保存调度证据重放 | [调度记录](../../../docs/nx5-scheduling.md) |

UI 截图、元素查询和交互直接使用 [Slint MCP](../../../docs/slint-mcp.md)。不要把硬件专有工具放入通用测试入口，也不要默认批量执行设备探针；部分工具要求暂停显示、独占设备或特定固件。

`display/run-startup.sh` 驱动专用 `startup-regression` 应用，保存首次显示失败、重试和延迟创建的资源及内核记录；`display/check-startup.py` 检查窗口状态、真实像素、任务执行及 TDE 清理，拒绝旧画面或缺失证据。新日志包含实际宽高，检查器按这些尺寸核对页偏移及像素数；旧日志没有尺寸时须用 `--size WIDTHxHEIGHT` 指定当时设备的记录值。步骤见 [启动实机回归](../../../docs/nx5-startup.md)。

`display/run-regression.sh` 在设备上执行有限时长用例并保存日志、退出码和资源快照；`display/inject-read-error.c` 只用于诊断进程内的一次 evdev EIO。`display/check-exit.py` 核对活动 TDE 分配、退出后的 MMZ、连续内核记录及 EIO/WBC 清理顺序。构建、部署、预期失败和证据判据见最新实机回归报告。原始寄存器读数不能替代实际 WBC 帧、驱动计数及分配状态。

从仓库根重放现有证据：

```sh
python3 test-app/tools/nx5/present/test_analyze.py
python3 test-app/tools/nx5/tde/check-results.py
python3 test-app/tools/nx5/single-cycle/analyze.py evidence/nx5-single-cycle/stages.log
```

下列判据目前应失败，表示没有达成单周期目标，不能改为放宽验收：

```sh
python3 test-app/tools/nx5/single-cycle/analyze.py evidence/nx5-single-cycle/stages.log --min-fps 55
```

`NX5_SDK_INCLUDE=<codec头文件目录> sh test-app/tools/nx5/tde/check-native.sh` 在主机运行原生故障注入和像素转换边界检查，并用替身 SDK 验证：从两个临时目录（含空格路径）加载无版本库名 `libVideoDec.so`，TDE 在显示会话加载前拒绝自行加载、加载后复用同一会话，构造函数只运行一次，引用正确释放。该检查不加载真实 SDK，也不访问设备。

维护中的 `display/run-regression.sh` 与 `display/run-startup.sh` 接受 `NX5_FRAMEBUFFER`（默认 `/dev/fb0`）和可选的 `NX5_TOUCH_DEVICE`；输入 EIO 用例必须指定触摸设备。脚本继承调用者的 `LD_LIBRARY_PATH` 和 `SLINT_DEFAULT_FONT`，不覆盖 SDK、插件或字体目录。显示回归使用板上已有的 `ls -ldnL` 获取所选设备的主次设备号并定位 sysfs，支持设备链接；不依赖该固件缺少的 `stat` 命令。退出检查将 MMZ 用量和块数与同次初始快照比较，不再假定某一分辨率下的固定显存占用。历史原始记录保持不变。

`blocking_flip_cadence` 可用 `SLINT_BRIDGE_TEST_FRAMEBUFFER` 选择 framebuffer，默认使用 `Config` 的路径；页偏移来自设备实际高度。TDE 像素比较中的固定图像尺寸是测试素材，不限制运行时显示尺寸。

`display/build-alpha-observer.sh` 构建 `libalpha-observer.so`，由 `alpha-regression` 在已取得显示所有权的进程内加载。它只读取物理前台页、VO 公共属性，并临时使用空闲 WBC；不初始化/关闭共用 VO，不改图层 alpha，也不接入解码器。framebuffer 检查采用当前 NX5 的 0–128 alpha 编码，WBC 比较半透明 RGB 色块与对应黑底不透明参考色的 Y/UV，要求设备现有底色为黑，通道误差至多 2。全透明阶段还检查输出为黑；不透明原色必须可见，避免全黑采样误判通过。UI 在每阶段事件循环退出并等待提交完成后采样；这是静态阶段和交替前台页验证，不是连续运动中每个扫描帧的验证。

```sh
NX5_SDK_INCLUDE=/home/dpower/data/GitLab/LGUI3/c_library/third_party/include/NX5/codec \
NX5_SDK_LIBRARY=/home/dpower/data/GitLab/LGUI3/c_library/third_party/lib/NX5/codec_rorate/libVideoDec-1.6.6.so \
sh test-app/tools/nx5/display/build-alpha-observer.sh
```

按设备说明交叉构建 `--bin alpha-regression`，分别使用 `nx5` 和 `nx5-tde` 验证软件/TDE feature 的输出。部署插件和程序、确认原显示所有者退出后，设置同一个 SDK/字体环境和插件搜索路径，运行 `alpha-regression OUTPUT_DIRECTORY [--framebuffer=PATH] [--touch=PATH]`；输出目录必须已存在。可用 `run-alpha.sh BINARY_DIRECTORY tde|software` 保存资源和内核快照；目录内分别放置 `alpha-tde`、`alpha-software` 及插件，同一结果目录拒绝覆盖。成功返回 0，日志应有五个 `passed=true` 阶段、每帧 `bad=0`，保存五份 WBC CSV；在主机运行 `python3 test-app/tools/nx5/display/check-alpha.py <结果目录>` 核对实际页、样本完整性、误差和退出资源。此测试只接受对应 8×6 色块网格可整除的诊断尺寸，不限制库支持的面板大小。

共享 MMZ 画布的面积、NEON 边界和真实写页回归入口见 [TDE 报告](../../../docs/nx5-tde.md#构建与接入)。真实写页测试使用生产像素转换与完成契约，交替呈现 210 帧色块；需独占显示，测完恢复常驻程序。NEON 边界项仅执行 CPU 转换，不加载 SDK。
