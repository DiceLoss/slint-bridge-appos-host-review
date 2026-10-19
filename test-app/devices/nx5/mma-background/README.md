# MMA 背景

通过统一的 `Config.background_storage` 选择背景存储，默认 `BackgroundStorage::Heap` 保留普通 Slint 图像。
`BackgroundStorage::Mma` 仅支持启用 `nx5-mma-background` 的 ARM32 NX5；初始化必须成功取得 TDE 画布，否则报错，不静默改用普通背景。
X9/其他构建选择普通模式时行为不变；显式选择不支持的 MMA 模式会在设备初始化前被拒绝。

MMA 复用现有 **0 号全尺寸源槽**，不新增分配。
源图片必须不透明且与物理画布同尺寸，每个会话只能准备一张，并须在首次绘制之前准备。
普通图片继续使用另一全尺寸槽及紧凑槽；背景缩放、裁剪或透明度不满足硬件复制条件时，Slint 从同一 CPU 映射软件采样。
所有图像引用释放后才能释放其 MMA 所有者；未知 DMA 保持原有资源隔离，不走软件回退。

## 构建

首次运行 `python3 tools/prepare-slint.py`，所有 Slint 包统一使用已校验的官方 1.18.1 补丁源码。
`nx5-mma-background` 自动启用配套 Slint 的 `external-pixel-buffer`，补丁由准备脚本应用，完成准备后使用普通 Cargo 命令，无需临时修改锁文件。
Slint 外部像素 feature 为显式 Rust/std 能力，与 FFI 互斥；普通构建不启用它，保持原像素布局。

```sh
SLINT_EMIT_DEBUG_INFO=1 cargo --config .cargo/nx5.toml build --locked --offline --release \
  -p slint-bridge-test-app --features nx5-mma-background,slint/mcp \
  --target arm-unknown-linux-gnueabi --bin showcase
```

普通 NX5 TDE 可继续只启用 `nx5-tde`；X9 继续启用 `fbdev`。
新机器需要先 `cargo fetch --locked`。Slint 提交尚未发布到配套远端时，新机器不能取得该锁定提交；本地提交不等同于依赖已发布。
原验证函数 `nx5_mma_background(&pixels)` 已由 `config.prepare_background(image)` 替代；应用直接传 Slint Image，不再手工提取像素或另开后端入口。旧命令中的 `slint/unstable-external-pixel-buffer` 不再使用。
旧验证补丁与包装脚本保留在 `4ff149e` 及更早的 Git 历史中，历史实测不改写为当前版本结果。

## 应用接入

```rust,no_run
use slint_bridge::{BackgroundStorage, Config};

let config = Config {
    background_storage: BackgroundStorage::Mma,
    ..Config::default()
};
slint_bridge::init(config.clone())?;
// background_png: &[u8]，由应用提供的 PNG 数据。
let source = slint::Image::load_from_data(background_png, Some("png"))?;
let background = config.prepare_background(source)?;
// 在首次 show/run 前：app.set_wallpaper(background);
```

`prepare_background` 消费图像句柄：普通模式原样返回，MMA 模式复制一次到已有源槽并返回共享所有者的图像。
配置不会自动替换应用中的任意 `Image`，应用应把返回值赋给自己的背景属性。
为避免普通堆副本继续驻留，不要同时保存原图克隆或先通过 `.slint` 默认资源加载相同背景；showcase 已在 Rust 初始化阶段选择存储。

showcase 参数：`--background=heap`（默认）、`--background=mma`；兼容旧 `--mma-background` 别名，均映射到同一个 Config 字段。
主机 `--preview` 仅支持普通模式。
设备运行与恢复按 [NX5 说明](../README.md)，此处不绕过独占和双页准入。

## 回归与诊断

```sh
cargo test --locked --offline --workspace --features nx5-mma-background
sh test-app/tools/nx5/tde/check-native.sh
```

正式 `nx5-mma-background` 不包含环境变量故障注入。
需要复测已知完成状态的软件回退时，单独用 `--features nx5-test-hooks,slint/mcp` 构建测试程序，再设置 `SLINT_BRIDGE_MMA_FORCE_FALLBACK=1` 并以 `--background=mma` 运行。
该测试开关在首次背景 DMA 提交前注入一次错误，走禁用 TDE、整帧重绘路径；不能替代未知 DMA 完成验证。
常驻部署使用不含 `nx5-test-hooks` 的正式构建。

已有实测及适用版本见 [MMA 背景记录](../../../../docs/nx5-mma-background.md)。
