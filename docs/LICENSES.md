# 许可

## 本项目

本项目使用 **MIT**（© 2026 kugou-tui contributors）。

许可全文在仓库根目录的 `LICENSE`；通过包管理器安装后位于 `/usr/share/licenses/kugou-tui/LICENSE`。

> 这里刻意不写指向 `LICENSE` 的相对链接：本文档会被装到 `/usr/share/doc/kugou-tui/docs/`，而按 Arch 惯例许可文件在 `/usr/share/licenses/` 下 —— 相对链接在安装后必然断掉。写路径则在两种情况下都正确。

## 第三方依赖许可

`Cargo.lock` 共 **523 个包**（含本仓库的两个 workspace crate：`kugou-tui` 与 `libresoda-fork`；不按目标平台拆分）。去掉本仓库，剩 **521 个第三方依赖**，分布如下：

| 许可 | 数量 | 说明 |
|---|---|---|
| `MIT` / `Apache-2.0` 及其各种组合写法（含 BSD / ISC / Zlib / Unlicense / WTFPL / 0BSD / BSL-1.0 / CC0 / CDLA / LLVM-exception，以及 Unicode 相关的） | **508** | 宽松许可，二选一即可 |
| **`MPL-2.0`** | **13** | **弱 copyleft**：symphonia 系列（FLAC / MP3 / Vorbis / AAC 解码器）与 `option-ext`。文件级 copyleft；分发静态链接的二进制时需要保持其源码可获取 |
| manifest 里没有 `license` 字段 | **0** | — |

统计方式：`cargo metadata`（从 registry 索引与 manifest 读 `license`，**不依赖某个 crate 是否恰好在本机解包**）。复现时在仓库根目录跑一次：

```bash
python3 scripts/license-stats.py
```

> 早先这个脚本直接读 `$CARGO_HOME/registry/src/<name>-<version>/Cargo.toml`，于是「读不到」的数量随本地缓存漂移 —— 同一份 `Cargo.lock`，一次跑出 26 个读不到，另一次 102 个，数字无法复现（本文件里那张旧表正是被这个坑带偏的）。改用 `cargo metadata` 后，结果只取决于 `Cargo.lock`。

几点注意：

1. **MPL-2.0**：若以二进制分发本项目，需保证 symphonia 相关 MPL 代码的源码可获取（保留 `Cargo.lock` 与目标平台的依赖拉取方式即可满足）。另外 `termina` 是 `MIT OR MPL-2.0`，**可选**双许可，整体可按 MIT 用，不产生 copyleft 义务。
2. **`ring` / `aws-lc-rs` / `aws-lc-sys`**：加密库，随**非 Windows** 的 `reqwest` + `rustls` 引入（Windows 上 TLS 走系统 SChannel，见 `Cargo.toml`）。它们的许可是 `Apache-2.0 AND ISC`、`ISC AND (Apache-2.0 OR ISC)` 这类**合取**组合；`aws-lc-sys` 还叠加 BSD-3-Clause 与 MIT。这些库在部分司法辖区可能受出口管制；商业分发前请确认。
3. **`native-tls` / `openssl` / `openssl-sys` / `foreign-types` / `vcpkg`**：本表**不区分目标平台**，所以它们也在列表里 —— 它们是 `native-tls` 为**非 Windows、非 macOS** 目标声明的 OpenSSL 后端。本项目只在 `cfg(windows)` 下用到 `native-tls`（那里走 `schannel`，完全不碰 OpenSSL），Linux 构建根本不会编译这些包，所以实际产物里没有 OpenSSL 代码。
4. **提到 GPL / LGPL 的 3 个包**都是**可选**双许可，整体可按宽松许可使用，不产生 copyleft 义务：`self_cell`（`Apache-2.0 OR GPL-2.0-only`，随 ratatui-image 引入），以及两个版本的 `r-efi`（`MIT OR Apache-2.0 OR LGPL-2.1-or-later`）。

> 本页只做**分类统计**，不做许可兼容性判断。要更严格的检查请用 [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny) 或 [`cargo-about`](https://github.com/EmbarkStudios/cargo-about)。
