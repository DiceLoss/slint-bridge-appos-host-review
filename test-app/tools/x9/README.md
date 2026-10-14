# X9 验证工具

仅用于 [已准入 X9](../../devices/x9/README.md)，运行前重新核对设备身份、当前显示所有者及恢复步骤。工具不加载厂商 SDK，不分配新的物理显示页。

从仓库根目录交叉编译：

```sh
/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf/bin/arm-linux-gnueabihf-gcc \
  -O2 -Wall -Wextra -Werror -o target/x9-capture test-app/tools/x9/capture.c
/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf/bin/arm-linux-gnueabihf-gcc \
  -O2 -Wall -Wextra -Werror -o target/x9-inject-touch test-app/tools/x9/inject-touch.c
```

- `capture OUTPUT.bgra`：以只读方式读取当时前台的 800×1280 / stride 3200 / BGRA8888 像素，前后校验页号不变，输出文件必须不存在。应在静态页面、提交完成后执行。页号不变不能证明任意动态画面的原子采样，也不是扫描输出或无撕裂认证。取回文件后逆时针旋转 90°，与同状态 1280×800 Slint 原生截图比较。
- `inject-touch RAW_X RAW_Y`：通过 `/dev/input/event1` 写完整 Type-A 按下和释放包；若已有 BTN_TOUCH 接触则拒绝。范围为 X=0..800、Y=0..1280。顺时针 90° 显示下，逻辑点 `(x,y)` 对应原始轴约 `((799-y)*800/799, x*1280/1279)`。这是 evdev 注入，覆盖内核输入事件到应用指针的路径，不能称为用户实体触摸。

`recover-console.c` 编译方式与上述两个 C 工具相同。仅在目标显示进程退出、重新核对当前所有者后运行 `recover-console`；它先获取独占资源并等到同一 FB 的匹配完成事件，再恢复 KD_TEXT。正在运行的应用持锁时工具必须失败；若等待失败，不得把其退出当作恢复成功。本轮已验证占用拒绝和停止/注入失败后的恢复。

`fault-initial-flip.c` 是实机失败分支探针，用相同编译器加 `-shared -fPIC -ldl` 构建。仅在独占测试启动时设置 `LD_PRELOAD=探针.so SLINT_BRIDGE_X9_FAULT_LOG=新日志路径`，让 page-flip ioctl 在入队前返回 EIO，并记录 KD 模式调用。应用应初始化失败、退出非零且不调用 KD_TEXT；随后用恢复工具恢复。绝不用于常驻程序；它不模拟真实 DMA 卡死。

工具由 root 执行；ADB 只负责传输。`capture` 临时增加约 4 MiB 进程内存与 4 MiB tmpfs 文件，先检查空闲内存，取回并校验后清理板端像素文件。注入时不要同时操作实体屏幕或发送 MCP 点击，以免混淆录制事件。

## 独立 CPU 参考

`test-app/examples/x9-reference.rs` 不打开设备，使用应用公开接口导出 800×1280、Rotate90、RGBA 的 Controls 48% 与 Scaling 125% 静态整帧。设备默认配置不改变其他表单值，光标离开控件后再采样。主机字体必须匹配设备仅注册的 DejaVuSans.ttf；否则主机系统的真实 Bold 字体会与设备合成粗体不同。

```sh
mkdir -p target/x9-reference-fonts target/slint-bridge-x9-fontconfig
# 目标链接不存在时创建，勿覆盖其他字体目录。
ln -s /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf target/x9-reference-fonts/DejaVuSans.ttf
XDG_CACHE_HOME="$PWD/target" FONTCONFIG_FILE="$PWD/test-app/tools/x9/fonts.conf" \
SLINT_DEFAULT_FONT=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf \
cargo run --locked --offline -p slint-bridge-test-app --features fbdev \
  --example x9-reference -- target/x9-reference-new
```

输出目录必须不存在。将设备 BGRA 的 R/B 通道交换，与参考 RGBA 逐字节比较，无需再旋转。与逻辑方向 MCP 截图直接比较会混入 Slint 曲线/圆角在不同扫描方向下的栅格边缘差异；物理页验证使用同旋转独立参考，MCP 用于界面和交互检查。

## 富列表性能采样

`profile-rich-list.py` 用原生 MCP 驱动同一正式 `showcase`：静态 Controls、富列表纵向、横向、停止滑动及返回 Controls。每个方向先在列表内部定位、预热至少 10 秒，再连续拖动至少 60 秒；每次核对内容位移大于视口的 40%、轴不漂移且未停在端点。MCP 查询和手势插值的开销包含在负载中。采样不截图、不录制输入，不把 UI ticks 当作 FPS。

