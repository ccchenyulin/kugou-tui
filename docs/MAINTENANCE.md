# 维护与排障手册
**什么时候读**：改播放/下载/缓存链路时；用户报「内存只涨不降」「进度条跳回开头」「播着播着跳歌」「缓存里只有半首歌」时；或隔了很久回到项目、想知道模块在哪时。

本文件记录**坑与验证方法**，不描述功能 —— 功能见 [README](../README.md) 与 [docs/USER_GUIDE.md](USER_GUIDE.md)，设计取舍见 [docs/DESIGN.md](DESIGN.md)。
## 1. 地图
### 1.1 目录
```
src/
├─ main.rs  cli.rs  config.rs  event.rs  keymap.rs  logger.rs  error.rs  util.rs
├─ bootstrap.rs  bootstrap_disabled.rs（`--no-default-features` 接管，同名同签名，全部返回错误）
├─ mpris.rs  tray.rs（**仅 Unix**）  window.rs（niri 窗口控制）  ws.rs（WebSocket，仅 127.0.0.1）
├─ api/        traits.rs client.rs catalog.rs cloud.rs lyric.rs model.rs node.rs
│  └─ native/  mod.rs（26 条路由，逐条对齐上游） sign.rs crypto.rs device.rs krc.rs transport.rs
├─ audio/      engine.rs streaming.rs download.rs cache.rs resample.rs downmix.rs levels.rs spectrum.rs
├─ source/     mod.rs netease.rs sodam/（mod.rs client.rs；数据面来自 crates/libresoda）
├─ app/        mod.rs state.rs update.rs navigation.rs playback.rs search.rs cloud.rs settings.rs desktop.rs queue.rs session.rs
├─ ui/         mod.rs theme.rs icons.rs widgets.rs views/
crates/libresoda/  汽水音乐数据面（独立 crate，包名 libresoda-fork），与上游 SodaM 同一个库
tools/             kat/（KAT 基准脚本）  qishui-signer/（CDP 签名页脚本）
```
配置路径：Linux `~/.config/kugou-tui/`、`~/.cache/kugou-tui/`；Windows `%APPDATA%\\kugou-tui\\`、`%LOCALAPPDATA%\\kugou-tui\\`。
### 1.2 线程与归属
```
输入线程 ──────┐
               ├─→ EventBus ─→ 主循环(update.rs) ─→ 绘制
异步任务 ──────┘                    │
                                    └─→ AppState（仅主线程，无锁）
音频线程(kugou-audio) ─────────────→ 原子量(position/duration/volume/state) + EventBus
ws 线程(ws.rs) ────────────────────→ 自建 current_thread runtime（见 1.9）
```
三条铁律：**只有主线程改 `AppState`**（所以无锁，异步任务不要碰）；异步任务产物一律**回主线程**由 `update.rs` 消费（`bus.emit(Loaded::…)`）；音频线程与主线程之间**高频数据走原子量**（position/volume/levels）、**离散事件走 EventBus**（加载完成、播完、出错）。WebSocket 线程是这三条的例外但不破坏它们：只读主循环每 tick 写入的 `watch` 快照，入站命令经 `EventBus` 回到主线程。
### 1.3 播一首歌的数据流
```
用户按 Enter
  └→ App::start_playback(song, start_at_ms)          update.rs
       ├→ 取消上一首仍在跑的流式下载（active_stream.cancel()）
       ├→ audio.mark_loading()                       状态立刻变「缓冲中」
       ├→ load_cover() / request_lyric()
       └→ request_stream()
            ├─ cache.find(key) 命中 ──→ audio.load(AudioSource::File(path), position)
            └─ 未命中 → 取直链（/song/url，逐个试候选 (hash, quality)）
                 └→ Loaded::StreamReady
                      └→ App::start_download()
                           ├─ start_at_ms > 0 → fetch_to() 整轨 → StreamCached → load(File)
                           └─ start_at_ms = 0 → start_streaming()（写 .part + 填缓冲）
                                └→ 攒够 128 KiB → Loaded::StreamPrerolled
                                     └→ audio.load(AudioSource::Stream(buffer))
                                └→ 整轨下完 → rename .part → Loaded::StreamCompleted（只做收尾）
```
### 1.4 缓存文件命名
`{hash}-{quality}.{ext}`，例如 `b3a52a7a…-128.mp3`；不同音质互不覆盖。未完成的文件是同目录、同名 + `.part`，两种形态，`cache.find` 都看不见（有意为之）：

| 形态 | 谁用 |
|---|---|
| `abc-128.mp3.part` | 整轨下载（`fetch_to`：续传、预取下一首） |
| `abc-128.mp3.stream.part` | 边下边播（`start_streaming`） |

**两种形态刻意分开**：同一首歌可能同时有整轨下载（后台预取）和流式下载在跑；共用一个 `.part` 时整轨下载的 `File::create` 会截掉流式已写入的字节，而窗口外的字节靠 `pread` 从该文件回读 —— 回读到空洞就是噪声。两者都以 `.part` 结尾，`cache::is_partial` 仍能识别。

同目录还有脚本用的运行期文件，命名是**两个平台之间的约定**，改一边就要改另一边：

| 文件 | 内容 | 写 / 读 |
|---|---|---|
| `api-<instance>.pid` | `<PID> <port>`（空格分隔） | 启动器与 `kugou-api` 写；`kugou-api stop/status` 读 |
| `api-<instance>.log` | 服务的 stdout（Windows 另有 `.log.err`） | 启动器与 `kugou-api` 写；`kugou-api logs` 读 |

实例名只有两个：`standard`（标准版 / 网易云）与 `lite`（酷狗概念版）。PID 文件带端口是因为：只有 PID 时，一个改过端口却没重启的旧进程会让 `status` 在新端口上报「运行中」（见 `kugou-api` 里 `pid_of` 的注释）。
### 1.5 打开歌单（两段式加载）
```
打开歌单
  ├→ first_screen_page()          ← 决定先取哪一页（与 sort_descending 一致）
  │     └→ 取一页 → Loaded::PlaylistTracks → set_songs_sorted(…, descending)
  ├→ needs_full_fetch()           ← 整表是否还需要补全
  └→ 后台并发取完剩余页 → Loaded::PlaylistTracks（整表覆盖）
```
**首屏页必须与显示方向一致**：默认降序 → 最后一页的内容出现在最上方，所以首屏必须取最后一页；否则用户先看到列表中段的歌，几秒后整表到达时画面整屏翻转。「还有没有更多」不能用本页是否满来判断（见 §5「列表分页」）。
### 1.6 平台分支在哪

