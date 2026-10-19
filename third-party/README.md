# 随仓库保存的构建与部署资源

这些文件用于迁移开发机器时保持相同的 SDK ABI 和字体，不依赖原机器上的 LGUI3 工程或系统字体安装。文件按来源原样复制，未修改厂商头文件、动态库或字体内容；`SHA256SUMS` 记录原文件摘要；本目录的 Git 属性保留原始换行和字节内容。

| 路径 | 用途与来源 |
| --- | --- |
| `nx5/include/` | TDE/MMZ 和 WBC 构建需要的 `mpi_tde.h`、`mpi_sys.h`、`mpi_vo.h` 及传递依赖；`vd/VoApi.h` 及其依赖保留 Rust VO FFI 的 ABI 依据。共 15 个头文件，来自 LGUI3 的 `c_library/third_party/include/NX5/codec/`。 |
| `nx5/lib/libVideoDec-1.6.6.so` | 来自同一工程 `c_library/third_party/lib/NX5/codec_rorate/` 的 ARM32 soft-float 厂商 SDK。与现有 NX5 #233 验证使用的库摘要一致；用于构建观察器插件和部署，不在主机执行。 |
| `nx5/lib/libVideoDec.so` | 指向上述版本库的相对软链接，供动态加载与链接器查找。实际 SONAME 为 `libVideoDec-1.6.6.so`，部署时同时保留版本文件和别名。 |
| `f1/include/` | 从 `/home/dpower/data/GitLab/slint-f1/lib/g2d/` 原样保存的 `dma-heap.h`、`video/artinchip_ge.h`、`video/mpp_types.h`；用于 F1 normal-mode UAPI 编译校验，保留原 SPDX/版权声明。Rust 路径直接使用内核 ioctl，不依赖视频解码库。 |
| `fonts/DejaVuSans.ttf` | 原 `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf`，用于两设备部署及匹配字体的画面对照。 |
| `fonts/DejaVu-COPYRIGHT` | 原 `fonts-dejavu-core` 包的版权说明；字体适用其中的 Bitstream Vera 许可，DejaVu 修改为公有领域。随字体部署。 |

SDK 保留原头文件中的厂商版权声明；源目录没有独立的 SDK 许可证文件，本目录不将这些厂商文件重新授权为项目原创代码。

## 默认路径

- ARM `nx5-tde` 的 `build.rs` 从本 package 的 `CARGO_MANIFEST_DIR` 定位 `nx5/include/`，因此其他 workspace 通过 path 引用本库时也可找到它。
- NX5 原生检查及观察器构建脚本先定位仓库根，再使用这里的头文件和动态库。已有 `NX5_SDK_INCLUDE`、`NX5_SDK_LIBRARY` 和 `NX5_SDK_LIB` 环境变量仍可显式覆盖；覆盖时须保持头文件、运行库及设备 ABI 配套。
- `libVideoDec.so` 的运行时加载方式仍由设备的 `LD_LIBRARY_PATH` 和系统加载器决定，构建时不将主机仓库路径写入设备的库搜索路径。
- UI 图片、SVG 和素材生成脚本已经位于 `test-app/ui/assets/`，不再复制一份。Slint 仍按 Cargo 清单从指定 Git 仓库获取。

从仓库根核验资源：

```sh
(cd third-party && sha256sum -c SHA256SUMS)
```

## 新机器仍需安装的工具

资源入库不包含编译器、系统库或 Cargo 缓存。新机器需要 Rust 1.92+、对应 Rust ARM target、主机 C 编译器及 Python；先运行 `cargo fetch --locked` 获取依赖。NX5 使用 `.cargo/config.toml` 指定的 `/opt/X5/gcc-linaro-5.3.1-2016.05-x86_64_arm-linux-gnueabi` 工具链，X9 使用 `.cargo/x9.toml` 指定的 `/opt/X9/gcc-linaro-5.5.0-2017.10-x86_64_arm-linux-gnueabihf` 工具链。两者 ABI 不同，须安装匹配版本及 sysroot；若安装目录不同，调整 Cargo 链接器和 CC/CXX/AR 配置。独立 NX5 C 工具可通过 `NX5_CC` 覆盖编译器路径。

F1 集成构建另需 `riscv64gc-unknown-linux-gnu` target 与 `.cargo/f1.toml`
指定的 `/opt/F1` T-HEAD 工具链。该 target 使用 LP64D ABI，不能复用 ARM 库；
F1 当前显示准入状态见 [设备说明](../test-app/devices/f1/README.md)。

部署工具还需 ADB；像素分析脚本按各自说明安装 Pillow/NumPy 等 Python 依赖。历史 `evidence/` 不属于构建资源，仍作为本地原始档案保留。
