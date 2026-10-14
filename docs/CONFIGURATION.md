# 配置

## 命令行参数

优先级：CLI > 环境变量 > 配置文件 > 内置默认。

| 参数 | 环境变量 | 说明 |
|---|---|---|
| `--api <node\|native>` | `KUGOU_API_BACKEND` | 由哪个后端提供酷狗接口。默认 `native`（内嵌纯 Rust，不需要 Node.js），见下文 |
| `-a`, `--api-base <URL>` | `KUGOU_API_BASE` | KuGouMusicApi 地址，默认 `http://127.0.0.1:3000`（仅在 `--api node` 下生效） |
| `-c`, `--cookie <COOKIE>` | `KUGOU_COOKIE` | 登录凭据，如 `token=xxx; userid=xxx` |
| `-s`, `--search <KEYWORDS>` | — | 启动后立即搜索这些关键词 |
| `--volume <0-100>` | — | 初始音量 |
| `--cache-dir <DIR>` | — | 音频缓存目录 |
| `--cache-limit <MiB>` | — | 缓存上限，`0` 表示不限 |
| `--tick-ms <MS>` | — | 刷新间隔，50–5000；值越大 CPU 占用越低 |
| `--page-size <N>` | — | 搜索结果与歌单广场每页条数，5–200 |
| `--proxy <URL>` | `KUGOU_PROXY` | 访问接口服务所用的 HTTP 代理 |
| `--sodam-cookie <COOKIE>` | `SODAM_COOKIE` | 汽水音乐登录态，如 `sessionid_ss=…; sessionid=…` |
| `--sodam-device-id <ID>` | `SODAM_DEVICE_ID` | 汽水设备指纹（使用签名服务时**可以为空**） |
| `--sodam-iid <ID>` | `SODAM_IID` | 汽水安装 id |
| `--sodam-signer-url <URL>` | `QISHUI_SIGNER_URL` | App 签名服务 URL；留空用内置**公共默认**，`none` 关闭签名 |
| `--sodam-signer-token <TOKEN>` | `QISHUI_SIGNER_TOKEN` | 签名服务的 Bearer token（公共地址留空） |
| `--sodam-x-helios <VALUE>` | `SODAM_X_HELIOS` | 手工抓包得到的静态签名头（使用签名服务时不生效） |
| `--sodam-x-medusa <VALUE>` | `SODAM_X_MEDUSA` | 同上，成对使用 |
| `--basic-color` | — | 老终端使用的固定 16 色配色 |
| `--no-tray` | — | 不注册系统托盘图标（配置里写 `tray = false` 可永久关闭） |
| `--no-ws` | — | 不开启内嵌 WebSocket 服务（配置里写 `ws = false` 可永久关闭） |
| `--ws-port <PORT>` | — | WebSocket 监听端口，默认 6520；仍只绑 `127.0.0.1` |
| `--no-api-start` | `KUGOU_API_AUTO_START=0` | 不自动拉起本地接口服务（由你自己管理；端口无人监听时报错退出） |
| `--api-start` | — | 启动本地接口服务后退出，不进入界面；服务**留在后台**，用 `--api-stop` 停止 |
| `--api-stop` | — | 停止由本程序启动的本地接口服务 |
| `--print-config` | — | 打印生效配置以及缓存/日志路径后退出 |
```bash
kugou-tui --print-config        # 打印生效配置与路径
kugou-tui --tick-ms 1000        # 省电：空闲 CPU 接近零
kugou-tui -s "海阔天空"          # 启动即搜索（需要登录）
```

### 没有命令行开关的环境变量

