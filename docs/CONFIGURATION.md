# 配置

## 命令行参数

命令行 > 环境变量 > 配置文件 > 内置默认值。

| 参数 | 环境变量 | 说明 |
|---|---|---|
| `-a`, `--api-base <URL>` | `KUGOU_API_BASE` | KuGouMusicApi 地址，默认 `http://127.0.0.1:3000` |
| `-c`, `--cookie <COOKIE>` | `KUGOU_COOKIE` | 登录凭据，形如 `token=xxx; userid=xxx` |
| `-s`, `--search <KEYWORDS>` | — | 启动后立刻搜索该关键词 |
| `--volume <0-100>` | — | 初始音量 |
| `--cache-dir <DIR>` | — | 音频缓存目录 |
| `--cache-limit <MiB>` | — | 缓存上限，`0` 表示不限制 |
| `--tick-ms <MS>` | — | 刷新间隔，50–5000，调大可进一步降低 CPU |
| `--page-size <N>` | — | 搜索结果与歌单广场的每页条目数，5–200 |
| `--proxy <URL>` | `KUGOU_PROXY` | 访问 API 服务时用的 HTTP 代理 |
| `--basic-color` | — | 使用 16 色固定色板，适配老终端 |
| `--print-config` | — | 打印最终生效的配置、缓存与日志路径后退出 |

```bash
kugou-tui --print-config        # 查看当前生效的配置与路径
kugou-tui --tick-ms 1000        # 省电模式：空闲 CPU 接近零
kugou-tui -s "海阔天空"          # 启动即搜索（需要登录）
```

## 配置文件

首次运行自动生成，路径 `~/.config/kugou-tui/config.toml`（权限 `0600`，因为可能含 cookie）。

```toml
api_base = "http://127.0.0.1:3000"
cookie = "token=xxx; userid=xxx"   # 登录态，可手动填
dfid = "..."                        # 设备指纹，首次启动自动获取
volume = 0.7                        # 0.0–1.0
playback_mode = "sequential"        # sequential | repeat_all | repeat_one | shuffle
cache_dir = "/home/you/.cache/kugou-tui"
cache_limit_mib = 512               # 0 = 不限制
lyric_offset_ms = 0                 # 歌词整体偏移，正=延后
tick_ms = 200                       # 界面刷新间隔（毫秒）
page_size = 30                      # 搜索结果与歌单广场的每页条目数
proxy = "http://127.0.0.1:7890"     # 可选
quality = "128"
basic_color = false
theme = "default"                   # 见下方「主题」
download_dir = "~/Music"            # 单曲下载保存到这里，留空也行
qr_aspect = 2.0                     # 终端字符「高:宽」比，见下方说明
lite_mode = false                   # 简易模式，见下方说明
cover_fill = "crop"                 # 首页大封面怎么铺满，见下方说明
```

`cover_fill` 决定**首页那块大封面**怎么填满它的区域。封面区是「多少列 × 多少行」，
换算成像素后几乎永远不是正方形，而专辑封面大多是正方形——**框和图的形状不一致时，
「铺满」「不变形」「不裁剪」三者只能同时满足两个**，必须挑一个放弃：

| 取值 | 铺满 | 变形 | 裁剪 | 说明 |
|---|---|---|---|---|
| `crop`（默认） | 是 | 否 | 是 | 居中裁剪。等价 CSS 的 `object-fit: cover` |
| `stretch` | 是 | 是 | 否 | 直接拉到区域大小，方图会被横向拉宽 |
| `fit` | 否 | 否 | 否 | 图完整，但框比图宽时左右露出底色 |

默认 `crop`：专辑封面基本居中构图，裁掉一点边框通常看不出来，而「框里空着一块」
是一眼就能看见的。觉得裁得太多就改成 `fit`。

运行时按 `,` 打开设置页，最后一项「封面铺满」可以直接切换，改完立即生效并落盘
——想比较三种效果不用重启。

`qr_aspect` 用来矫正登录二维码的形状：终端字符的高通常是宽的 2 倍，此时用
半块字符（一个字符承载两行模块）画出来正好是正方形。**如果你觉得二维码被
拉长或压扁**，按 `L` 让二维码出现，量一个字符的高宽比，把它填到这一行：
`< 1.5` 改用全块字符（一模块占一字符一行），`>= 1.5` 用半块字符。

主题取值：`default`（冷蓝）、`graphite`（石墨，近乎无彩）、`sunset`（日落）、
`forest`（森林）、`neon`（霓虹）、`dracula`（暗紫）。写错或删掉这行会回落到
`default`。运行时按 `,` 打开设置页可直接切换，改完立即写入这个文件。

`quality` 的合法取值：`128`（默认）、`320`、`flac`、`high`、`super`、
`viper_clear`、`viper_atmos`、`viper_tape`。后三个是酷狗的「蝰蛇音效」系列，
**仅部分歌曲支持**，拿不到时服务端返回空地址，界面会给出提示。

