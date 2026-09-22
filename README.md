# kugou-tui

酷狗音乐的命令行 TUI 播放器：Rust 编写，单个二进制，常驻内存约 14 MiB。

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)
![Platform](https://img.shields.io/badge/platform-linux--macos--windows-lightgrey.svg)

下面是在 108×30 终端里的实际画面（歌单页，正在播放，歌词跟随进度滚动）：

```
╭kugou-tui──────────╮╭歌单广场─────────────────────────────────────────────────────────────────────────────▲
│── 导航 2/5        ││>    1 KPOP女团歌曲 夏日必备 by _                                                    ┃
│  搜索             ││     2 听过但不知道名字的神曲 by Jay                                                 │
│> 歌单             ││    3 提神醒脑！赶抄作业用的激萌电音 by lcen                                        │
│  歌手             ││     4 欧美流行：撩人旋律，入耳沉醉 by 安稳余生                                  │
│  排行榜           ││     5 享受顶级Rap吧！ by 産倫-                                                      │
│  云端             ││     6 抖音热歌2026：无脑循环，刷到失眠 by 爱听歌的啾啾                              │
│                   │╰─────────────────────────────────────────────────────────────────────────────────────▼
│── 连接            │╭KPOP女团歌曲 夏日必备 · 30 首────────────────────────────────────────────────────────▲
│  API              ││> *  1 Circus 太妍 (태연) Something New - The… 03:54                                 ┃
│127.0.0.1:3000     ││     2 WE GO fromis_9 9 WAY TICKET 02:55                                             │
│  登录 否          ││     3 Why 太妍 (태연) Why- The 2nd Mini A… 03:27                                    │
│  指纹 已获取      ││     4 All With You 太妍 (태연) 달의 연인 - 보보경… 03:53                            │
│                   │╰─────────────────────────────────────────────────────────────────────────────────────▼
│── 播放            │╭歌词 · Circus────────────────────────────────────────────────────────────────────────╮
│  模式 顺序        ││                                Circus - 태연 (太妍)                                 │
│  队列 30 首       ││                                     词：조윤경                                      │
│  音量 100%        ││                      曲：Denniz Jamm/Allison Kaplan/Le'mon                        │
│                   │╰─────────────────────────────────────────────────────────────────────────────────────╯
│── 缓存            │╭播放队列 · 30 首 · 顺序──────────────────────────────────────────────────────────────╮
│  已用 8.2 MiB     ││> *  1 Circus 太妍 (태연) Something New - The… 03:54                                 │
│  上限 512 MiB     ││     2 WE GO fromis_9 9 WAY TICKET 02:55                                             │
│                   ││     3 Why 太妍 (태연) Why- The 2nd Mini A… 03:27                                    │
╰───────────────────╯╰─────────────────────────────────────────────────────────────────────────────────────╯
╭播放──────────────────────────────────────────────────────────────────────────────────────────────────────╮
│>> 太妍 (태연) - Circus                                              播放中 · 音量 100% · 顺序         │
│██                                            00:03 / 03:54                                               │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 正在播放：太妍 (태연) - Circus                                                  [?] 帮助  [q] 退出
```

---

## 项目简介

`kugou-tui` 是一个终端里的酷狗音乐播放器。它**不是**音乐下载器，也不提供任何音乐内容——
它只做一件事：把 [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi)（第三方逆向封装的
酷狗接口服务）返回的数据，用一套键盘优先的终端界面呈现出来，并调用系统的音频设备播放。

需要说明的两点：

- **依赖本地 API 服务**。本程序不含任何接口实现，必须先在**本机**运行 KuGouMusicApi
  （默认 `http://127.0.0.1:3000`）。它只与本机的这个服务通信。
- **播放的是缓存文件**。取到直链后先下载到本地缓存目录，再由 [rodio](https://github.com/RustAudio/rodio)
  解码播放。因此一首歌的等待时间取决于网络与文件大小，而不是"边下边播"。

## 特性

| 分类 | 能力 |
|---|---|
| 浏览 | 歌单广场、歌手列表（可按地区筛选）、排行榜、个人云端歌单 |
| 检索 | 单曲搜索（登录后可搜）；结果按服务端相关性排序，`M` 加载更多 |
| 播放 | 播放 / 暂停 / 上一首 / 下一首 / 进度跳转（±5 秒）/ 音量（±5%）/ 静音 |
| 播放模式 | 顺序、列表循环、单曲循环、随机 |
| 歌词 | 逐字歌词（KRC）、译文 / 音译（多语言轨）、卡拉 OK 式居中滚动、时间偏移微调（±100 ms） |
| 逐字高亮 | 解析 KRC 的每字时间戳，按**每个字自己的进度**在底色与强调色之间插值——边界字是渐变过渡，仿 Apple Music 的推进；非当前行按距离线性变暗 |
| 可视化 | **真频谱**：对音频线程采集的采样做 FFT 后按对数分频（40Hz–16kHz），贝斯亮左、镲片亮右；非随机动画 |
| 播放队列 | 追加单曲（`a`）、插播下一首（`i`）、整列表加入（`A`）、移除单曲（`x`）、清空（`X`） |
| 云端歌单 | 收藏单曲（`s`）、整个队列同步（`S`）、从歌单移除（`d`）、删除歌单（`D`）、新建歌单（`N`） |
| 登录 | **应用内扫码**（`L`），二维码直接画在终端里，无需额外工具 |
| 输入 | 键盘 + 鼠标（点击选中、双击激活、滚轮、点进度条跳转） |
| 缓存 | 音频落盘缓存 + LRU 上限回收；`C` 一键清空 |
| 外观 | 6 套主题（冷蓝 / 石墨 / 日落 / 森林 / 霓虹 / 暗紫），各有真彩与 16 色两版 |
| 设置页 | `,` 打开：主题、音质、播放模式、刷新间隔、歌词偏移、每页条数、缓存上限、16 色、歌词面板、侧边导航、下载目录、封面铺满方式；改完立即落盘 |
| 单曲下载 | `W` 把当前播放的歌曲下载到设置里选的目录（默认 `~/Music`） |
| **桌面集成** | **MPRIS**：注册为 `org.mpris.MediaPlayer2.kugou-tui`，状态栏/媒体控件/`playerctl` 可直接控制并显示封面 |

支持的音频格式由 rodio 决定：**MP3、FLAC、M4A/MP4、OGG(Vorbis)、WAV**。
可选的播放音质见[配置文件](docs/CONFIGURATION.md)的 `quality`。

---

## 文档

完整说明拆成了几份，各管一块：

| 文档 | 内容 |
|---|---|
| [KEYBINDINGS.md](KEYBINDINGS.md) | 全部快捷键、鼠标操作、歌曲右键菜单 |
| [docs/USER_GUIDE.md](docs/USER_GUIDE.md) | 界面布局、音源配置、播放队列、登录与云端歌单、桌面集成 |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | 命令行参数、配置文件每一项、会话持久化 |
| [docs/FAQ.md](docs/FAQ.md) | 常见问题与排查 |
| [docs/DESIGN.md](docs/DESIGN.md) | 线程模型、低资源占用、接口适配 |


## 环境要求

| 项目 | 要求 |
|---|---|
| Rust 工具链 | **1.85+**（edition 2024） |
| Node.js | 用于运行 KuGouMusicApi（需 16+） |
| 音频输出 | 任意 rodio 支持的后端（Linux 上为 ALSA/PulseAudio） |
| 终端 | 支持 256 色与 UTF-8；老终端可加 `--basic-color` 退回 16 色 |
| D-Bus（可选） | 有 session bus 时自动启用 MPRIS 桌面集成；没有（纯 tty）则跳过，**不影响播放** |

---

## 安装

### 1. 启动 KuGouMusicApi

```bash
git clone https://github.com/MakcRe/KuGouMusicApi.git
cd KuGouMusicApi
# 锁定到经过验证的提交：上游是活跃仓库，接口字段会变，
# 直接跟 master 可能某天就解析不出歌名或歌词
git checkout a5a98013cce79fe0ae2ad65fc84b68176ebcfc1e
npm install
npm start          # 注意是 npm start，不是 npm run dev
```

> 概念版（lite）实例要带 `platform=lite` 启动：
> `platform=lite PORT=3001 npm start`
> 不加这个环境变量，概念版搜索会拿不到正确结果。

服务默认监听 `http://127.0.0.1:3000`。验证一下：

```bash
curl -s "http://127.0.0.1:3000/register/dev"
```

> `npm run dev` 走的是 nodemon（一个 devDependency），只装过生产依赖的环境里会直接失败。

> **只用「酷狗」音源的话，起这一个实例就够了。**
> 想用「酷狗概念版」音源，见[常见问题](docs/FAQ.md)里的「概念版音源怎么配」。

### 2. 编译本客户端

```bash
git clone https://github.com/sijin-xb/kugou-tui.git
cd kugou-tui
cargo build --release
./target/release/kugou-tui --help
```

release 产物约 **6.7 MiB**（`opt-level="z"` + fat LTO + strip）。

### 3.（可选）安装启动器脚本

仓库提供两个脚本：

| 脚本 | 作用 |
|---|---|
| `scripts/kugou-tui` | 启动播放器；API 服务没起就自动拉起并等待就绪 |
| `scripts/kugou-api` | 一次拉起**两个** API 实例（标准版 + 概念版） |

```bash
ln -s "$PWD/scripts/kugou-tui" "$PWD/scripts/kugou-api" ~/.local/bin/
```

只用一个音源时，装 `kugou-tui` 就够；两个音源都要用，再装 `kugou-api`：

```bash
kugou-api          # 启动两个实例（已在跑的会跳过）
kugou-api status   # 查看状态
kugou-api stop     # 停止
```

启动器用几个环境变量控制行为，都有默认值：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `KUGOU_API_BASE` | `http://127.0.0.1:3000` | API 服务地址 |
| `KUGOU_API_DIR` | `$HOME/KuGouMusicApi` | 服务所在目录，用于自动拉起 |
| `KUGOU_API_LOG` | `$XDG_CACHE_HOME/kugou-tui/api.log` | 服务日志路径 |
| `KUGOU_TUI_BIN` | 自动探测 | 手动指定二进制路径 |

---

## 快速上手

**不登录也能听歌**。搜索和云端歌单才需要账号。

1. 确认 KuGouMusicApi 已在跑（见上一步）。
2. 启动：`./target/release/kugou-tui`
3. 按 **`2`** 进入歌单广场 → `Enter` 打开一个歌单 → 移动光标 → `Enter` 播放。

想搜歌再按 **`L`** 扫码登录。

> 首次启动时程序会自动获取设备指纹 `dfid` 并写入配置。取播放直链需要它，
> 这一步是自动的，不需要你做任何事。

---

## 免责声明与合规提示

使用本项目前请务必阅读：

1. **仅供学习与技术研究的自用工具**，不提供、不托管、不分发任何音乐内容。
2. 本项目**不含任何接口实现**，全部数据来自第三方项目
   [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi)。本项目不拥有任何音乐版权，
   也不对该接口的可用性、稳定性与合法性负责。
3. **第三方服务合规风险**：酷狗（ KuGou / 腾讯音乐娱乐集团）的相关服务受其用户协议与
   当地法律法规约束。通过非官方接口访问可能违反其服务条款，存在账号被限制或终止的风险。
   本项目仅作为客户端调用你在**本机**自行部署的服务，不鼓励、不协助任何形式的
   批量下载、传播或商业使用。
4. **版权**：请尊重音乐版权，支持正版。播放过程中产生的缓存文件请在合理期限内清除
   （默认缓存上限 512 MiB，超出自动回收）。
5. **地域与法律**：禁止在违反当地法律法规的前提下使用本项目。使用者因使用本项目
   产生的一切后果，由使用者自行承担。
6. **隐私**：登录凭据（cookie）仅保存在你本机的配置文件中（权限 `0600`），
   除访问你自己部署的 API 服务外不会发送到任何地方。本项目无任何遥测。

## 致谢

- [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi) —— 本项目的接口来源。
  所有接口路径都对照其 `docs/README.md` 核对过。
- [MoeKoeMusic](https://github.com/MoeKoeMusic/MoeKoeMusic) —— 歌词解析、播放模式、
  云端歌单同步的设计参考。
- [KugouMusic.NET](https://github.com/Linsxyx/KugouMusic.NET) —— 接口封装思路参考。
- [ratatui](https://github.com/ratatui/ratatui) / [rodio](https://github.com/RustAudio/rodio) /
  [tokio](https://github.com/tokio-rs/tokio) —— 本项目的三块基石。
- [ratatui-image](https://github.com/benjajaja/ratatui-image) —— 封面渲染。它把图片
  写进 ratatui 的 Buffer 而不是自己写 stdout，并负责探测 kitty / iTerm2 / sixel
  协议（都不支持时退到彩色半块）。

## 许可

本项目采用 [MIT](LICENSE)（© 2026 kugou-tui contributors）。

### 第三方依赖许可

`Cargo.lock` 共 **468 个包**（含传递依赖，不区分目标平台）。已逐个核对 `license`
字段。分布如下（按包数排序，分类互斥）：

| 许可 | 数量 | 说明 |
|---|---|---|
| `MIT` / `Apache-2.0` 及其各种组合写法 | 427 | 宽松，任选其一即可 |
| `Unicode-3.0` / `Unicode-DFS-2016` | 21 | 宽松（Unicode 数据表与算法） |
| **`MPL-2.0`** | 13 | **弱著佐权**：symphonia 系列（FLAC / MP3 等解码器）与 `option-ext`。文件级 copyleft，静态链接分发需保留其源码可得性 |
| 其余宽松许可（`WTFPL`、`CDLA-Permissive-2.0` 等） | 7 | 宽松 |

需要注意的三点：

1. **MPL-2.0**：若以二进制形式分发本项目，需保证 symphonia 相关 MPL 代码的源码可得
   （保留 `Cargo.lock` 与目标平台的依赖获取方式即满足）。
2. **`ring` / `aws-lc-rs`**：加密库，许可为 `Apache-2.0 AND ISC` 等组合；在某些司法
   辖区可能涉及出口管制，商业分发前请自行确认。
3. **出现 GPL / LGPL 字样的 3 个包**均为**可选**双许可，整体按宽松许可使用即可，
   不构成著佐权义务：`self_cell`（`Apache-2.0 OR GPL-2.0-only`，随 ratatui-image
   引入）、`r-efi` 两个版本（`MIT OR Apache-2.0 OR LGPL-2.1-or-later`）。

> 许可信息由脚本从 `Cargo.lock` 与各依赖的 `Cargo.toml` 自动提取。依赖变动后建议
> 用 `cargo-deny` 或 `cargo about` 复核，本表可能滞后。