| 位置 | 差异 | 为什么不能统一 |
|---|---|---|
| `Cargo.toml` 的 `[target.'cfg(windows)']` / `[target.'cfg(unix)']` | TLS 后端（SChannel / rustls）、`zbus`、`libc`、`windows-sys` | Windows 上 rustls 拖入 `aws-lc-sys`，需装 CMake + NASM；Windows 没有 D-Bus |
| `audio/streaming.rs` | `read_at` 包装：Unix `pread` / Windows `seek_read` | Windows 没有 `pread` |
| `logger.rs` | stderr 重定向：Unix `dup2` / Windows `SetStdHandle` | Windows 没有 fd 表 |
| `main.rs`、`app/mod.rs`、`app/update.rs` | `mpris` / `tray` 模块、字段、同步逻辑 | 同上（D-Bus） |
| `config.rs`、`app/settings.rs` | 路径都走 `dirs`；只有 `~` 展开需要额外回退 | Windows 没有 `HOME` |
| `ui/icons.rs` | Nerd Font 检测走 `fc-list`，Windows 没有 → 回退 ASCII + `KUGOU_TUI_NERD_FONT` 覆盖 | 那边的字体列表在注册表里 |
| `api/model.rs`、`event.rs`、`keymap.rs` | 三处 `#[cfg_attr(not(unix), allow(dead_code))]` | 这些项只被 MPRIS 构造；用 `allow` 而非 cfg 掉，是为了枚举/模型形状两边一致，下游 `match` 不需平台分支 |
| `scripts/*`（bash） | 无平台分支；只**使用两边都有的构造** | 脚本在 Linux 与 macOS 上跑同一套，见 §1.7 |
| `scripts/*.ps1` | `$env:OS -eq 'Windows_NT'` 判断（`-WindowStyle` 等只在 Windows 有） | `$IsWindows` 是 PowerShell 6+ 自动变量，而 Windows 自带 5.1 |

验证 Windows 侧**不需要 Windows 机器**（编译期可查的部分）：
```bash
rustup target add x86_64-pc-windows-msvc
cargo check   --target x86_64-pc-windows-msvc --all-targets
cargo clippy  --target x86_64-pc-windows-msvc --all-targets -- -D warnings
```
`--all-targets` 也会查测试代码；**最后的链接步骤需要 Windows 或 MSVC 工具链**，在 Linux 上报 `error: linker link.exe not found` 就说明 Rust 侧全部通过。运行期部分由 `.github/workflows/ci.yml` 覆盖：**Linux / Windows / macOS 三条 lane** 各跑 `clippy -D warnings + test`，**不要删** —— 路径展开、配置目录、缓存文件命名只有真跑才暴露；macOS lane 与 Linux 共用 `cfg(unix)`，但 `dirs` 给的是 `~/Library/...`，音频走 CoreAudio，且没有 `fc-list`。这三条 lane **只做验证**（没有 `cargo build --release`、不出发布包、不上传产物）；三平台发布产物由 `release.yml` 在打标签时产出。
### 1.7 启动器脚本：跨平台约定
`scripts/` 下的 bash 脚本在 Linux 与 macOS 上跑**同一套**；PowerShell 那些只在 Windows 有意义（发布 zip 里也带 bash 的，供 Git Bash / WSL 用）。

| Windows | Unix 侧 | 说明 |
|---|---|---|
| `kugou-tui.ps1` | `kugou-tui` | 启动器。**刻意不写 `param()` 块**，否则 PowerShell 会把 `-s 海阔天空` 当成拼错的参数名；参数经 `$args` 原样透传。`--dry-run` 只打印决策 |
| `kugou-api-install.ps1` | `kugou-api-install` | 只 clone + 装依赖，**不启动** —— 启动服务是启动器的事，避免两份逻辑 |
| `kugou-api.ps1` | `kugou-api` | 启停管理（`start` / `stop` / `restart` / `status` / `logs`）。与启动器共用 PID 文件，见 §1.4 |
| `build-windows.ps1` | `make-release-tarball` | 构建 + 打包 zip。**哪些脚本进包由「文档里提到过」决定**，不只看平台 —— v0.4.3 漏了 `.ps1` 那几个，而文档恰好叫用户去跑漏掉的那些 |

#### bash 侧：不允许的写法
macOS 带 BSD 工具加 **bash 3.2**；下面每一条都会让脚本直接坏掉。三个脚本各自独立分发，**没有共享库** —— 改就要三个一起改。

| 不允许 | 后果 | 改用 |
|---|---|---|
| `readlink -f` | macOS 报 `illegal option -- f` | 逐层解软链（见各脚本的 `self_path` / `self_dir`） |
| `dirname --` | BSD 把 `--` 当成参数本身 | 去掉 `--` |
| `seq` | GNU coreutils，BSD 没有 | `while [ "$i" -lt N ]` 计数循环 |
| `setsid` | util-linux 专有 | `nohup` |
| `ss` | iproute2，macOS 没有 | `lsof -nP -iTCP:<port> -sTCP:LISTEN -t` |
| `declare -A` | bash 3.2 报 `declare: -A: invalid option` | `case` 分发 + `${!name}` 间接展开 |
| `"${arr[@]}"`（空数组） | bash 3.2 + `set -u` 报 unbound variable | `"${arr[@]+"${arr[@]}"}"` |

**目录必须与 `dirs` crate 对齐**：配置根 Linux 是 `$XDG_CONFIG_HOME` / `~/.config`、macOS 是 `~/Library/Application Support`；缓存根 Linux 是 `$XDG_CACHE_HOME` / `~/.cache`、macOS 是 `~/Library/Caches`。macOS 上 `dirs` **不读** XDG 变量，所以脚本也不能读：启动器去 `~/.config` 找一个永远不存在的 `config.toml`，于是永远按默认值探测（`:3000`、标准版），概念版用户看到的就是「服务启动失败」。配置根可以用 `KUGOU_TUI_CONFIG_DIR` 整体覆盖（三个脚本 + 两个 ps1 都接受，语义与 `config.rs` 的 `config_root()` 一致）。
#### PowerShell 侧：四点注意
* **只用 PowerShell 5.1 语法**（Windows 自带的版本）。不要用 `$IsWindows`（6.0+）或 `Start-Process -Environment`（7.4+）；平台判断统一用 `$env:OS -eq 'Windows_NT'`。
* **文件必须带 UTF-8 BOM**，这一点**只能靠人保证** —— CI 抓不到。没有 BOM 时 5.1 用当前 ANSI 代码页解码 `.ps1`（中文 Windows 上是 GBK），而脚本里有几十条面向用户的中文提示，会全变乱码。PowerShell 7 默认按 UTF-8 读，CI 用的正是 `shell: pwsh`（7.x），所以**CI 绿不等于用户那边没问题**。新增或重写 `.ps1` 后用 `head -c 3 scripts/*.ps1 | od -An -tx1` 确认每个文件都以 `ef bb bf` 开头；LF 行尾没问题。
* `Start-Process` **不允许**把 stdout 与 stderr 重定向到同一个文件，所以服务日志有两个：`api-<instance>.log` 与 `api-<instance>.log.err`（实例名 `standard`/`lite`/`netease`）；出错时两个都 dump。
* 启动器里的配置解析（读 `sources.active` 与 `[sources.<kind>].api_base`）是手写正则，不是 TOML 解析器；配置结构改了要一起改。同理 `kugou-api-install.ps1` 里钉的 commit 必须与 bash 版一致。

