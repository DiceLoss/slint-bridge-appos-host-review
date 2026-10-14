# 展示应用图片背景

`coastal-background.png` 是保留的原始背景：1638×960、PNG。[build.rs](../../build.rs) 在主机构建时按 `cover` 居中裁剪/缩放为 1024×600 RGBA PNG（Triangle 滤波），写入 Cargo `OUT_DIR` 并嵌入应用。设备只解码这份约 2.34 MiB 的 RGBA 像素，不分配原图解码及缩图临时缓冲；默认面板尺寸下背景可使用现有 TDE 复制。其他窗口尺寸仍由 Slint `cover` 适配，超出 TDE 条件时软件回退。背景仍是真实照片，不是色块或渐变。公共卡片和列表行使用半透明底色，Motion 绘图区另使用同一图片资源；Layers 透明开口会隐藏全屏图片。

2026-10-13 使用内置 `image_gen`（imagegen 技能）生成，未使用 CLI/API 回退。此文件保留原始生成尺寸与字节；构建产物会进行上述转换，不覆盖原图。首次直接嵌入原图的历史结果见 [图片背景报告](../../../docs/nx5-image-background.md)，不代表当前构建期预处理方案的性能。

生成提示词：

> Use case: photorealistic-natural. Asset type: an opaque raster wallpaper for a 1024×600 landscape embedded touch-screen control console, not a UI mockup. Create a calm misty coastal landscape photograph, broad layered blue-green hills and softly textured teal water, atmospheric pale blue daylight, fine natural photographic detail and gentle tonal variation across the entire frame. Tasteful low contrast so dark interface text and translucent pale panels will remain readable. Wide landscape composition close to a 1.707:1 aspect ratio, no dominant foreground object. Output only the background image, ideally 1024×600 pixels. No words, labels, logos, frames, UI controls, people, buildings, or watermark. This is the actual image background to be rendered beneath moving and scrolling Slint controls, so it must have real image detail rather than a flat color or simple gradient.


## 运行时缩放基准素材

`scaling-reference.svg` 与 `scaling-reference.png` 是 256×160 的确定性测试图，包含四个纯色象限、细线、圆和透明外边缘。PNG 在原始尺寸下由同目录 SVG 栅格化生成；两者均不经过 `build.rs` 的背景预处理。

`showcase::configure()` 通过 `Image::load_from_data(..., Some("png"))` 和 `Image::load_from_svg_data()` 在应用运行时加载嵌入的原始字节。独立 Image scaling 页只改变同一 Image 的显示尺寸，测试 50%～200%（128×80～512×320）及 125% 非整数比例；没有为各缩放档位生成预缩放文件。

需要重建原始 PNG 时，从仓库根运行（输出仍为 SVG 固有的 256×160）：

```sh
python3 -c 'import cairosvg; cairosvg.svg2png(url="test-app/ui/assets/scaling-reference.svg", write_to="test-app/ui/assets/scaling-reference.png")'
```

## 富内容长列表

`rich-row-{blue,amber}.png` 为 800×112，`rich-card-{blue,amber}.png` 为 244×328，透明边缘内含圆角、渐变、光斑、弧线和多层柔和阴影。四份同名 SVG 为底图源，使用本目录 `generate-rich-cards.py`（Python + CairoSVG）确定性生成；它不是 Cargo 构建步骤。PNG 原图在首次进入 Rich list 时加载，按面板上的原生卡片尺寸绘制。

`rich-{climate,light,room,air}.svg` 是 32×32 主图标，`rich-chevron.svg` 是 16×16 箭头，均为本工程绘制的简单矢量路径。它们通过 `Image::load_from_svg_data` 保留为运行时 SVG；未合入底图、未预先栅格化。两套底板在 100 项中交替复用，不是 100 张不同照片的纹理负载。
