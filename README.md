# kugou-tui

**在终端里听酷狗：逐字歌词、真实频谱、单进程二进制，常驻内存约 19 MiB。**

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.90%2B-orange.svg)
![Platform](https://img.shields.io/badge/platform-linux%20%7C%20windows-lightgrey.svg)
![crates.io](https://img.shields.io/crates/v/kugou-tui.svg)

![kugou-tui：歌单广场、正在播放、逐字歌词与播放队列](assets/screenshot-0.6.2.png)

*终端里的酷狗播放器。没有窗口、没有广告、没有推荐流：按 `2` 搜索，`Enter` 播放，按 `L` 扫码拉取云端歌单。*

## 这是什么

一个酷狗终端播放器，只做一件事：**找到歌、放出来、把歌词和频谱渲染好。**

1. **逐字歌词** —— 解析酷狗 KRC 格式的逐字时间戳，插值出每个字自己的进度，边界字带渐变过渡；不是 LRC 那种「整行一起亮」。
2. **真实频谱** —— 对音频线程上采到的样本做 FFT，按对数分频，覆盖 40 Hz–16 kHz。
3. **酷狗曲库 + 云端歌单** —— 不需要本地文件就能搜索播放；歌单广场、歌手、排行榜、个人云端歌单；登录二维码直接在终端里画出来。

## 安装

接口**内嵌在二进制里**（纯 Rust），默认路径不需要 Node.js。两种后端随时可切：

| | `--api native`（默认，内嵌） | `--api node`（回退） |
|---|---|---|
| 需要 Node.js | **否** | 是，≥ 12 |
| 常驻内存 | 约 19 MiB（单进程） | 本程序约 19 MiB **+ 独立的 node 服务 42–67 MiB** |
| 覆盖范围 | 酷狗标准版 / 概念版全部 | 同上，外加网易云与汽水音源的传输层 |

> **默认后端是 `native`**：酷狗接口在进程内实现，装完即用，不需要 Node.js。要切回 Node 服务（自建、跑在另一台机器上，或想对比实现），加 `--api node`，或在配置文件里设 `api_backend = "node"`。

**cargo**（推荐）：

```bash
cargo install kugou-tui
kugou-tui
```

内嵌后端启动即就绪，没有「首次运行先准备服务」这一步。想用 Node 回退（或网易云 / 汽水音源）时，首次运行会自动准备并拉起服务（下载 + 装依赖，约 25 秒），之后每次启动直接复用。想提前做这一步：`kugou-tui --api node --api-start`；想自己管服务：`--no-api-start`。

**构建一个永不包含 Node 引导代码的二进制：**

```bash
cargo build --release --no-default-features
```

关掉 `node-bootstrap` feature 后，引导代码（探测端口、下载 KuGouMusicApi、`npm install`、拉起 `node app.js`）完全不进编译，二进制里也没有。此时 `--api node` 会明确报「该 feature 已在构建时禁用」，而不是含糊的连接失败。体积差约 0.06 MiB（见 [docs/DESIGN.md](docs/DESIGN.md)）。

**预编译包**：[Releases](https://github.com/sijin-xb/kugou-tui/releases) 提供三个平台，附带脚本与全部文档。

```bash
# Linux
tar xzf kugou-tui-<version>-x86_64-unknown-linux-gnu.tar.gz
cd kugou-tui-<version>-x86_64-unknown-linux-gnu
./scripts/kugou-tui --api native      # 内嵌后端，不需要 Node
./scripts/kugou-api-install kugou     # 只在要用 --api node 回退时装
```

```powershell
# Windows
Expand-Archive kugou-tui-<version>-x86_64-pc-windows-msvc.zip -DestinationPath .
cd kugou-tui-<version>-x86_64-pc-windows-msvc
.\scripts\kugou-tui.ps1 --api native
.\scripts\kugou-api-install.ps1       # 只在要用 --api node 回退时装
```

**从源码构建**：

| 平台 | 前置 |
|---|---|
| Linux | Rust 1.90+、`alsa-lib` 开发头文件 + `pkg-config` |
| Windows | Rust 1.90+、MSVC 工具链（不需要 CMake / NASM / OpenSSL） |
| macOS | Rust 1.90+、Xcode 命令行工具（不在真机上长期使用） |

```bash
git clone https://github.com/sijin-xb/kugou-tui.git
cd kugou-tui && cargo build --release
```

启动器脚本、网易云音源、各平台的详细前置：**[docs/INSTALL.md](docs/INSTALL.md)**。Node.js 只是 `--api node` 回退路径的前提，内嵌后端不碰它。

## 快速开始

```bash
kugou-tui
```

按 `3` 进歌单广场，`Enter` 打开歌单，移动光标，`Enter` 播放。浏览和播放不需要登录；搜索和云端歌单需要（按 `L` 扫码）。

## 功能总览

| 类别 | 能力 |
|---|---|
| 浏览 | 歌单广场、歌手列表（可按地区筛选）、排行榜、个人云端歌单 |
| 搜索 | 单曲搜索；`M` 加载更多 |
| 播放 | 播放/暂停、上一首/下一首、±5 秒跳转、音量、静音；顺序 / 列表循环 / 单曲循环 / 随机 |
| 歌词 | 逐字高亮（KRC）、翻译与音译、居中滚动、行间淡入淡出、点行跳转、±100 ms 偏移 |
| 可视化 | 真实频谱：FFT + 对数分频 |
| 队列 | 追加（`a`）、下一首播放（`i`）、整列表加入（`A`）、移除（`x`）、清空（`X`） |
| 云端歌单 | 收藏单曲（`s`）、同步整个队列（`S`）、增删歌单（`N` / `D`） |
| 登录 | 应用内扫码（`L`），二维码在终端里画出来 |
| 桌面集成 | MPRIS（可用 `playerctl` 控制）+ 系统托盘（菜单 / 滚轮调音量 / 图标随状态变暗）；**仅 Linux 桌面** |
| 输入与外观 | 键盘 + 鼠标；6 套主题（真彩与 16 色两种版本）；音频落盘缓存并按 LRU 逐出 |

完整按键见 [KEYBINDINGS.md](KEYBINDINGS.md)，其余见 [docs/USER_GUIDE.md](docs/USER_GUIDE.md)。

## 与 cmus / mpd 的对比

|  | **kugou-tui** | cmus | mpd + ncmpcpp |
|---|---|---|---|
| **曲库** | **酷狗在线曲库**：搜索即播，不需要本地文件 | 仅本地文件 | 仅本地文件 |
| **逐字歌词** | **有**：KRC 逐字时间戳，高亮按字推进 | 整行 LRC | 整行 LRC |
| **频谱** | **内置**，零配置 | 无 | 有，但要配 mpd 的 fifo 输出 |
| **云端歌单** | **有**：终端内扫码登录 | 无 | 无 |
| **常驻内存** | 约 19 MiB（`--api native`，单进程，本机实测） | 通常 10–25 MiB | 通常 15–30 MiB |
| **形态** | 单进程、单二进制（约 11.7 MiB） | 单进程 | 客户端 / 服务端分离 |
| **需要自建服务** | **否**（`--api native` 接口在进程内；只有 `--api node` 回退才用本地服务） | 否 | 是（mpd） |

> 内存一行是本机实测（release 构建、112×34 终端、`--no-tray`，读 `/proc/<pid>/status` 的 `VmRSS`）；方法与拆解见 [docs/DESIGN.md](docs/DESIGN.md)。cmus / mpd 两行是社区常见值，非本机实测。相比迁移前的版本，`--api native` 去掉了独立的 Node 进程（它自己就占 42–67 MiB），代价是本程序自身约 +0.4 MiB。
>
> 一句话：**cmus / mpd 放你有的，kugou-tui 放你想听的。**

## 依赖什么

**`--api native`（内嵌后端）：程序自己就是接口实现。** 搜索、取流、歌词解密（KRC）、设备指纹、扫码登录、云端歌单读写全部是同进程内的纯 Rust —— 没有子进程，机器上不需要 Node.js。请求 URL、请求头、参数顺序、签名算法逐点移植自上游 [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi)，并有已知答案测试（KAT）覆盖。

**`--api node`（回退）：** 搜索、取流、歌词、云端歌单都走本地第三方服务 [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi)（独立的 Node.js 仓库）。启动时探测一次端口，没有就自动准备并拉起服务，且只停自己启动的那个实例 —— 所以除了 Node.js，不需要手工部署任何东西。想手动部署，或要部署网易云服务，见 [docs/INSTALL.md](docs/INSTALL.md#部署第三方接口服务)。

Node 回退的引导代码可以用 `--no-default-features` 整个去掉（见[安装](#安装)）。

## 内嵌 WebSocket 服务

`--api native` 与 `--api node` 都会在启动时开一个 WebSocket 服务，供第三方客户端读取播放状态与歌词、并控制播放（协议兼容 [MoeKoeMusic](https://github.com/MoeKoeMusic/MoeKoeMusic)；文档见 [music.moekoe.cn/zh-CN/websocket-api.html](https://music.moekoe.cn/zh-CN/websocket-api.html)）：

| 项 | 值 |
|---|---|
| 默认地址 | `ws://127.0.0.1:6520/` |
| 绑定 | **仅 `127.0.0.1`**，非回环连接一律拒绝 |
| 关闭 | `--no-ws`，或配置里 `ws = false` |
| 改端口 | `--ws-port <PORT>`，或配置里 `ws_port = 6520` |

服务推送 `welcome` / `lyrics` / `playerState`，接受 `control` 命令（`toggle` / `next` / `prev`）。握手会校验 `Origin`：带浏览器 `Origin` 的跨站握手被拒（403），本地不带 `Origin` 的客户端放行。入站消息与单帧上限 64 KiB。

## 文档

| 文档 | 内容 |
|---|---|
| [docs/INSTALL.md](docs/INSTALL.md) | 环境要求、各平台安装路径、接口服务部署、启动器脚本与环境变量 |
| [KEYBINDINGS.md](KEYBINDINGS.md) | 全部快捷键、鼠标操作、歌曲右键菜单 |
| [docs/USER_GUIDE.md](docs/USER_GUIDE.md) | 界面布局、音源配置、播放队列、登录与云端歌单、桌面集成 |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | 命令行参数、配置文件每一项、会话持久化 |
| [docs/FAQ.md](docs/FAQ.md) | 常见问题与排障 |
| [docs/DESIGN.md](docs/DESIGN.md) | 线程模型、边下边播、低资源占用、接口适配（含测量方法） |
| [docs/MAINTENANCE.md](docs/MAINTENANCE.md) | 维护手册：模块地图、风险点、已知的坑、怎么验证改动 |
| [docs/LICENSES.md](docs/LICENSES.md) | 第三方依赖许可证分析 |
| [docs/RELEASE.md](docs/RELEASE.md) | 发版流程、产物清单、版本与标签规则 |
| [CHANGELOG.md](CHANGELOG.md) | 更新日志 |

## 许可与免责

[MIT](LICENSE) © 2026 kugou-tui contributors。

**仅供学习与技术研究的自用工具。** 不提供、不托管、不分发任何音乐内容。请尊重音乐版权、支持正版服务；访问非官方接口可能违反酷狗服务条款，风险自负。完整条款见 [docs/DISCLAIMER.md](docs/DISCLAIMER.md)。

## 致谢

- [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi) —— 本项目接口实现的来源。
- [MoeKoeMusic](https://github.com/MoeKoeMusic/MoeKoeMusic) —— 歌词解析、播放模式、云端歌单同步的设计参考；「每日领取概念版 VIP」两步流程也来自它。
- [libresoda](https://github.com/sodahub-org/libresoda) —— 汽水音乐数据层（`crates/libresoda`），官方 SodaM 客户端用的同一个库。
- [ratatui](https://github.com/ratatui/ratatui) / [rodio](https://github.com/RustAudio/rodio) / [tokio](https://github.com/tokio-rs/tokio) / [ratatui-image](https://github.com/benjajaja/ratatui-image) —— 界面、音频、异步运行时与封面渲染。