`docs/INSTALL.md` 的「在 Windows 上构建与运行」一节列了平台能力表；改完平台分支后那张表也要同步。
### 1.8 拆分 `update.rs`：进度与规则
`update.rs` 曾是 4700 行的 `impl App`；拆分目标是降低「改一处要读多少上下文」的成本，不是减少行数。先修可靠性问题（`download.rs` 的 Range 校验、陈旧结果判据集合 `accepts_*`），再动结构。

| 模块 | 行数 | 内容 |
|---|---|---|
| `navigation.rs` | 275 | 切页、焦点、移动选中、`go_back`、数字键、`v` 键开音源页 |
| `playback.rs` | 370 | 起播 / 切歌 / 进度 / 音量 / 静音 / 歌词偏移 |
| `search.rs` | 142 | 提交搜索、加载更多 |
| `desktop.rs` | 160 | MPRIS / 托盘快照、窗口控制 |
| `cloud.rs` | 1160 | 登录 / 切音源 / VIP / 云端歌单 / 账号资料 |
| `settings.rs` | 760 | 设置页候选值与三种交互（`move_settings` / `click_setting` / `adjust_setting`） |

`update.rs`：4728 → 4144 → 2958 行（当前 3679）。每块都是**纯搬移**、零逻辑变更。`activate()` 与 `startup_search()` 搬回了分发层 —— 留在子模块会形成 `navigation ↔ search` 的叶子对叶子依赖。

依赖形状（**要守住的目标**）：
```text
update ──→ navigation / search / playback / cloud / desktop / settings / mod   （分发层）
navigation ──→ update（向加载层要数据）, settings（设置页光标移动）
playback ──→ update (client_for / load_cover / request_lyric / sync_queue_cursor)
settings ──→ update (refresh_cache_usage)
cloud ──→ update (client_for), mod (ensure_device_fingerprint)
desktop ──→ (none)
search ──→ (none)
```
叶子之间零调用。留在 `update.rs` 的是分发（`handle_event` / `handle_action` / `activate`）、加载层（`load_*` / `open_selected_*`）、结果与收尾（`handle_loaded` / `handle_audio_event` / `tick`）。**判据**：方法被谁调用、结果由谁处理，比挂在哪个标题下更重要 —— 所以 `sync_mpris` / `sync_tray` / `toggle_window` 去了 `desktop.rs`，`client_for`（8 处调用）与 `request_lyric` 留在 `update.rs`；唯一刻意保留的跨模块调用是 `navigation.rs::move_selection` 调 `settings.rs::move_settings`。

规则：

* **一次只搬一块，搬完立刻 `clippy -D warnings` + `cargo test`**；全部测试（当前 703 个）必须绿，不绿就是搬错了。
* **搬移要留证据**：搬前拷到 `/tmp`，搬完按函数名比对签名 + 函数体（空白折叠、`pub(super)` 归一化）；这一轮 116 个函数逐字符存活。
* **不要把改动混进搬移 commit**。
* `handle_loaded` 留在 `update.rs`（判据见 §4 第 11 条）。
* **剪切容易吃掉小节标题**：`sed -i 'A,Bd'` 后立刻 `sed -n` 看接缝，并 `grep -n '^    // [^=]'` 查剩下的小节标题。
* **拆完跑模块调用图，要的是星形**：数 `src/app/X.rs` 里 `self.<Y 的方法>(` 出现次数；叶子 ↔ 叶子就是边界错了。
* 常量跟着使用者走：`RESTART_THRESHOLD_MS` 进 `playback.rs`，`SEARCH_MAX_PAGES` 进 `search.rs`。
* **Rust 方法私有性按「impl 块写在哪个模块」算**：方法搬走后在另一边调会报 `E0624 method is private`，要改成 `pub(super)`（navigation 13、playback 16、desktop 3、cloud 15、settings 1），**两个方向都要改**。
### 1.9 WebSocket 服务（`ws.rs`）
协议对齐 MoeKoeMusic（<https://music.moekoe.cn/zh-CN/websocket-api.html>），默认监听 `127.0.0.1:6520`，`--no-ws` / `ws = false` 关闭，`--ws-port` 改端口。
```
主循环(tick) ──sync_ws()──→ watch::Sender<Arc<Snapshot>>   （只写快照）
                                        │
ws 线程 ──watch::Receiver───────────────┘  diff_messages() → broadcast → 各连接
    │
    └── 入站控制 ──EventBus──→ 主循环分发成已有的 Action
```
改之前先读的取舍：**独立线程 + `current_thread` runtime**，不塞进主 runtime（主循环 tick 节奏不应受网络 I/O 影响）；**`sync_ws` 每 tick 写一份完整快照**（含 `Song` clone），`send_if_modified` 用 `Snapshot` 自定义的 `PartialEq` 只比「会改变推送内容」的字段，刻意不比整个 `Song`（`privilege` 是很大的嵌套 JSON），快照没变就不唤醒广播任务；**`playerState` 只在播放/暂停切换时推**（上游 `updatePlayerState` 的唯一调用点是 `electron/main.js` 的 `play-pause-action`），连接建立时另外单独发一次；**`lyrics` 每 tick 都推**（上游 `server-lyrics` 那条路径同样没有防抖），`lyricsData` 是**原始歌词文本**即 `Lyric::text`，不是解析后的 `lines`，因为逐字标记和 `[language:]` 标签正是第三方客户端要自己排版的东西；**welcome 的 `data` 是纯字符串**，不是文档示例里的嵌套对象，以源码为准；**未知 `control` 命令回 `error`**，不像上游那样静默忽略；**握手校验 `Origin`**（浏览器里任何页面都能连本地端口，不校验等于把播放控制开放给当时开着的每个标签页；没有 `Origin` 的原生客户端放行）；**入站消息上限 64 KiB**（tungstenite 默认 64 MiB 太松），出站不限，KRC 可能有几十 KB；**`lyrics` 变体是 `Box<LyricsData>`**（不装箱时每个枚举实例 264 字节，而且是按值放在 `Vec` 里传）。
### 1.10 怎么验证 WebSocket
没有自动化集成测试（需要真播放器在跑）；用外部客户端手工验：
```bash
# 1) 启动播放器（隔离配置目录！别碰 ~/.config/kugou-tui）
KUGOU_TUI_CONFIG_DIR=/tmp/kt-ws ./target/debug/kugou-tui --search 周杰伦 --no-tray
# 2) 另一个终端，任意 WS 客户端连 ws://127.0.0.1:6520/
#    预期：先 welcome + playerState，播放中每 200ms 一条 lyrics；
#    发 {"type":"control","data":{"command":"toggle"}} 应切换播放并回一条 playerState。
```
`node --experimental-websocket` 或浏览器控制台都行，不需要额外依赖。
### 1.11 内嵌接口层（`api/native/`）与 KAT
`--api native`（默认）在进程内实现酷狗的 26 条路由，**不再需要 Node.js**。风险是「结果错了却不报错」：签名、设备指纹、KRC 解密任何一字节偏了，服务端返回的都像业务错误（或空 body）。对策是 **KAT（已知答案测试）**：在上游 JS 上用固定输入算出基准值，Rust 侧断言逐字节相等。基准值**不是手抄的常量**，由 `tools/kat/` 下的脚本现场产出（依赖、用法、已知坑见 [tools/kat/README.md](../tools/kat/README.md)）；上游改了算法或需要复查签名时，重跑同一个脚本比对。

