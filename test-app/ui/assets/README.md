# 展示应用图片背景

`coastal-background.png` 是 `showcase` 使用的实际位图背景：1638×960、PNG，编译时嵌入应用。Slint 按窗口尺寸以 `cover` 缩放；不是运行时生成的色块或渐变。公共卡片和列表行使用半透明底色，Motion 绘图区另使用同一图片资源；Layers 透明开口会隐藏全屏图片。

2026-10-13 使用内置 `image_gen`（imagegen 技能）生成，未使用 CLI/API 回退。原始生成尺寸保留，没有额外裁剪或重采样。该尺寸在 1024×600 设备上需要缩放，其内存与渲染成本应以对应实测为准。

生成提示词：

> Use case: photorealistic-natural. Asset type: an opaque raster wallpaper for a 1024×600 landscape embedded touch-screen control console, not a UI mockup. Create a calm misty coastal landscape photograph, broad layered blue-green hills and softly textured teal water, atmospheric pale blue daylight, fine natural photographic detail and gentle tonal variation across the entire frame. Tasteful low contrast so dark interface text and translucent pale panels will remain readable. Wide landscape composition close to a 1.707:1 aspect ratio, no dominant foreground object. Output only the background image, ideally 1024×600 pixels. No words, labels, logos, frames, UI controls, people, buildings, or watermark. This is the actual image background to be rendered beneath moving and scrolling Slint controls, so it must have real image detail rather than a flat color or simple gradient.