`observe-resources.c PID MAX_SECONDS PHASE_FILE` 每秒只读 `/proc`，记录单调时钟、进程 CPU ticks、RSS/VmHWM、major faults、线程数、CPU 频率、MemFree/Shmem 以及观察器自身 CPU ticks。绑定进程 starttime，PID 被复用或进程退出会失败；phase 为 `done` 时正常结束。编译方法同本目录其他普通 C 工具。

`count-flips.c` 用相同交叉编译器加 `-shared -fPIC -ldl` 构建。仅测 FPS 的独立进程设置 `LD_PRELOAD=计数库.so SLINT_BRIDGE_X9_FPS_FILE=新文件`；探针转发原 ioctl/read，核对成功 page-flip 的 fd、cookie 和 32 字节 FLIP_COMPLETE，然后记录单调时间。不改变返回值、事件或翻页顺序，不加载 SDK。固定映射为 262,160 字节，最多 32,768 条事件，溢出、错配或提交错误会被分析器拒绝。CPU/RSS 主结果使用**无此探针的新进程**，避免混入额外映射和计数开销。

重新核对目标、所有者、程序摘要与恢复工具后，按设备说明切换到同一正式二进制的新进程。临时目录必须新建为 `/tmp/slint-bridge-x9-perf-...`，由 ADB 用户创建可写的 `phase` 文件；仅这个工作目录用于阶段标记。root 启动外部观察器，将 CSV 保存到该目录：

```sh
observe-resources ACTUAL_PID 420 WORK_DIR/phase > WORK_DIR/resources.csv
```

观察器与应用启动后，在主机运行；输出目录必须不存在：

```sh
python3 test-app/tools/x9/profile-rich-list.py --serial 192.168.1.143:5555 \
  --remote /tmp/slint-bridge-x9-perf-本次目录 --output evidence/本次目录/resources
```

对无探针和带计数探针的两个新进程各执行一次，保存各自 CSV / 应用日志，后者另外取回 `flips.bin`。结束后按恢复流程启动不含探针的常驻程序，确认其 maps 没有计数库，再归档并清理本轮诊断文件。分析命令：

```sh
python3 test-app/tools/x9/analyze-performance.py --samples resources.csv \
  --phases resources/phases.json --output resources-summary.json
python3 test-app/tools/x9/analyze-performance.py --samples fps-resources.csv \
  --phases fps/phases.json --flips flips.bin --output fps-summary.json
```

分析使用设备单调时钟，以单核 100% 计算 CPU；FPS 为同窗口中匹配完成事件数除以时长。统一丢弃首个样本，并将末端限制在主机动作窗口内部，排除阶段边界和导航设置。动态有效窗口至少 45 秒。VmHWM 属于该次进程生命周期，不能当作每个阶段独立峰值；RSS 不包含 tmpfs 部署文件，也不等于整机或独立扫描缓冲的总占用。

主机检查：

```sh
cc -std=c11 -O2 -Wall -Wextra -Werror -DPROBE_TEST \
  test-app/tools/x9/count-flips.c -o target/x9-count-test
./target/x9-count-test
python3 -m unittest discover -s test-app/tools/x9 -p 'test_performance.py' -v
```

## 绘制阶段对照

`test-app/examples/x9-render-bench.rs` 在同一 X9 CPU 上运行真实 Showcase 富列表，但不打开 framebuffer、输入或 SDK。参数为 `0|90 vertical|horizontal 帧数 [新输出.rgba]`；先定位到列表内部，预热 20 帧，再每帧固定滚动 8 个逻辑像素，输出事件处理、render 耗时和脏像素面积。帧数允许 1–300，末尾输出全图 FNV 摘要，可另存 RGBA 作逐字节对照。

```sh
SLINT_EMIT_DEBUG_INFO=1 cargo --config .cargo/x9.toml build --locked --offline --release \
  --target arm-unknown-linux-gnueabihf --features fbdev,slint/mcp \
  -p slint-bridge-test-app --example x9-render-bench
# 上传后在 X9 root 会话运行，每个输出路径必须尚不存在：
SLINT_DEFAULT_FONT=RUN_DIR/DejaVuSans.ttf RUN_DIR/x9-render-bench \
  90 vertical 60 RUN_DIR/vertical-90.rgba > RUN_DIR/vertical-90.csv
```

同轴 90° 与 0° 的 render 差用于估计旋转增加的 CPU 成本；软件 renderer 将旋转融合在采样过程中，该差值不是一次独立全屏旋转复制的耗时。它不包含实际写页、提交和 MCP 手势，不能换算为正式应用 FPS。对照必须保持 ELF 构建配置、字体、帧数和动作一致；正式 FPS/CPU/RSS 仍使用上一节的方法。