改 `api/native/` 下任何东西时的规则：

- **每个函数在注释里写明来源文件与函数名**（逐函数从上游移植），供日后交叉核对。
- **改实现就要重跑对应 KAT 脚本，必要时更新期望值** —— 只改 Rust 不改基准，测试要么红要么假绿。
- `tools/kat/` 下的脚本**不参与构建，也不是测试夹具**，纯粹是「基准值从哪来」的存档。

汽水音源的数据面是另一个 crate（`crates/libresoda`），行为对齐官方 SodaM 客户端，那边出问题**先看上游修没修**；不要在本仓库写第二份。
## 2. 运行与验证
```bash
cargo test                         # 单元测试（约 700 个，秒级）
cargo clippy --all-targets         # 发布脚本会跑这条
cargo build --release              # 二进制约 11.7 MiB
./target/release/kugou-tui --print-config   # 显示生效配置、日志路径、缓存目录
```
**冒烟测试**（机器上有真声卡与接口服务）：
```bash
# 1) 启动接口服务（首次需要 ./scripts/kugou-api-install）
./scripts/kugou-api              # 或 launchd/systemd 那套

# 2) 用干净缓存播一首，同时看日志
KUGOU_TUI_DEBUG=1 ./target/release/kugou-tui --search "周杰伦 晴天" --no-tray \
    --cache-dir /tmp/smoke-cache
# 然后：Esc → Enter 播放 → n 切歌 → 空格暂停/继续 → q 退出
# 日志：~/.cache/kugou-tui/kugou-tui.log
```
要把实验与真实配置/会话**隔离**，用 XDG 变量改位置：
```bash
XDG_CONFIG_HOME=/tmp/xdg/config XDG_CACHE_HOME=/tmp/xdg/cache \
    ./target/release/kugou-tui --print-config
```
> 原因：`session.json`（上次位置、队列）在 `XDG_CACHE_HOME/kugou-tui/`，且**退出时整体覆盖**。拿真实配置跑自动化测试会冲掉用户的队列光标与播放位置。要么隔离，要么先备份 `~/.cache/kugou-tui/session.json`。
## 3. 两类难缠问题的排障
### 3.1 内存只涨不降
**现象**：连续听歌、切歌之后 RSS 超过 100 MB，停下来也不回落。**先测，别猜。**
```bash
pid=$(pgrep -f 'target/release/kugou-tui' | head -1)
grep -E 'VmRSS|VmHWM|Threads' /proc/$pid/status    # 当前 / 峰值 / 线程数
while :; do grep VmRSS /proc/$pid/status; sleep 1; done   # 循环采样，看趋势不看单点
```
**这一类历史上两个根因**（都可能在别处复发；动手前先确认这里没有重新引入）：
1. **无界缓冲**。`StreamingBuffer` 曾把整轨下载的每个字节都留在内存里（还是 `Vec` 倍增的整轨），所以 64 MiB 的 Hi-Res 歌就是 66.5 MiB。现在只保留 4 MiB 的尾部窗口，其余用 `pread` 从磁盘 `.part` 回读。→ **任何「随媒体大小增长」的内存结构，都要问「上界是多少」。**
2. **没人取消的后台任务**。切歌时不取消上一首的流式下载，每按一次 `n` 就多一个正在跑的下载，把整轨往内存里灌 —— 实测连续切 5 次后 323.7 MiB。现在 `App::active_stream` 持有当前流的句柄，`start_playback` / `stop_playback` / `shutdown` 都会 `cancel()`。→ **创建后台任务时，问「谁负责停它」。**

**定位手法**（按性价比排序）：

| 手法 | 能看出什么 |
|---|---|
| 采样 `VmRSS`，与「时间窗 + 用户动作」对齐 | 增长发生在播放中、切歌时，还是空闲时 |
| `VmHWM` 与当前 RSS 的差 | 有没有一次性大峰值（如整轨下完那一刻） |
| `Threads:` 数量 | 线程/任务有没有泄漏（正常 7–9） |
| `KUGOU_TUI_DEBUG=1` 看 `预取` / `缓存回收` / `流式下载` 日志 | 哪条链路在跑、跑完没有 |
| 对照实验：把 `MAX_KEPT_BYTES` 临时设成 64 KiB | RSS 跟着降，说明大头就是这个缓冲 |

**不要做**：周期性 `malloc_trim`、周期性 `shrink_to_fit`、GC 式「回收线程」—— 只是把增长藏起来，峰值和上界都还在。要改的是**上界与归属**。
### 3.2 播放跳回开头 / 自动跳歌
**现象**：边下边播时缓冲一段后从头重放；或听着听着跳到下一首；或十几秒一循环。**先分清「播完了」与「坏了」**，两者在播放层看起来一样：

