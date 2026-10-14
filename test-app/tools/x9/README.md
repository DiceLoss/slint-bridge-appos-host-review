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