| 变量 | 作用 |
|---|---|
| `KUGOU_TUI_CONFIG_DIR` | **覆盖配置根目录。** 设为某路径后，配置读写于 `<path>/config.toml` 而非 `~/.config/kugou-tui`。适合便携安装；测试用它把状态隔离到临时目录。缓存目录不受影响，仍可用 `--cache-dir` 单独设置 |
| `KUGOU_TUI_NERD_FONT` | `1` 强制按已装 Nerd Font 渲染图标，`0` 强制按未装渲染 ASCII。留空为自动探测（Linux 下查 `fc-list`；**Windows 没有 fontconfig，自动探测一律视为未安装**） |
| `KUGOU_TUI_DEBUG` | `1` 打开 DEBUG 级日志（按键、进度同步这类逐帧日志默认关闭，否则会淹没日志） |
| `KUGOU_TUI_MEM_TRACE` | `1` 每 5 秒把 RSS 和当前曲目写进日志，用于诊断长时间播放的内存增长 |
| `KUGOU_API_DIR` | 本地 KuGouMusicApi 的目录。设置后**不再自动搜索或下载**；路径错误立即报错 |
| `KUGOU_API_AUTO_START` | `0` / `false` / `no` / `off` 关闭接口服务的自动拉起 |
```powershell
# Windows：便携目录 + 启用 Nerd Font 图标
$env:KUGOU_TUI_CONFIG_DIR = "D:\apps\kugou-tui\config"
$env:KUGOU_TUI_NERD_FONT  = "1"
.\kugou-tui.exe
```

## 配置文件

首次运行时自动生成。Linux/macOS 位于 `~/.config/kugou-tui/config.toml`，Windows 位于 `%APPDATA%\kugou-tui\config.toml`（Unix 下权限 `0600`，因为可能含 cookie）。缓存与日志在缓存目录：Linux 为 `~/.cache/kugou-tui/`，Windows 为 `%LOCALAPPDATA%\kugou-tui\`。不确定实际读的是哪个文件时用 `--print-config`。
```toml
api_backend = "native"              # native（默认，内嵌）| node（回退到本地服务）
api_base = "http://127.0.0.1:3000"  # 仅在 --api node 下生效
cookie = "token=xxx; userid=xxx"   # 登录态，可手写
dfid = "..."                        # 设备指纹，首次启动自动获取
volume = 0.7                        # 0.0–1.0
audio_device = "..."                # 输出设备名，留空 = 跟随系统默认
playback_mode = "sequential"        # sequential | repeat_all | repeat_one | shuffle
cache_dir = "/home/you/.cache/kugou-tui"
cache_limit_mib = 512               # 0 = 不限
lyric_offset_ms = 0                 # 全局歌词偏移，正数 = 延后
lyric_anim_ms = 200                 # 行淡入淡出最长时长，0 = 关闭
tick_ms = 200                       # 界面刷新间隔（毫秒）
page_size = 30                      # 搜索结果与歌单广场每页条数
proxy = "http://127.0.0.1:7890"     # 可选
quality = "128"
basic_color = false
theme = "default"                   # 见「其他设置」
download_dir = "~/Music"            # 单曲下载目录，可为空
qr_aspect = 2.0                     # 终端字符高宽比，见下文
lite_mode = false                   # 见下文
cover_fill = "crop"                 # 首页封面填充方式，见下文
tray = true                         # 系统托盘，见下文
ws = true                           # 内嵌 WebSocket 服务，只监听 127.0.0.1
ws_port = 6520                      # WebSocket 端口（与 MoeKoeMusic 一致）
api_auto_start = true               # 服务未运行时自动拉起，见下文
api_dir = ""                        # 本地 KuGouMusicApi 目录，留空 = 自动探测

