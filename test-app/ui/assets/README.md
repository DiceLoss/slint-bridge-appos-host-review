# 展示应用图片背景

`coastal-background.png` 是保留的原始背景：1638×960、PNG。[build.rs](../../build.rs) 在主机构建时按 `cover` 居中裁剪/缩放为 1024×600 RGBA PNG（Triangle 滤波），写入 Cargo `OUT_DIR` 并嵌入应用。设备只解码这份约 2.34 MiB 的 RGBA 像素，不分配原图解码及缩图临时缓冲；默认面板尺寸下背景可使用现有 TDE 复制。其他窗口尺寸仍由 Slint `cover` 适配，超出 TDE 条件时软件回退。背景仍是真实照片，不是色块或渐变。公共卡片和列表行使用半透明底色，Motion 绘图区另使用同一图片资源；Layers 透明开口会隐藏全屏图片。

2026-10-13 使用内置 `image_gen`（imagegen 技能）生成，未使用 CLI/API 回退。此文件保留原始生成尺寸与字节；构建产物会进行上述转换，不覆盖原图。首次直接嵌入原图的历史结果见 [图片背景报告](../../../docs/nx5-image-background.md)，不代表当前构建期预处理方案的性能。

生成提示词：

> Use case: photorealistic-natural. Asset type: an opaque raster wallpaper for a 1024×600 landscape embedded touch-screen control console, not a UI mockup. Create a calm misty coastal landscape photograph, broad layered blue-green hills and softly textured teal water, atmospheric pale blue daylight, fine natural photographic detail and gentle tonal variation across the entire frame. Tasteful low contrast so dark interface text and translucent pale panels will remain readable. Wide landscape composition close to a 1.707:1 aspect ratio, no dominant foreground object. Output only the background image, ideally 1024×600 pixels. No words, labels, logos, frames, UI controls, people, buildings, or watermark. This is the actual image background to be rendered beneath moving and scrolling Slint controls, so it must have real image detail rather than a flat color or simple gradient.