> `rodio` 的解码器把**任何**读错误都吞成 EOF（`symphonia` 的 `format.next_packet().ok()?`），所以音源「结束」可能是真播完，也可能是流断了。

所以 `engine::Runtime::sync()` **不能把播放器空当成播完**，必须按流的状态判（`classify_drain`）：

| 真实流状态 | 收尾 | 用户看到 |
|---|---|---|
| `is_cancelled()` | 静默 | 切歌了，不该弹任何东西 |
| 已下完 | `TrackFinished` | 正常进下一首 |
| 未下完 + 有 `error` | `Failed(reason)` | 报错、停止、不切歌 |
| 未下完 + 无 error（读超时） | `StreamInterrupted { position }` | 原地不动，从该位置自动恢复一次 |

**这一族历史上两个根因**：
1. **播完后又重载**（把 `Loaded::StreamCached` 当成「可以播了」）。歌已经在播了，再 `audio.load()` 会把位置重置为 0。现在流式路径走 `Loaded::StreamCompleted`，只收尾，不碰播放器。
2. **流断了被当成播完**。单曲循环（或只有一首的队列）下，切歌就等于「同一首从 0 重放」；现在由上面的分级处理，**读超时**那种还会保留位置并自动恢复一次。

**位置必须在歌结束之前记下来**：
```rust
// 最后一帧读不到 get_pos()：音源已经没了，rodio 报 0
let position_ms = self.last_position_ms;   // 上一帧还在播时的值
```
否则播完那一刻进度条跳回 00:00，之后的「恢复」也没有位置可恢复。

**复现方法**（真机、可重复）：用一个「掐流」代理，把 CDN 的数据在传输中途断 20–30 秒，剩下的交给真实解码与播放。要点：选小音质（`quality = "128"`，4 MB 的歌），让下载能在播放中途「下完」或「断掉」；选播放模式 `repeat_one`，它把「从头重放」放大成看得见的现象（顺序播放只会看起来像「跳过这首」）；用 **app 自己的状态**观察（退出时写的 `session.json` 里的 `position_ms`，或日志行「会话已保存：N 首，位置 X ms」），不要用 MPRIS 的 `Position` —— **播放器状态为 Stopped 时 `playerctl position` 永远报 0**。

**另一个坑**：半首歌。如果流式下载直接写真实缓存文件名，失败/取消就会在磁盘上留下一个「看起来完整」的短文件，而 `cache.find` 只查「文件存在吗」，下次播放命中它 → 播到一半 EOF → 判为播完 → 单曲循环重启 → 十几秒一循环。**所以 `.part` + 下完才改名不能省。**
## 4. 改代码时必须守住的不变量
改 `audio/` 下任何东西前，先确认这些没被破坏：

1. **先写盘，再 `push`**。缓冲只留一个窗口，窗口外的字节只能从 `.part` 回读；顺序反了就会读到还没写的空洞（听感是噪声）。
2. **磁盘文件必须以读写方式打开**（`OpenOptions::read(true).write(true)`）。`File::create` 只给 `O_WRONLY`，在这种句柄上 `pread` 是 `EBADF`。踩过一次。
3. **区分读位置与写状态**：`is_finished()`（任务结束）≠ `is_complete()`（整轨完整）。播放引擎靠这个区分「播完」与「下载失败」。
4. **只有真正读不出来时才报错**。已经下好的部分照样能播 —— 后面的失败不能掐断用户已经听到的内容。
5. **取消必须唤醒阻塞的读**。`read()` 阻塞在条件变量上；`cancel()` 必须置标志并 `notify_all`，否则切歌要等到下一次读超时（最长 15 秒）才释放，而音频线程正卡在 `Player::clear()` 里等。
6. **`.part` 不是缓存**。回收与「清空缓存」必须跳过它（`cache::is_partial`）。
7. **结束后不要读 `get_pos()`**（见 §3.2）。
8. **结束/失败/取消都必须丢掉句柄**：`active_stream`（App）、`Runtime::stream`（engine）。留着一个就会把整个缓冲与文件句柄一起拖住。
9. **不要靠改 `WAIT_TIMEOUT` 解决内存**。15 秒是「网络真的坏了、必须报错」的兜底；调大只是让卡顿更久，调小会误判正常等待。
10. **`Output` 的字段顺序不能改**（播放器必须在设备之前），也不要删 `_stream` 字段 —— 它靠生命周期工作，删了声音立刻没。

改 `app/update.rs` 的 `handle_loaded` 前，还要满足：

11. **每个异步结果都必须自证身份**。请求都是 `tokio::spawn` 出去的，返回顺序不保证；期间用户可以重新搜索、打开另一个歌单、点另一个歌手。无条件接受结果会产生「输入框写着 B，列表是 A」这种**看起来像数据出错**的状态，而且不报任何错。判据是「身份比对」，不是给每个请求编号：播放侧早就是这么做的（`App::is_current` 比 `song.hash`，歌词比 `hash`），其余照抄同一思路。现有比对点：

    | 结果 | 比对对象 |
    |---|---|
    | `StreamReady` / `StreamPrerolled` / `StreamCached` | `song.hash`（`App::is_current`） |
    | `Lyric` / `LyricFailed` / `CoverReady` | `hash` |
    | `Search` | 关键词（`state.search.submitted`） |
    | `PlaylistTracks` | 当前打开的歌单 id（`open_playlist`） |
    | `ArtistSongs` / `RankTracks` | 当前打开的歌手 / 排行榜 id |
    | `Playlists` / `Artists` | 请求时的分类 id（`category` / `kind`） |

    判据本身抽成了自由函数（`accepts_search_result` / `accepts_open_item`），因为它们能单独测，而构造 `App` 需要音频引擎与 runtime。**新增带网络请求的 `Loaded` 变体时，先定它返回时跟什么比。** 身份字段必须在 **`spawn` 之前**写进状态（`open_playlist` / `open_artist` / `open_board` 都这么做）—— 写在结果里就太晚了。

改 `audio/downmix.rs`（多声道下混）前，还要满足：

12. **多声道文件必须在进 rodio 之前下混。** rodio 的 `ChannelCountConverter` 在减声道时「每帧保留前 N 个采样，其余丢掉」（`conversions/channels.rs`；单测 `remove_channels` 里 4→1 得到 `[1.0, 5.0]`）。而酷狗**有多声道无损文件**，还可能是「前两个声道不是这首歌」那种：实测《东京不太热 (DJ Z新豪版)》的 `flac` 版是 4.0，歌在最后两个声道（前两个低 6–10 dB，与正确混音的相关性只有 +0.35）。没有这一层，用户听到的不是这首歌，表现为「音频版本与其他客户端不一样」。判据与实测数据在 `downmix` 模块顶部；`build_decoder` 的两个出口都必须过 `downmix::to_stereo`。去掉这一层会有两个守卫测试挂掉：`decoded_multichannel_files_are_downmixed_to_stereo` 与 `the_mixer_hears_the_song_only_because_we_downmixed_first`。