# ---- 各音源的连接与身份，以及当前音源 ----
# 四个音源相互独立：登录态与设备身份**不能跨音源复用**（见使用指南的音源章节）。
[sources]
active = "kugou"                   # kugou | kugou_concept | netease | sodam
[sources.kugou]
enabled = true
api_base = "http://127.0.0.1:3000"  # 本地 KuGouMusicApi（标准版）
cookie = "token=…; userid=…"        # 该音源自己的登录态
device_id = "…"                      # 该音源自己的 dfid
priority = 0                         # 数字越小越靠前
[sources.kugou_concept]
enabled = true
api_base = "http://127.0.0.1:3001"  # 本地 KuGouMusicApi（platform=lite）
priority = 10
[sources.netease]
enabled = true
api_base = "http://127.0.0.1:3002"  # 本地 NeteaseCloudMusicApi
priority = 20
[sources.sodam]
enabled = true
api_base = "https://api.qishui.com" # 汽水**直连公网**，不需要本地服务
cookie = "sessionid_ss=…; sessionid=…"
priority = 30
# device_id = "…"                    # 使用签名服务时可为空，只有手工抓包才需要

# 汽水的签名服务：决定 VIP 完整曲目与无损是否可用。没有签名时服务端返回
# 「HTTP 200 + 空 body」，看起来像会员没生效，所以这项值得先查。
# 留空 = 用上游内置的公共签名服务（开箱可用）；也可换自建地址，或写 "none" 关闭签名。
# signer_url = "http://127.0.0.1:8899/sign"
# signer_token = "仅在服务启用鉴权时需要"
# 以下仅**手工抓包**路径需要（用签名服务时留空）：device_id / iid / fp 留空 = 用签名
# 服务自己的设备身份；x_helios / x_medusa 是抓来的静态签名头，会过期。
[sources.sodam_app]
```

### 酷狗接口后端（`api_backend`）

默认 `native`：标准版与概念版酷狗接口在进程内用纯 Rust 实现，**不需要 Node.js**，开箱即用，没有「首次运行准备服务」这一步。`node` 是回退方案：使用本地（或远程）KuGouMusicApi 服务，保留给自建服务与对比内嵌实现差异的场景。两个后端共用登录态与设备指纹（`cookie` / `dfid`），**切换无需重新登录**；该开关只影响两个酷狗音源。
```bash
kugou-tui --api node            # 临时切回 Node 服务
kugou-tui --api native          # 临时切回内嵌（等同默认）
```

### 接口服务的自动拉起

> **仅适用于 `--api node` 回退方案（以及网易云音源）。** 默认的内嵌后端（`--api native`）在进程内实现接口，既不需要本地服务也不需要 Node.js。

`--api node` 使用第三方 KuGouMusicApi 服务（Node.js 编写），`cargo install` 无法携带，因此程序在**运行时**自行准备：启动时探测一次端口，已有服务则复用（退出时也不动它），没有才动手。查找顺序：配置的 `api_dir` → `/usr/share/kugou-tui/api/kugou`（发行版打包）→ `~/.local/share/kugou-tui/api/kugou` → `~/KuGouMusicApi`；都不存在则下载固定 commit 并执行 `npm install --omit=dev`。要求机器上有 Node.js（≥ 12）与 npm；这样启动的服务随程序退出而停止，不留常驻进程。要让服务常驻（例如另一台机器或 systemd 管理），设 `api_auto_start = false` 或 `--no-api-start`；要预装好以后秒起，用 `kugou-tui --api node --api-start`；服务目录在别处则设 `api_dir` 或 `KUGOU_API_DIR`。要从二进制里去掉这段引导代码，用 `--no-default-features` 构建（见 [README](../README.md#安装)）。

### 封面填充（`cover_fill`）

封面区域是若干列 × 行，几乎不会正好是像素正方形，而专辑图通常是正方形：填满、不变形、不裁剪三者只能同时满足两个。

| 取值 | 填满 | 变形 | 裁剪 | 说明 |
|---|---|---|---|---|
| `crop`（默认） | 是 | 否 | 是 | 居中裁剪，等同 CSS `object-fit: cover` |
| `stretch` | 是 | 是 | 否 | 拉伸到区域尺寸；正方形图会被横向拉宽 |
| `fit` | 否 | 否 | 否 | 图保持完整，但区域比图宽时两侧露出背景 |

可在设置页（`,`）运行时切换，立即生效并写盘。

### 歌词动画（`lyric_anim_ms`）

换行时淡入淡出的最长时长（默认 `200`，`0` = 关闭）。终端没有亚字符定位，上滚只能整行跳变，唯一能做过渡的维度是**颜色**：切换时旧行仍亮、新行未亮，两者交叉淡变，其余行亮度平滑重排。实际时长随行距自适应：`min(本值, 本行到下一行的间隔 × 0.55)` —— 快歌里一行只有几百毫秒，用上限会显得拖沓。三种情况**强制关闭过渡**：

| 情况 | 原因 |
|---|---|
| `lite_mode = true` | 它的卖点就是少重绘 |
| `basic_color = true` | 16 色没有中间层级，淡变退化成过半硬切，比不做动画更难看 |
| 一次跨 3 行以上 | 那是拖动进度条或点击歌词行跳转，逐行淡变又慢又晕 |

### 输出设备（`audio_device`）

默认**跟随系统默认**（本字段留空）。设置页最后一项显示**实际打开的声卡**，可在「系统默认」与枚举出的设备之间切换。

这项存在是因为一个典型 Linux 故障：进度条在走、状态是「播放中」却没有声音 —— 音频去了另一张声卡（例如没接音箱的板载接口），且**直通硬件**（绕过 PipeWire），`pactl list sink-inputs` 里看不到本程序。根因通常在程序之外：ALSA 的 `default` 被 `/etc/asound.conf` 或 `~/.asoundrc` 钉在某张声卡上（`pcm.!default { type hw card 2 }`），而**声卡编号会变**。修法是把 `default` 指回 PipeWire（`type pipewire`），或删掉该文件。三条诊断命令：
```bash
ls -l /proc/<pid>/fd | grep snd      # /dev/snd/pcmCxD0p = 直通硬件（绕过 PipeWire）
pactl list sink-inputs               # 播放时，你的流应该出现在这里
aplay -D default /dev/zero -f cd     # 打不开（busy / no such device）说明 ALSA 配置有问题
```

候选列表只保留「能提供默认输出配置」且不是 `null` 的 PCM，再按名称去重。选中打不开的设备不会让播放静音，而是回退到上一张声卡，只在状态栏提示。

### 其他设置

- **`qr_aspect`** —— 登录二维码的形状校正。终端字符高约为宽的两倍时，半块字符画正好是正方形。二维码看起来被拉伸或压扁时按 `L` 显示并量一个字符的高宽比：`< 1.5` 用全块，`>= 1.5` 用半块。
- **`theme`** —— `default`（冷蓝）、`graphite`、`sunset`、`forest`、`neon`、`dracula`。写错或删掉会回退到 `default`。在设置页（`,`）切换。
- **`quality`** —— `128`（默认）、`320`、`flac`、`high`、`super`、`viper_clear`、`viper_atmos`、`viper_tape`。后三个是「蝰蛇音效」，**只对部分歌曲可用**；不可用时服务端返回空 URL，界面给出提示。
- **`lite_mode`** —— 关掉三个最耗资源的功能（**播放本身不受影响**）：不下载/解码封面、不做实时频谱、刷新降到 5 fps 且不做动画加速。实测常驻内存：空闲 19.1 MiB / 播放 19.1 MiB（`--api native`），开启后更低。各场景数据见 [DESIGN.md](DESIGN.md#低资源占用)。
- **`tray`** —— 系统托盘，默认开启：注册为 `org.kde.StatusNotifierItem` 后，Quickshell / waybar / KDE 会显示图标，右键菜单提供播放/暂停、上一首、下一首（niri 下另有「最小化 / 显示窗口」）。没有图形会话、没有会话总线、或状态栏没有 `StatusNotifierWatcher` 时自动跳过（各记一条日志）。**改动需重启生效。** 非 Unix 平台不编译这段代码，取值默认为 `false`。

音量、播放模式、歌词偏移在退出时自动写回。

### 自定义快捷键

在 `[keymap]` 段把**动作名**映射到**按键**。动作名是 `Action` 变体的 snake_case 形式：
```toml
[keymap]
reload = "f"                 # 把刷新当前列表（默认 R）改到 f
help = "f1"
quit = "Z"
cycle_artist_filter = "ctrl+f"
```

**未列出的动作保留默认键**，旧配置文件无需改动，只写想改的条目即可。

| 按键写法 | 示例 |
|---|---|
| 单个字符（**区分大小写**） | `q`、`Q`、`/`、`[` |
| 具名键 | `space`、`enter`、`esc`、`tab`、`backtab`、`up`、`down`、`left`、`right`、`home`、`end`、`pgup`、`pgdn`、`backspace`、`delete`、`insert` |
| 功能键 | `f1` – `f12` |
| 带修饰键 | `ctrl+n`、`alt+1`、`shift+tab` |

可用动作名，按用途分组：
```
# 全局
quit  force_quit  help  toggle_sidebar  switch_source
# 导航
move_up  move_down  move_top  move_bottom  page_up  page_down
focus_next  focus_prev  submit  cancel  context_menu
backspace  delete  cursor_left  cursor_right  cursor_home  cursor_end
# 播放
play_pause  next  prev  seek_forward  seek_backward  volume_up  volume_down  toggle_mute
cycle_playback_mode  cycle_quality  toggle_lyric_panel  lyric_delay  lyric_advance
# 业务
open_search  open_settings  reload  load_more_search  download_current
queue_append  queue_play_next  add_all_to_queue  remove_from_queue  clear_queue
clear_cache  toggle_sort_order  open_ranks  open_cloud  login  claim_vip  cycle_artist_filter
add_to_cloud  remove_from_cloud  sync_to_cloud  delete_cloud_playlist  new_cloud_playlist
set_default_source  raise_source_priority  lower_source_priority
# 窗口（仅 niri）
toggle_window
```

> `toggle_window` 是**唯一没有默认键的动作**，通常从托盘菜单点击。仅在 niri 下有效（依据是 `NIRI_SOCKET` 是否存在）—— Wayland 的 xdg-shell 没有「最小化」请求，终端程序只能走合成器 IPC。带参数的动作（切换标签页、跳转到指定位置、输入字符）不在列表中，它们的值来自运行期，无法写进配置文件。

**无效条目会被跳过，不会中断启动**，并记一条 WARN（`~/.cache/kugou-tui/kugou-tui.log`）：

| 情况 | 日志 |
|---|---|
| 未知动作名 | `键位配置：未知动作 "xxx"（键 "yyy"），已忽略` |
| 无法解析的键名 | `键位配置：无法解析按键 "yyy"（动作 xxx），已忽略` |
| 一个键绑了两个动作 | `键位配置：… 同时绑定了 …，以 … 为准` |

启动成功时会有一行 `已加载 N 条自定义键位`，用它核对条数。

### 会话持久化

退出时把**播放队列 + 当前曲目 + 播放位置**存到 `~/.cache/kugou-tui/session.json`；下次启动恢复并提示「已恢复上次会话：N 首 · 按 Space 继续播放」。

> 该路径**固定**：`--cache-dir` / `cache_dir` 只改音频缓存目录，不会连带移动会话文件（日志同理）。要连会话一起移动，改 `XDG_CACHE_HOME`。

**自动播放被有意关闭**（程序一开就出声太突兀）。恢复后按 `Space` 继续，播放**从上次位置接着走**，不从开头开始；该位置只对恢复的那首有效，先播别的会丢弃它，否则下次对任意曲目按播放都会从旧位置开始。同一个文件还记录 `vip_claimed_day`（上次领取概念版每日 VIP 的日期），「每天只能领一次」靠它实现（该接口有风控）；删掉文件会让程序下次启动重新领取，通常无害。
