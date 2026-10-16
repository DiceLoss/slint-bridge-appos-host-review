# MMA 背景验证版

此验证版复用现有 **0 号全尺寸源槽**，不新增 MMA 分配。
背景必须与画布同尺寸、RGBA 且完全不透明；每个会话只设置一次。
普通图片改用另一块全尺寸槽和紧凑槽；背景缩放、裁剪或透明度不符合直接复制条件时，Slint 从同一 CPU 映射软件采样。

Slint 上游接口不能保留外部像素所有者，本目录保存相对于 `c878901` 的 [验证补丁](slint-external-pixels.patch)。
补丁使 `SharedPixelBuffer` 保留只读外部存储的所有者，克隆不复制像素，修改时才复制到普通堆。
这是 Rust 验证补丁；没有验证其他语言 ABI 或其他 renderer，不作为通用 Slint API 发布。

在同源 Slint 检出目录应用补丁，再从本仓库根执行：

```sh
# 在 Slint 源码目录执行，补丁路径指向本仓库。
git apply /absolute/path/slint_bridge/test-app/devices/nx5/mma-background/slint-external-pixels.patch

# 从 slint_bridge 根执行。指定已应用补丁的 Slint 源码目录。
SLINT_EMIT_DEBUG_INFO=1 python3 test-app/tools/nx5/tde/with-mma-slint.py \
  --slint-root /absolute/path/slint-source -- build --config .cargo/nx5.toml \
  --locked --offline --release -p slint-bridge-test-app \
  --features nx5-mma-background,slint/mcp --target arm-unknown-linux-gnueabi --bin showcase
```

包装脚本检查补丁已应用，把所有 Slint 包一起切换到该源码，保留其他依赖的锁定版本，命令结束时恢复原 `Cargo.lock`。
不要同时在同一工作目录运行其他 Cargo 命令。
普通构建仍使用原 Git 依赖，不需要该补丁；`nx5-mma-background` 必须通过此入口构建。

完成 [设备部署和独占检查](../README.md) 后，用同一 ELF 进行对照：

- 普通背景：`showcase --seconds=100`
- MMA 背景：`showcase --mma-background --seconds=100`
- 已知完成状态的软件回退：`SLINT_BRIDGE_MMA_FORCE_FALLBACK=1 showcase --mma-background --seconds=100`

第三种模式在第一次背景 DMA 提交前注入一次失败，走现有禁用 TDE、整帧重绘路径；它不是未知 DMA 完成测试。
未知 DMA 的保留资源行为由原生故障注入覆盖，不能通过软件回退绕过。
背景图像的所有者同时保留 TDE 分配和 SDK 会话，避免窗口销毁后残留的图像引用访问已释放映射。

主机检查：

```sh
python3 test-app/tools/nx5/tde/with-mma-slint.py \
  --slint-root /absolute/path/slint-source -- test --locked --offline \
  -p slint-bridge-test-app --features nx5-mma-background --test external_background
sh test-app/tools/nx5/tde/check-native.sh
```

普通 feature 矩阵、格式、clippy 和文档检查仍按仓库说明执行。
实测结果与验证边界见 [MMA 背景记录](../../../../docs/nx5-mma-background.md)。