改 `app/desktop.rs`（把播放状态同步给桌面集成）前，还要满足：

13. **每 tick 先比后 clone。** `tick` 每 200ms 调 `sync_ws` / `sync_mpris` / `sync_tray`，它们只拿 `&self.state`：**不要**先把当前曲目、歌手列表或整份歌词 `clone()` 出来再交给句柄比对。`WsHandle::update_from` / `MprisHandle::update_track` / `TrayHandle::update_track` 都是「先比 `hash` / `track_id` 与进度，变了才重建元数据」；传引用 + 闭包就够了；曲目比的是**原始 hash 字符串**（不是摘要），逐字节相等即视为未变。守卫测试：`update_from_wakes_only_when_the_snapshot_changes`、`update_track_skips_metadata_rebuild_for_the_same_track`。

改 `app/playback.rs` 的取流失败路径前，还要满足：

14. **加载失败必须清掉「缓冲中」状态。** `start_playback` 先 `mark_loading()` 把 `PlaybackState` 置为 `Loading`，但引擎在「什么都没加载」时不会改它（`sync()` 见 `!loaded` 直接返回）。如果取流失败而没人处理，状态就永远停在 `Loading` —— 而 `toggle_playback` 恰好忽略 `Loading`，用户按空格没反应，看起来像卡死。两条路径都必须清：同步错误走 `App::abort_loading`，异步失败走 `Loaded::Failed` 里 `playback == Loading` 时的 `audio.stop()`；只在 `Loading` 时动手，避免误停正在播的歌。

改 `app/playback.rs` 的取流失败跳歌、或 `app/cloud.rs` 的云端歌单写入前，还要满足：

15. **只有音源级取流失败才自动跳歌；单曲级失败不跳。** 取流 URL 失败分两类：音源级（连不上、未登录、本地服务没起 —— `is_connectivity` / `is_auth_related` / `Service` / `NonJsonBody`）与单曲级（版权受限、VIP 试听无权限等）。只有音源级走 `Loaded::StreamFailed` → `App::handle_stream_failed` → 自动跳到队列里下一首可播的；单曲级仍然只报错并 `abort_loading`。规则都在纯函数 `auto_skip_next` 里：单曲循环（`RepeatOne`）不跳；同一轮里已确认失败的音源直接跳过；连续跳歌最多 `min(队列长度, MAX_AUTO_SKIP = 5)`；只往后找，不绕回；全失败就停在 `Stopped` 并给一条汇总消息。**用户的任何手动动作（切歌、暂停、播新歌）都会调 `reset_auto_skip()` 把计数清零** —— `play_from` / `play_from_queue` / `toggle_playback` / `next_track` / `previous_track` / `stop_playback` 都调。播放取流失败单独用 `Loaded::StreamFailed`（不是 `Loaded::Failed`），因为预取取流失败与播放的消息完全一样，靠文本分不开。守卫测试：`auto_skip_*`（纯函数）与 `stream_failure_*` / `single_song_failure_does_not_skip` / `manual_track_change_clears_the_auto_skip_state`。

16. **云端歌单的增删同步只接受与歌单同音源的歌。** 歌单接口按「歌单的音源」选 client（`active_source_kind()`）；塞给它另一个音源的歌，酷狗会按 fileid、网易云会按 hash 去找一个不存在（或别人的）条目，把歌单弄坏。所以 `remove_focused_song_from_cloud` / `add_focused_song_to_cloud` / `sync_queue_to_cloud` 都过 `playlist_accepts_song`（`song.source == 歌单的音源`）：跨音源直接拒绝并给消息，不发请求；批量操作按 `partition_by_playlist_source` 逐首分类，只发同音源的，并在消息里说明跳过了几首。**不做跨音源转换**（给酷狗的歌去网易云找同名歌）—— 那是另一个功能。守卫测试：`playlist_guard_*` / `partition_*` / `adding_a_cross_source_song_is_refused_before_any_request` / `syncing_a_mixed_queue_only_sends_matching_songs`。
## 5. 常见坑
**播放 / rodio**
- 解码器读错误 = EOF，没有异常能往上抛。任何「流断了」的判断都必须自己看缓冲状态。
- `Player::stop()` 之后 `get_pos()` 归零（`periodic_access` 里显式写 0）；`Player::clear()` 还会调 `pause()`。
- `Player::clear()` 会等当前音源退出（`sleep_until_end`）。音源若在 `read()` 里永久阻塞，音频线程就跟着卡死 —— 所以缓冲的读超时必须存在。
- `LevelMeter` 包装器必须转发 `try_seek`，否则整个音频变得不可 seek，而播放本身完全正常（极难发现）。
- 队列里只有一首 + 单曲循环 / 列表循环时，「播完」与「从头重放」看起来一样；调试时容易把 bug 当特性。
- **多声道文件在 rodio 里会被「丢声道」**：减声道只保留每帧前 N 个采样，其余丢掉（`conversions/channels.rs`）。酷狗有多声道无损文件，还可能是「歌在最后两个声道」那种（见 §4 第 12 条）。`build_decoder` 现在统一过 `downmix::to_stereo`，不要去掉这一层。诊断：`ffprobe` 看 `channels`，再算各声道与正确混音的相关性。
- **`/song/url` 会静默降级，而 `status` 仍是 1**：实测对没有无损版本的歌请求 `quality=flac`，返回 `extName=mp3` + `bitRate=128000`。只看 `status` 会让界面显示 flac 而实际播 128 kbps。判据在 `catalog::downgrade_note`（按容器 + 码率粗排；`super` 与蝰蛇系列不参与），原因经 `StreamUrl::reason` 呈现给界面 —— `handle_loaded` 里 **`reason` 的两个分支都要处理**，只处理试听那个分支会吞掉降级提示。

