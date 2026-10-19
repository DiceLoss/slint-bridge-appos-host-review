# Slint 1.18.1 补丁来源

本目录维护官方源码基线、累积补丁及依赖包映射；准备流程见[获取配置与使用](../../docs/getting-started.md)。

- 官方仓库：`https://github.com/slint-ui/slint.git`，标签 `v1.18.1`。
- 固定提交：`372cf0ee5577c3dfec309a45e7b778ba4e81b734`。
- 官方树：`b49d21cc696c4aba26074e24fab0e17448fed8da`。
- 补丁后树：`17ffa560ec9ad91dca05e3be309b7a95e52d2324`。
- 补丁来源：原配套仓库 `bcb6381..b62643e` 的 13 个提交，12 个文件。

官方树与原导入基线完全相同，应用补丁后的整棵树与 `b62643e` 完全相同。
补丁是 `git diff --binary --full-index` 格式，用 `git apply`，不能用 `git am`。
`manifest.json` 是准备脚本读取的版本、补丁 SHA256 和包路径清单。
源码沿用 Slint 文件中的版权及许可声明。

补丁包括纹理源身份、绘制依赖失效、SVG 栅格缓存、旋转截图和 SVG 尺寸修复、旋转分块、批量混合、外部像素及其兼容性隔离、尺寸和空图片相等性修复。
它也保留关闭 Parley 复杂文字断行字典的既有取舍；外部像素能力仅支持 Rust + `std`，与 FFI 互斥。

维护者更新补丁时同时更新清单摘要和结果树；如果包映射变化，还需同步根 `.cargo/config.toml`。
不要直接修改准备目录来发布修复，也不要让脚本静默忽略应用失败。
已有准备目录与新清单不一致时，先保存需要的改动并移出旧源码，再重新准备。
脚本会保留不匹配的源码，不执行自动重置或覆盖。

回归入口：

```sh
python3 -m unittest discover -s tools/tests -v
python3 tools/prepare-slint.py --offline
```

准备脚本测试使用本地 Git fixture，不联网；覆盖首次准备、重复及离线复用、损坏补丁、错误源码树和本地改动保护。
正式源码完整性、Cargo 来源一致性、主机 feature 矩阵及交叉构建仍须按仓库验证要求执行。
