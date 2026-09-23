# kugou-tui

酷狗音乐的命令行 TUI 播放器：Rust 编写，单个二进制，常驻内存约 15 MiB。

![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.86%2B-orange.svg)
![Platform](https://img.shields.io/badge/platform-linux-lightgrey.svg)

下面是在 **108×30 终端里的真实截图**（歌单页，正在播放，歌词跟随进度滚动）。
界面由程序渲染，数据来自模拟后端——**不含任何真实音乐内容**，也没有封面图
（所以歌词面板上方没有专辑缩略图）。

```
╭kugou-tui─────────────╮ ╭歌单广场─────────────────────────────────────────────────────────────────────────▲
│ kugou-tui 3/10       │ │ >    1 KPOP女团歌曲 夏日必备 30 首 · by _                                       ┃
│ ── 正在播放 ──       │ │      2 听过但不知道名字的神曲 30 首 · by Jay                                ┃
│   1  首页           │ │      3 提神醒脑！赶抄作业用的激萌电音 30 首 · by lcen                      ┃
│   0  可视化         │ │      4 欧美流行：撩人旋律，入耳沉醉 30 首 · by 安稳余生                     │
│ ── 发现 ──           │ │      5 享受顶级Rap吧！ 30 首 · by 産倫-                                      │
│   2  搜索           │ ╰─────────────────────────────────────────────────────────────────────────────────▼
│ > 3  歌单           │ ╭KPOP女团歌曲 夏日必备 · 6 首─────────────────────────────────────────────────────▲
│   4  歌手           │ │ >    1 11:11 太妍 (태연) 11:11 03:43                                            ┃
│   5  排行榜         │ │      2 Fine 太妍 (태연) My Voice - The 1s… 03:38                                ┃
│ ── 我的 ──           │ │      3 All With You 太妍 (태연) 달의 연인 - 보보… 03:53                         │
│   6  云端           │ │      4 Why 太妍 (태연) Why - The 2nd Min… 03:27                                 │
│   7  队列           │ ╰─────────────────────────────────────────────────────────────────────────────────▼
│ ── 设置 ──           │ ╭歌词 · Circus────────────────────────────────────────────────────────────────────╮
│   8  音源           │ │                               谁能体谅我                                    │
│   9  设置           │ │                             的不安                                     │
│ ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁ │ │                              让我再想一遍                                   │
│                      │ ╰─────────────────────────────────────────────────────────────────────────────────╯
│ ── 连接 · 已连通     │ ╭播放队列 · 4 首 · 顺序───────────────────────────────────────────────────────────╮
│   API 127.0.0.1:3000 │ │   *  1 Circus 太妍 (태연) Something New - T… 03:54                              │
│   音源 酷狗概念版    │ │      2 WE GO fromis_9 9 WAY TICKET 02:55                                        │
│ … 另有 播放 / 缓存   │ │      3 Why 太妍 (태연) Why - The 2nd Min… 03:27                                 │
╰──────────────────────╯ ╰─────────────────────────────────────────────────────────────────────────────────╯

╭播放──────────────────────────────────────────────────────────────────────────────────────────────────────╮
│  Circus · 太妍 (태연) · Something New - The 1st Mini Album    下一首 WE GO · 播放中 · 音量 100% · 顺序 │
│ █████████████████                            00:14 / 01:30                                               │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────╯

 《KPOP女团歌曲 夏日必备》共 6 首                                              [?] 帮助 [q] 退出
```

> 侧边栏在矮终端里装不下全部状态块（30 行时只能显示「连接」），最后一行会写明
> 还有哪些没显示——上图末尾的 `… 另有 播放 / 缓存` 就是。**能放几行放几行**，
> 不会静默裁掉。高度够时不出现这一行。

---

## 项目简介

`kugou-tui` 是一个终端里的酷狗音乐播放器。它**不是**音乐下载器，也不提供任何音乐内容——
它只做一件事：把 [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi)（第三方逆向封装的
酷狗接口服务）返回的数据，用一套键盘优先的终端界面呈现出来，并调用系统的音频设备播放。

需要说明的两点：

- **依赖本地 API 服务**。本程序不含任何接口实现，必须先在**本机**运行 KuGouMusicApi
  （默认 `http://127.0.0.1:3000`）。它只与本机的这个服务通信。
- **边下边播**。取到直链后先攒够开头（128 KB，约 8 秒音频）就开播，剩下的在后台
  继续下，同时落盘缓存。所以首次播放的等待是「攒开头」而不是「下完整首」；同一首
  第二次播放直接命中缓存，零等待。解码由 [rodio](https://github.com/RustAudio/rodio)
  完成，读指针跑到还没下到的位置时会阻塞等数据——表现是声音停一下，而不是提前结束。

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
| 概念版 VIP | **每日自动领取**（启动时 / 登录后 / 切到概念版时各试一次，一天只领一次），`V` 手动重试；`我的资料`里显示领取状态，鼠标点那一行也行 |
| 输入 | 键盘 + 鼠标（点击选中、双击激活、滚轮、点进度条跳转） |
| 缓存 | 音频落盘缓存 + LRU 上限回收；`C` 一键清空 |
| **网络** | **瞬时故障自动重试**：连接被拒 / 连接被重置 / 响应体截断 / 408 / 429 / 5xx 会重试（共 3 次尝试，间隔 300ms → 900ms）；超时、业务错误码、其它 4xx 不重试。写操作（收藏 / 删歌单 / 领 VIP）一律不重试 |
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
| Rust 工具链 | **1.86+**（edition 2024）。下限由 `ratatui-image` 11.x 决定，见 `Cargo.toml` |
| Node.js | 用于运行 KuGouMusicApi（上游 `engines` 要求 **12+**） |
| 音频输出 | 任意 rodio 支持的后端（Linux 上为 ALSA/PulseAudio） |
| 终端 | 支持 UTF-8；**真彩（24 位）** 才能看到完整的主题配色与逐字渐变，老终端可加 `--basic-color` 退回 16 色 |
| 操作系统 | **Linux**（在 CachyOS 上实测）。macOS 未验证但理论上可行；**Windows 不行**——`zbus` 在 Windows 上要求 `async-io` 特性，而这里按 `default-features = false` 只开了 `tokio` |
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

release 产物约 **6.8 MiB**（`opt-level="z"` + fat LTO + strip）。

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
kugou-api start     # 启动两个实例（已在跑的会跳过），打印 PID / 日志路径 / 访问地址
kugou-api status    # 查看状态（运行中会显示 PID；端口被别人占着也会如实说明）
kugou-api restart   # 先停再起（改了端口/配置后用它）
kugou-api stop      # 停止（按 PID 精确停止）
kugou-api help      # 完整说明
```

`start` 之前会做一轮预检，缺什么补什么：`node`、KuGouMusicApi 目录、
`node_modules`（缺了自动 `npm install --omit=dev`）、客户端二进制（缺了自动
`cargo build --release`）。任一步失败都会带着明确原因终止，不会留一个半死不活的进程。
不想让它碰编译就设 `KUGOU_API_SKIP_BUILD=1`（`stop` / `status` 本来就不触发编译）。

参数按**环境变量 > 配置文件 > 默认值**取值。配置文件是可选的
`~/.config/kugou-tui/api.env`，每行一个 `KEY=VALUE`：

```bash
# 临时换端口起一次，不动任何文件
KUGOU_STANDARD_PORT=3100 KUGOU_LITE_PORT=3101 kugou-api restart
```

`kugou-api` 认这些（配置文件里写同名键）：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `KUGOU_API_DIR` | `$HOME/KuGouMusicApi` | 服务所在目录 |
| `KUGOU_API_LOG_DIR` | `$XDG_CACHE_HOME/kugou-tui` | 日志与 PID 文件目录 |
| `KUGOU_API_HOST` | `127.0.0.1` | 监听地址 |
| `KUGOU_STANDARD_PORT` | `3000` | 标准版端口 |
| `KUGOU_LITE_PORT` | `3001` | 概念版端口 |
| `KUGOU_API_BIN` | `<仓库>/target/release/kugou-tui` | 客户端二进制路径 |
| `KUGOU_API_SKIP_BUILD` | 空 | 设为 `1` 跳过编译预检 |
| `KUGOU_API_CONFIG` | `$XDG_CONFIG_HOME/kugou-tui/api.env` | 配置文件路径 |

`scripts/kugou-tui`（播放器启动器）认的是另一组：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `KUGOU_API_BASE` | **不设置** | 设了才给程序传 `--api-base`，并拿它探活。不设时让程序**读配置里选中的音源**——否则每次都被强行拉回标准版 `:3000`，用概念版的人得按 `v` 切两次才回得去 |
| `KUGOU_API_DIR` | `$HOME/KuGouMusicApi` | 服务所在目录，用于自动拉起 |
| `KUGOU_API_LOG` | `$XDG_CACHE_HOME/kugou-tui/api.log` | 服务日志路径 |
| `KUGOU_TUI_BIN` | 自动探测 | 手动指定二进制路径 |
| `API_PORT` | `3000` | 自动拉起服务时用的端口 |

---

## 快速上手

**不登录也能听歌**。搜索和云端歌单才需要账号。

1. 确认 KuGouMusicApi 已在跑（见上一步）。
2. 启动：`./target/release/kugou-tui`
3. 按 **`3`** 进入歌单广场 → `Enter` 打开一个歌单 → 移动光标 → `Enter` 播放。

数字键落点是 `1` 首页、`2` 搜索、`3` 歌单、`4` 歌手、`5` 排行榜、`6` 云端、
`7` 队列、`8` 音源、`9` 设置、`0` 可视化（完整表见 [KEYBINDINGS.md](KEYBINDINGS.md)）。

想搜歌（`2`）得先登录，按 **`L`** 扫码即可。

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
  云端歌单同步的设计参考；「每日领取概念版 VIP」的两步流程（领一天 → 升级为畅听 VIP）
  也来自它的 `getVip()`。
- [KugouMusic.NET](https://github.com/Linsxyx/KugouMusic.NET) —— 接口封装思路参考。
- [ratatui](https://github.com/ratatui/ratatui) / [rodio](https://github.com/RustAudio/rodio) /
  [tokio](https://github.com/tokio-rs/tokio) —— 本项目的三块基石。
- [ratatui-image](https://github.com/benjajaja/ratatui-image) —— 封面渲染。它把图片
  写进 ratatui 的 Buffer 而不是自己写 stdout，并负责探测 kitty / iTerm2 / sixel
  协议（都不支持时退到彩色半块）。

## 许可

本项目采用 [MIT](LICENSE)（© 2026 kugou-tui contributors）。

### 第三方依赖许可

`Cargo.lock` 共 **469 个包**（含本仓库自己；不区分目标平台）。用
[`scripts/license-stats.py`](scripts/license-stats.py) 从 `Cargo.lock` 与各依赖的
`Cargo.toml` 逐个提取 `license` 字段统计得出，分布如下：

| 许可 | 数量 | 说明 |
|---|---|---|
| `MIT` / `Apache-2.0` 及其各种组合写法（含 BSD / ISC / Zlib / Unlicense / WTFPL / 0BSD / BSL-1.0 / CC0 / CDLA / LLVM-exception，以及 Unicode 相关） | 429 | 宽松，任选其一即可 |
| **`MPL-2.0`** | 13 | **弱著佐权**：symphonia 系列（FLAC / MP3 / Vorbis / AAC 等解码器）与 `option-ext`。文件级 copyleft，静态链接分发需保留其源码可得性 |
| 本机读不到 `license` 字段 | 26 | Windows / Android / macOS 专属包（`winapi*`、`windows*`、`jni`、`ndk-sys`、`objc2-*`、`simd_cesu8`）。Linux 上根本不会下载，所以没有源码可读 |

需要注意的三点：

1. **MPL-2.0**：若以二进制形式分发本项目，需保证 symphonia 相关 MPL 代码的源码可得
   （保留 `Cargo.lock` 与目标平台的依赖获取方式即满足）。另外 `termina` 是
   `MIT OR MPL-2.0`，属于**可选**双许可，整体按 MIT 用即可，不构成著佐权义务。
2. **`ring` / `aws-lc-rs` / `aws-lc-sys`**：加密库（随 `reqwest` + `rustls` 引入），
   许可是 `Apache-2.0 AND ISC`、`ISC AND (Apache-2.0 OR ISC)` 这类组合；
   `aws-lc-sys` 还叠了 BSD-3-Clause 与 MIT。这些库在某些司法辖区可能涉及出口管制，
   商业分发前请自行确认。
3. **出现 GPL / LGPL 字样的 3 个包**均为**可选**双许可，整体按宽松许可使用即可，
   不构成著佐权义务：`self_cell`（`Apache-2.0 OR GPL-2.0-only`，随 ratatui-image
   引入）、`r-efi` 两个版本（`MIT OR Apache-2.0 OR LGPL-2.1-or-later`）。

> 依赖变动后重新跑一次 `python3 scripts/license-stats.py` 即可复核本表。
> 想更严格的话用 `cargo-deny` 或 `cargo about`。