**下载 / 缓存**
- `cache.find` 只按「文件存在吗」判断，不校验长度 —— 这就是 `.part` 存在的原因。
- **分块下载的每一步都必须自证**：状态码必须是 `206`，`Content-Range` 必须与请求的区间逐字节一致，每块必须写满长度，总和必须等于总长度。少任何一条，当服务端（或中间 CDN / 代理）忽略 `Range` 返回 `200 OK` 整轨时，4 块会互相覆盖，拼出一个「长度正确、内容全错」的文件 —— 它照样会被改名进缓存。自己写实验代理时别忘了透传 `Range`。回归测试：`audio/download.rs` 里的 `server_ignoring_range_*` 与 `truncated_transfer_*`（用 std `TcpListener` 的假服务端，无新依赖）。
- `.part` 用 `set_len(total)` 预分配，所以**文件大小不能证明完整性**，只有已写字节数能。因此「下载任务结束」≠「下载成功」。
- 分块下载失败会**回退到单连接**重下（`fetch_to`），所以「看起来下了两次」可能是正常的。
- 缓存回收是同步目录扫描，必须放进 `spawn_blocking`；不要阻塞 UI。
- **同一目标不能同时跑两条流**：`start_streaming` 用按临时文件路径索引的注册表去重，第二条复用第一条的缓冲、不起任务。原因是两条流会互相破坏 —— 后者的 `truncate(true)` 抹掉前者写的字节，失败/取消时的 `remove_file` 会删掉对方的文件。触发它不需要异常操作：**连按两次 Enter** 就会对同一目标发起两次 `start_streaming`（`App::active_stream` 在头部缓冲够之前不会持有）。回归测试：`audio::download::tests::a_second_stream_for_the_same_target_reuses_the_first_buffer`。
- 整轨下载与流式下载**不共用**临时文件（`x.mp3.part` 与 `x.mp3.stream.part`，见 §1.4）：后台预取下一首时，用户很可能正好切到那一首。

**列表分页**
- 歌单类接口的 `pagesize` 是**硬上限 30**；响应里的 `count` 只是本页条数，**不是总数**；唯一能算总页数的是歌单元数据里的 `song_count`。
- 「本页没满 ⇒ 就这些了」**只在第 1 页成立**。降序显示时首屏取的是最后一页，最后一页不满是正常的 —— 照抄这条会让「补全整表」永不触发，只显示最后一页那几十首。判据抽在 `app::update::needs_full_fetch`，有测试钉住。
- 更根本的：**「有没有下一页」只能用本页是否为空来判断**。解析会过滤条目（缺 hash、字段类型不对），30 条缩成 29 条很常见，靠「没满」停会静默截断列表。**现在两边用同一判据**：酷狗在 `catalog.rs::collect_all_pages`，网易云在 `source/netease.rs::should_continue_paging`（2026-09-28 改的，见 §7）。网易云那条抽成了自由函数，因为它的单元测试跑不了整条分页流程（需要 HTTP），而判据本身必须能被钉住。
- **并发取回的页必须按页号拼，不能按完成顺序拼**：`JoinSet::join_next()` 给的是任务完成顺序，一批页一起发出去就会按返回顺序追加，整表的页序变随机 —— 对排行榜尤其致命，它的「顺序」就是内容本身。回归测试：`api::catalog::tests::concurrent_pages_are_assembled_in_page_order`（手写假服务端，让页号**必然**逆序返回）。
- 取哪一页由 `app::update::first_screen_page` 决定：**必须与 `sort_descending` 一致**，否则首屏与最终列表头部不一致，画面整屏翻转。
- 首屏取到空页（`song_count` 过期）就回退到第 1 页；不要让用户对着空列表。

**进程 / 环境**
- 默认不记 DEBUG；排障前先 `KUGOU_TUI_DEBUG=1`。
- `session.json` 退出时整体覆盖。自动化测试要么隔离 `XDG_CACHE_HOME`，要么先备份。
- MPRIS 的 `Position` 在 `Stopped` 时无意义（`playerctl` 报 0）。
- 一台机器上跑两个 kugou-tui：后一个拿不到 `org.mpris.MediaPlayer2.kugou-tui`；不要把 `playerctl` 的读数当作唯一证据。
- ALSA 会把插件（`lavrate`/`jack`/`oss`…）报成设备；设备枚举已经做了两轮过滤（可给出默认输出配置 + 排除 `null`）；不要退回「全部列出」。
- 概念版（lite）与标准版对 `ppage_id` 的处理不同；**取直链时不要自己造 `ppage_id`**（见 `api/catalog.rs` 注释：错的会直接给 31863，表现为「所有歌都播不了」）。

**歌词解析**
- **「这一行算不算歌词」的判据只能存在一处**（`api/lyric.rs::has_lyric_content`）：`parse_lrc` 用它决定哪些行进 `Lyric`，`attach_translations` 用它算这些行在**原始带时间轴的行**中的下标，以便从语言轨（`[language:base64]`）取翻译。两边各判一次而判据略有差异，下标就会错一位，**之后每一行的翻译都错** —— 表现为「翻译与文本对不上」，不报错不崩溃。
- **`<` 之前的文本也是正文**：标准 KRC 每行以 `<offset,duration,0>` 开头，所以标记前的文本通常是空的；但歌里出现带 `<` 又没有配对 `>` 的文本时（如 `宝贝<3`），只取标记之后的文本会把整行丢掉。`parse_krc_words` 现在先收集前缀，而前缀没有逐字时间轴，`words` 会被「词数必须等于标记数」那条检查清掉并回退整行高亮 —— 这是设计好的回退，不是 bug。
- 逐字信息**宁可没有也不要猜**：词数与标记数不匹配时清空 `words`，界面回退整行高亮。猜出来的时间轴会让整行唱得与音频不同步。

**渲染 / 布局**
- **`Ord::clamp` 在 `min > max` 时会 panic**（`assert!(min <= max)`）。`area.height == 0` 时 `x.clamp(1, area.height)` 就是定时炸弹，而它看起来完全像防御性代码。先用 `if area.width == 0 || area.height == 0 { return ...; }` 挡住退化区域；**不要**用 `max(1)` 糊过去 —— 得到的框会伸到区域外，随后 `area.width - columns` 下溢。踩过一次：`ui/views/player.rs::fit_box`。
- 反过来也重要：`area.width - columns` 这类减法在「框比区域还大」时下溢，所以算完框要断言**两边都**满足 `box ⊆ area`，而不只是「没 panic」。
- **UI 层生产代码目前没有 `unwrap()` / `expect()`**（全在 `#[cfg(test)]` 里）。一次 panic 就意味着界面没了、终端留在 raw 模式 —— 最贵的一类故障；保持这样。
- **宽而矮**的终端（如 100×8）是最容易压坏布局的形状：走了宽分支但高度不够，`Min` 与 `Length` 抢空间。改完布局跑几个这种尺寸（`TestBackend` + `terminal.draw`，见 `render_home_in_a_short_wide_area_does_not_panic`）。实测空间紧张时 `[Min(6), Length(8)]` 里 `Min` 赢，封面区仍有至少 4 行 —— 但那是**实测行为，不是文档承诺**；升级 ratatui 后要重测。
- **不要每帧重编码登录二维码**：`render_login` 直接复用 `login.qr`（收到二维码时按 `config.qr_aspect` 渲染好，见 `Loaded::LoginQr`），`qr_lines_fitted` 只在已渲染的行上判断「放不放得下」；只有终端放不下时才用 `qr_needed_size` 编一次。一次编码约 0.9ms，在 30fps 的预算里占很大一块（基准：`bench_qr_encoding_cost`、`bench_login_render_cost`）。