`lite_mode` 开启后会关掉三样最吃资源的，换更低的占用（**听歌本身不受影响**）：

- 不下载 / 解码封面（图片解码 + 图形协议是最占内存的一块）
- 不算实时频谱
- 界面刷新降到 5fps，并且**不做动画提速**（逐字歌词推进与可视化频谱都退回 5fps）

实测常驻内存（VmRSS，release 构建）：空闲 14.2 MiB、播放中 16.9 MiB，开启后能再降
一截（降的主要是封面那几十到几百 KB 的解码位图）。在低配机器或电池供电时有用。
设置页可直接开关。分场景的完整数字见 [DESIGN.md](DESIGN.md#低资源占用)。

音量、播放模式、歌词偏移会在退出时自动写回。

### 自定义键位

用 `[keymap]` 段把**动作名**映射到**按键**。动作名是 `Action` 变体的 snake_case：

```toml
[keymap]
reload = "f"                 # 刷新当前列表（默认 R）改到 f
help = "f1"                  # 帮助面板
quit = "Z"
cycle_artist_filter = "ctrl+f"
```

**未列出的动作沿用默认键位**，所以老配置文件不用改，只写想改的那几条。

按键名怎么写的规则：

| 形式 | 例子 |
|---|---|
| 单个字符（**区分大小写**） | `q`、`Q`、`/`、`[` |
| 具名键 | `space`、`enter`、`esc`、`tab`、`backtab`、`up`、`down`、`left`、`right`、`home`、`end`、`pgup`、`pgdn`、`backspace`、`delete`、`insert` |
| 功能键 | `f1` – `f12` |
| 带修饰键 | `ctrl+n`、`alt+1`、`shift+tab` |

可用的动作名（59 个，按用途分组）：

```
# 全局
quit  force_quit  help  toggle_sidebar  switch_source
# 导航
move_up  move_down  move_top  move_bottom  page_up  page_down
focus_next  focus_prev  submit  cancel
backspace  delete  cursor_left  cursor_right  cursor_home  cursor_end
# 播放
play_pause  next  prev  seek_forward  seek_backward
volume_up  volume_down  toggle_mute
cycle_playback_mode  cycle_quality
toggle_lyric_panel  lyric_delay  lyric_advance
# 业务
open_search  open_settings  reload  load_more_search  download_current
queue_append  queue_play_next  add_all_to_queue  remove_from_queue  clear_queue
clear_cache  toggle_sort_order  open_ranks  open_cloud
login  claim_vip  cycle_artist_filter
add_to_cloud  remove_from_cloud  sync_to_cloud  delete_cloud_playlist  new_cloud_playlist
set_default_source  raise_source_priority  lower_source_priority
```

**非法条目只跳过、不中断启动**，并在日志里记一条 WARN
（`~/.cache/kugou-tui/kugou-tui.log`）——键位错了只是不顺手，不该让程序起不来：

| 情况 | 日志 |
|---|---|
| 动作名不存在 | `键位配置：未知动作 "xxx"（键 "yyy"），已忽略` |
| 按键名解析不了 | `键位配置：无法解析按键 "yyy"（动作 xxx），已忽略` |
| 一个键绑了两个动作 | `键位配置：… 同时绑定了 …，以 … 为准` |

启动成功时会有一行 `已加载 N 条自定义键位`，用它核对生效条数。

> 带参数的动作（切换标签页、跳转到指定进度、输入字符）不在清单里——它们的值来自
> 运行时，配置文件里写不出完整语义。

### 会话持久化

退出时会把**播放队列 + 当前曲目 + 播放位置**存到
`~/.cache/kugou-tui/session.json`，下次启动自动恢复，界面提示
「已恢复上次会话：N 首 · 按 Space 继续播放」。

> 这个路径是**固定的**，`--cache-dir` / `cache_dir` 只改音频缓存目录，不会把
> 会话文件一起搬走（日志同理）。想连会话一起挪，只能改 `XDG_CACHE_HOME`。

**刻意不自动播放**——一开程序就出声很吓人，也可能在不该出声的场合。
恢复后按 `Space` 继续，而且**从上次的位置接着播**（界面提示「接着上次播：MM:SS」），
不是从头开始。

只对恢复的那首歌生效：如果你先去播了别的，那个位置就作废了——否则下次停在
任意一首上按播放都会从上次的位置开始。

它和配置分开存：配置是你手改的长期设置，会话是程序自己写的瞬时状态，
混在一个文件里会互相覆盖。

同一个文件里还记着 `vip_claimed_day`——上一次领取概念版每日 VIP 的日期。
有它才能做到「一天只领一次」（那个接口带风控，官方文档也写着「尽量别频繁调用」）。
删掉这个文件会让程序下次启动重新领一次，通常无害。

---