**测试**
- 单元测试同进程运行，所以**不要用 `VmRSS` 做断言**（并行测试与分配器缓存会干扰）。要断言「内存有界」，断言**窗口长度**（见 `streaming::tests::keeps_only_a_bounded_window_in_memory`），RSS 级对比放到进程外实验里（§6）。
- 磁盘相关测试必须**以读写方式**打开临时文件（同 §4 第 2 条），否则测试以 `EBADF` 失败。
## 6. 内存对照实验（改完缓冲/下载后跑）
思路：把**同一份** `streaming.rs` 的新旧版本编进一个小程序，跑同样的负载，从 `/proc/self/status` 读 `VmRSS` / `VmHWM`。临时工程放在仓库外，不污染仓库：
```bash
mkdir -p /tmp/mem-repro/src && cd <仓库根目录>
git show HEAD:src/audio/streaming.rs > /tmp/mem-repro/src/old.rs
cp src/audio/streaming.rs            > /tmp/mem-repro/src/new.rs
# main.rs 里：#[path = "old.rs"] mod old; #[path = "new.rs"] mod new;
# 每种负载各跑三遍：单曲 / 连续切 N 次 / 连续切但不取消
```
`streaming.rs` 只依赖 `std`，所以能直接用 `#[path]` 拉进来。

**2026-09 基线数据**（64 MiB 单曲、64 KiB 分块、release、CachyOS）：

| 负载 | 修复前 | 修复后 |
|---|---|---|
| 单曲完整播一遍（峰值 RSS） | 66.5 MiB | 12.5 MiB |
| 连续切 5 次，都不取消 | 323.7 MiB | 3.0 MiB（取消后） / 53 MiB（不取消） |
| 连续播 24 首 | 66.6 MiB（恒定，不随首数增长） | 12.5 MiB（恒定） |

**真实进程**对比（`--search` + 在 pty 里按键 + 采样 `/proc/<pid>/status`）：同样 6 次切歌，修复前 85 MiB 且单调上升，修复后 25 MiB 且稳定；12 次切歌时，修复前峰值 **175.8 MiB**，退出时仍有 145.7 MiB（不回落），修复后峰值 **35.3 MiB**，退出 30.1 MiB。

判断「有没有持续性泄漏」：**播够多首（≥20）看 RSS 是否收敛到同一个值**。修复后连续 24 首稳定在 12.5 MiB，说明每条流的生命周期都被完整清理。
## 7. 已知但**未动**的区域
这一节记录「已复核、确认有问题或可疑，但这次刻意不改」的东西。留着不是忘了；改动的代价或风险大于收益 —— 动手前先读这里的理由。

| 位置 | 问题 | 为什么没动 |
|---|---|---|
| ~~`source/netease.rs::playlist_tracks_all`~~ **（2026-09-28 已修，保留结论）** | ~~按「不满一页 ⇒ 结束」停分页，与酷狗侧「只在空页停」的判据冲突；任何被解析过滤掉一条的页都会静默截断整表~~ | 起了 NeteaseCloudMusicApi（`:3002`）实测越界 offset —— `offset=500`（歌单 200 首）与 `offset=999999` 都返回 `{"songs":[],"code":200}`，**是空数组不是错误**，也不会把 offset 夹到最后一页返回重复。判据抽成了 `should_continue_paging`（`got > 0`），与酷狗侧一致；回归测试 `paging_stops_only_on_an_empty_page` 在改回旧判据时确实会失败。代价：整表加载末尾多一次请求 |
| `source/netease.rs::plaza_playlists` / `artist_list` | 分类（`_category_id`）与地区（`_kind`）参数被忽略，界面却仍显示选择器 | 界面传的是**酷狗**的分类 id；网易云要的是 `cat` 字符串 / `area`，两者对不上。加映射是新功能，不是修 bug |
| `api/catalog.rs::request_song_url_with_hash` | `Song.album_id` / `album_audio_id` 解析了但不发给 `/song/url` | 2026-09-28 在概念版服务上实测四种参数组合（不带 / 带 `ppage_id` / 带搜索来的 id / 带 `/privilege/lite` 来的 id），**都拿到了直链**，而 `status_reason()` 记着一个「privilege 来的 id 给 status=3」的反例。发它们没有可证明的收益、错了代价明确，所以都不发；结论在那里的注释里 |
| `api/model.rs::normalize_duration` | 用 10000 猜单位：9 秒的短曲（9000 ms）被当成 9000 秒；2.8 小时的长合辑（10800 s）被当成 10.8 秒 | 两个方向都在 1000–9999 区间相撞，而实际接口的单位是**已知的**（搜索 `Duration` 是秒，歌单 `timelen` 是毫秒）；这个回退只对没见过的形状生效。改阈值只是把错误从一个区间挪到另一个，不是修复；有测试钉住当前行为，真踩到再说 |
| `config.rs::save` | 直接 `fs::write` 覆盖 `config.toml`，不是「写临时文件 + rename」；理论上崩溃会留下半个配置文件 | 配置文件只有几百字节，一次 `write` 系统调用填不满一个块，实际拿到的是旧的或新的，不会混杂；而且 `load()` 遇到坏文件会带警告回退默认值，最坏也就是「设置丢了，重新登录」。正经做还要处理 Windows 上 `rename` 不能覆盖已存在文件的差异，收益不值得这份复杂度 |
| `audio/download.rs` | 两个**整轨**下载落在同一目标（预取 vs 续传）仍会争同一个 `.part` | 两者从同一个 URL 写同样的字节；最坏是后者的 rename 失败、报一次下载错误。触发链很长（预取在飞 → 切歌 → 那首又坏了）；先把更容易撞的「流式 × 整轨」组合分开就够了 |

**改这些之前**，先看有没有办法在真机上验证 —— 这些条目的共同点是「改得对不对，光靠单元测试看不出来」。
