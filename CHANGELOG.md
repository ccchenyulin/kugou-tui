# 更新日志

本文件记录项目的所有重要改动。
格式基于 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/)。

## [0.6.2] - 2026-10-11

### 变更

- **终端布局重做**：侧边栏去掉每项前的图标，改为紧凑的「指针 / 快捷键 / 标题」结构；删除与标题重复的 `kugou-tui N/N` 行、与播放条重复的「播放」状态块，以及导航下方的电平表；统一面板间距。
  - 侧边栏过矮时折叠成一行状态（如 `● embedded · kugou`），不再留 `… N more …` 占位。
- **可视化页面重做**：柱顶精确到八分之一格，柱内渐变，独立峰值标记，基线，居中曲目信息行；空闲与缓冲各有确定布局。
- **播放条**：右侧按优先级显示分段（循环模式 > 音量 > 下一首），窄终端下循环模式不再消失。

### 修复

- **歌词抓错**：校验候选歌词的时长与歌手/标题，拒绝「不是同一首歌」的候选；找不到匹配时显示「纯音乐，请欣赏」，而不是别的歌的歌词。
  - 同时过滤掉作词/作曲/编曲等制作人员名单，以及「标题 - 歌手」这类表头行。
- 窄终端下曲目列表的标题/歌手/专辑列按可用宽度收缩，必要时整列丢弃，时长不再被挤成 `03:2`。

## [0.6.1] - 2026-10-09

### 修复

- 登录对话框在少于 7 行的终端上不再崩溃（`clamp(7, area.height)` 在 `min > max` 时 panic）。
- 日志重定向失败时不再启动死锁（持有 `SINK` 锁时重复加锁）。
- VIP 到期日遇到非 ASCII `end_time` 时不再切片 panic。
- 系统托盘能检测 D-Bus 连接断开并重连，不再在信号发送反复失败后静默卡死。
- 流式下载任务异常终止时会收尾写入端并注销，读端不再永久阻塞。
- 从播放队列移除曲目后高亮不再错位或越界。
- 网易云服务使用自己的实例名（`api-netease.pid`），不再与酷狗标准版争 PID 文件。
- 若干被静默吞掉的错误现在会记日志（KRC 解码、汽水封面/会员查询），`clienttime` 解析失败改为显式错误。
- 读取 PID 文件容忍 UTF-8 BOM（PowerShell 5.1 的 `Set-Content` 可能写入），Rust 与 `kugou-api`（bash）两侧都处理。
- 音频缓存键加入音源（`<source>-<hash>-<quality>`），不同音源的曲目不再共用一个缓存文件；旧的 `hash-quality` 缓存文件不再匹配，会重新下载一次。
- 流地址解析失败（连不上目标音源、网络中断）不再卡在「缓冲」：清掉加载态，Space 恢复可用。
  - 此前会永久停在 `Loading`，连按 Space 也无反应。

### 变更

- 服务日志文件名统一为 `api-<name>.log`（启动器与 `kugou-api` 一致）。
- 网易云端口 `NETEASE_PORT` 可以像其他端口一样写进配置文件。
- Windows 交叉编译的平台门收紧为 `all(target_os = "linux", target_env = "gnu")`，不再误伤 `*-pc-windows-gnu`。
- Windows 上 npm 安装超时会杀掉整个进程树（`taskkill /T /F`）。
- 目标音源不可用（连不上、未登录、服务未启动）时自动跳到队列中下一首可播曲目。
  - 同一轮内跳过已确认失败的音源，最多跳 `min(队列长度, 5)` 次，单曲循环不跳；手动切歌清零计数；全部失败则停在停止态并给一条汇总提示。
  - 单曲失败（地区限制、无 VIP 试听权限等）仍只报错不跳。
- 云端歌单的增删同步只接受与歌单同音源的曲目：跨音源操作被拒绝并给出提示，异源曲目不再喂给错误的客户端。
  - 批量同步逐曲跳过并说明跳过数量。

### 性能

- 曲目列表与播放队列每帧只构建可见窗口内的行。
- 桌面集成（WebSocket / MPRIS / 托盘）每 tick 先比较再克隆，不再无条件深拷贝当前曲目与歌词。
- 登录二维码复用已渲染的行，不再每帧重新编码（实测每帧省约 0.9 ms）。
- 条目列表（歌单广场、歌手列表等）每帧只构建可见窗口内的行（2000 项实测：577 µs → 251 µs）。

## [0.6.0] - 2026-10-09

### 新增

- **内嵌纯 Rust 接口层**：客户端实际调用的 26 条路由改为进程内实现，**不再需要 Node.js**。此前搜索、流地址解析、歌词、登录依赖本地 Node 服务（KuGouMusicApi）。
  - 这去掉了安装门槛：`cargo install` 只放一个二进制到 `~/.cargo/bin`，不必再准备 Node 运行时。
  - 签名、设备指纹、KRC 解密这些「出错也不报错」的部分逐函数从上游移植，注释标出源文件与函数名，并与上游 JS 在同一组固定输入上逐字节比对（KAT）。
- **内嵌 WebSocket 服务**：推送 `lyrics`（当前歌词与播放位置）和 `playerState`（播放状态），接受 `control` 命令（`toggle` / `next` / `prev`）。
  - 默认开启，只监听 `127.0.0.1:6520`，**不接受非本机连接**，并校验 `Origin`、限制单条消息大小。`--no-ws` 关闭，`--ws-port` 改端口。
- **`node-bootstrap` feature**（默认开）：关闭后编译时不包含引导代码（端口探测、下载 KuGouMusicApi、`npm install`、拉起 `node app.js`）。
  - `cargo build --release --no-default-features` 构建的二进制小约 0.06 MiB；此时 `--api node` 报明确的「该 feature 已在构建时禁用」，而不是含糊的连接失败。

### 变更

- **`--api` 默认值从 `node` 改为 `native`**：启动不再拉起 Node 服务，`ps` 里不再多一个 `node app.js`。
  - Node 后端**保留**作回退，`--api node`（环境变量 `KUGOU_API_BACKEND`）可切回，便于对照与排障。
- **WebSocket 默认开启**：零配置下会多一个监听 `127.0.0.1:6520`。
  - 这是有意的行为变更，不需要就用 `--no-ws` 关闭。
- **`--api-start` 在 native 下改为打印提示并以 0 退出**：native 没有「常驻服务」这回事。
  - 原本往 stdout 打的那行 `interface service ready: <地址>` 是脚本抓地址用的，格式相似但没有地址会让解析脚本拿到垃圾，现在改到 stderr，退出码仍为 0。

### 修复

- **创建云端歌单不再总是报失败**：服务端把新歌单的 `listid` 放在 `data.info.listid`，而解析只扫 `data` 顶层。
  - 症状是「歌单建了但程序说没建」，native 与 Node 两条路径都有此问题，一并修掉。
- **日志中的凭据一律脱敏**：`token` / `userid` / `dfid` / `mid` 在写日志前替换为 `<redacted>`。
  - 覆盖此前泄露的三种形式：出站请求的完整 URL、`reqwest::Error` 的 Display（会把 URL 拼进去）、非 JSON 响应体的前几个字符。
- **主包移除未使用的 `ctr` 依赖**：它只被 `crates/libresoda` 的 `src/soda/crypto.rs` 使用，而该 crate 已声明 `ctr`，主包那行是汽水实现移出时留下的。

## [0.5.0] - 2026-10-03

### 新增

- **汽水音乐音源**，数据面由 [libresoda](https://github.com/sodahub-org/libresoda) 提供（官方客户端 [SodaM](https://github.com/sodahub-org/sodam) 用的同一库）。登录、搜索、流地址解析、解密、歌词全部委托给它，行为与官方客户端一致，上游修复可直接继承。新增 git 依赖 `libresoda`（未发布到 crates.io，按 `rev` 固定）。
  - **直连公网**（`api.qishui.com`），无本地服务。`bootstrap` 不为它做任何准备，首次使用无需 `--api-start`，也不需要 Node.js。
  - **下发的音频是加密的**（MP4/CENC），解析流地址必须先完成「下载整首 → 解密 → 落盘」，所以汽水曲目**不能边下边播**，首次播放要等一次完整下载。这是加密方案决定的，不是实现上的取巧。
- **VIP 完整曲目与无损开箱可用**：汽水的流解析分两层，Web 端点只返回 30/60 秒试听，App 端点需要应用级签名才返回完整曲目。
  - **没有签名时返回「HTTP 200 + 空 body」**，很像「会员没开」，实际是缺签名。
  - 上游 SodaM 内置的公开签名服务默认启用，无需抓包或自建；也可用 `[sources.sodam_app].signer_url` 换成自己的，或设为 `none` 显式禁用（只剩试听与免费曲目）。
- **扫码登录**（按 `L` 选汽水）：使用 libresoda 内置的 **CDP 签名页**直接驱动本地 Chromium，**不需要 Node**。
  - 必须走浏览器，是因为 passport 接口需要 `a_bogus`，且确认后的登录态只存在于该浏览器会话的 cookie jar 里。成功后拿到 **session cookie**，写入 `[sources.sodam].cookie`。
  - 设备指纹**可以留空**（走签名服务时签名方有自己的设备身份），手工填 cookie 仍然可用。
- **降级如实上报**：流解析按「完整优先，再谈音质」挑选。要无损但只有 128k 完整曲目时，选完整曲目并在状态栏说明「实际音质标准」；只有试听的曲目明确说明是否因会员。不会出现界面显示 flac 而实际放 128k 却无从得知的情况。
- **CLI**：`--sodam-cookie` / `--sodam-device-id` / `--sodam-iid` / `--sodam-signer-url` / `--sodam-signer-token` / `--sodam-x-helios` / `--sodam-x-medusa`，各自有 `SODAM_*` / `QISHUI_SIGNER_*` 环境变量。按单项覆盖而非整块替换；空串与纯空白视为「未传」。
- **下载器支持 `file://`**：汽水解密后的输出本地复制进音频缓存，缓存查找、容量回收、下一首预取都照常工作，汽水不需要单独的缓存逻辑。

### 变更

- `SourceKind` 增加 `Sodam`，`SourceSet` 增加 `sodam` 与 `sodam_app` 字段。缺这两节的旧配置回退到 `Default`，`normalize()` 填入公网地址并**置为启用**。
- 音源管理页把公网音源标注为「直连公网」，避免用户以为应该有本地服务在跑。
- 启动器脚本（bash 与 PowerShell）增加 `sodam` 分支并引入 `NEEDS_SERVICE`：选汽水时跳过整条探测/拉起/等就绪流程。
  - 此前会落进回退分支，尝试拉起 KuGouMusicApi 并报「未运行，正在启动…（端口 3000）」。
- **云端歌单、歌单详情、用户资料、会员状态全部接通**：此前对汽水返回「不支持」，症状是「登录了但什么都不工作」，真实原因是分发层没接，而不是汽水缺这些接口。
  - 现在走 libresoda 的公开实现（`get_user_playlists` / `get_playlist_songs` / `fetch_pc_me` / `is_vip_account`），Capability 的 catalog / cloud / vip 都置 true。
  - **三个功能确实没有对应端点，保持显式报错**（已对照 libresoda 代码核实，不是遗漏）：歌单广场（`get_recommended_playlists` 与 `get_playlist_categories` 上游为 `Unsupported`，只有歌单搜索可用）、歌手列表（只能按 id 查，没有热门歌手列表）、排行榜（无对应端点）。错误文案说明原因与替代方案。

### 修复

- **保存配置不再覆盖汽水的登录凭据**：`Config::save()` 第一步调用 `sync_active_source()`，会用顶层 `cookie`（酷狗的地盘，汽水恒为 `None`）覆盖音源 profile 里的 cookie。
  - 表现为**登录成功 → 立刻保存 → 被覆盖**，重启后显示未登录，且配置文件里找不到 cookie 键。汽水不再参与镜像，酷狗/网易云旧路径不变，两侧都有钉住测试。
- **登录后立刻查会员/资料不再失败**：`apply_server_cookie` 原先写入 `self.client_for(Sodam)` 再 `set_cookie`，但音源已激活时 `client_for` 返回的是 `self.api.clone()`，改副本对 `self.api` 无效，后续请求仍用没有 cookie 的客户端，服务端回 `1000016 login state expired`。
- **二维码改为纯黑白**：原先用 ANSI 调色板 0/15（`Color::Black` / `Color::White`），而**终端可以重映射**（kitty 允许改 ANSI 0-15），「黑色模块」可能被画成灰色，二维码发虚扫不出来。
  - 看起来像终端或字体问题，改为硬编码纯黑与纯白 RGB。不跟主题是有意的。
- **二维码缩小**：模块数只由「内容长度 + 纠错等级」决定，内容（服务端下发的扫码地址）改不了，所以降低纠错等级是唯一的杠杆：M（约 15%）→ L（约 7%）；屏幕显示是理想条件，L 够用；静区也从 2 模块减到 1。
  - 实测：汽水（193 字符地址）从约 59×30 降到 51×26。汽水地址比酷狗长 5 倍，本来就更大，不是渲染问题。
- **二维码自适应终端尺寸**：汽水扫码地址远长于酷狗，二维码版本更高，对话框尺寸按二维码算，于是撑满整个终端。
  - 二维码不能缩放（行数由内容长度决定，压密只会拉坏比例扫不出来），改为先判断放不放得下：放不下就不画，提示「约需 N 列 × M 行」，并给两条出路（放大终端重试，或手工填 cookie）。画一个被裁掉的二维码比不画更糟。
- `ApiClient` 增加 `cookie()` 访问器：音源模块需要用登录态重建自己的客户端，「存在什么身份凭据」是通用信息，不该被 HTTP 层藏起来。

## [0.4.8] - 2026-10-02

### 文档

本轮**只动文档**，代码与 0.4.7 完全相同。需要单独发版，是因为 crates.io 页面渲染的是**包内的 README**，不发版页面就不变。

- **全部文档围绕「这是写给谁的」重写**，合计 1844 → 1473 行：
  - README 192 → 162，删掉硬塞进去的实现论证（Windows 为什么不用 CMake / NASM、边下边播的 4 MiB 窗口、内存测量方法），安装部分改为以 cargo 开头；
  - INSTALL 590 → 425，删掉与内置功能重复的 fish 流程（探测/拉起/收尾已由程序自己做，只差一个 `git pull`），压缩 Windows 与 macOS 部分的解释性段落；
  - CONFIGURATION 318 → 256，每个配置项下展开的「为什么」压成一句，论证留给 DESIGN.md；
  - USER_GUIDE 335 → 230，合并界面布局与桌面集成里的重复描述；
  - FAQ 新增 0.4.7 引入的四个真实问题：找不到 node、为什么首次启动要等半分钟、服务起不来/端口被占、内存只涨怎么排查；
  - CONTRIBUTING 补全代码结构树（`bootstrap.rs` / `mpris.rs` / `tray.rs` / `window.rs` / `source/` 等），发布一节补上 crates.io 步骤（`scripts/release` 不含）；
  - `DESIGN.md` 与 `MAINTENANCE.md` **有意不动**，它们是「为什么」的档案，细节本身就是价值。
- 文档里的事实做了一轮机检：43 条内部链接与锚点、14 处 `kugou-tui --xxx` 用法（与 `cli.rs` 的 17 个长开关对齐）、所有 `KUGOU_*` 环境变量与配置键名与代码一致。

## 早期版本

完整历史见 `git log`。

- **[0.4.7] - 2026-10-02** —— 接口服务自动引导（新增 `src/bootstrap.rs`）：首次启动自行准备并拉起 KuGouMusicApi，先探测并复用已有服务，退出时清理自己启动的实例；新增 `api_auto_start` / `api_dir` 配置与 `--api-start` / `--api-stop`，发布到 crates.io，`cargo install kugou-tui` 可用。同时修掉「按 Space 或 Enter 重试」是空头支票、播完不取消下载任务、mmap 阈值 1 MiB 设得过高导致内存只涨（改 256 KiB）、MPRIS 推送不稳定（`mpris:trackid` 固定、发送失败被吞、Position 走 `PropertiesChanged`）。
- **[0.4.6] - 2026-09-29** —— 系统托盘升级为完整集成：滚轮调音量与快进、菜单加「退出」、图标随播放状态变化、注册每 5 秒自愈、图标改为双联音符。新增 `audio/resample.rs`，用 rubato sinc 抗混叠重采样，修掉 hi-res 母带比其他客户端「更糙」；另修进度条只能向前拖（无 SEEKTABLE 的 FLAC 无法后退，改为重建解码器）、点歌词总是跳相邻行、多声道、mp4 混入 flac 档位、内存超 100 MB 不回落。
- **[0.4.5] - 2026-09-29** —— 新增 `audio/downmix.rs`：4.0 声道 FLAC 里歌在最后两个声道，而 rodio 的 `ChannelCountConverter` 只保留前两个，听到的不是这首歌；改为偶/奇声道平均下混成立体声，相关性从 +0.35 升到 +0.998。同时把 `/song/url` 的静默降级如实上报（`quality downgraded: requested flac, server actually gave 128 kbps mp3`），候选音质不再跨版本混用（先按 `album_audio_id` 过滤），打包脚本进入常规 CI。
- **[0.4.4] - 2026-09-28** —— 拆分 `app/update.rs`（4728 → 2958 行），纯搬移零逻辑改动，先后拆出 `app/navigation.rs` / `playback.rs` / `search.rs` / `cloud.rs` / `settings.rs` / `desktop.rs`，依赖保持星形无互调；新增 Windows 侧服务管理 `scripts/kugou-api.ps1`、`kugou-api logs`、启动器 `--dry-run` 与 node 检查。另修网易云歌单/排行榜静默截断、`;` 无法重绑、歌词翻译整体错行、封面「不变形」模式在退化区域 panic、Windows PowerShell 脚本乱码、并发分页顺序错乱、同一首歌两个流式下载互相破坏、分块下载写坏缓存。
- **[0.4.3] - 2026-09-27** —— Windows 与 macOS 发布包交给 CI（`release.yml`，支持 `workflow_dispatch` 回填）；新增歌词逐行切换过渡动画（`lyric_anim_ms`，默认 `200`，0 关闭）与点击歌词行跳转；`scripts/make-release-tarball` 兼容 macOS（BSD `readlink`、`shasum -a 256`、bash 3.2）。
- **[0.4.2] - 2026-09-27** —— 正式支持 Windows（10/11 x86_64）：修掉 `pread`、`libc::dup2`、D-Bus 三处编译错误，修 `~` 展开只认 `HOME` 与 `--print-config` 谎报托盘状态，TLS 后端在 Windows 换系统 SChannel（免 CMake + NASM），新增 `scripts/build-windows.ps1`、`kugou-tui.ps1`、`kugou-api-install.ps1` 与三平台 CI。修正 MSRV 声明 1.86 → 1.90，清掉因此暴露的 29 条 clippy 建议。
- **[0.4.1] - 2026-09-26** —— 修复程序一运行就独占声卡、麦克风与扬声器失声且退出后不恢复：设置页里名为「Default Audio Device」的设备其实是 ALSA 的 `sysdefault`（`plughw:0` 直连别名），`open_default_sink()` 还会在默认设备打不开时抢占第一个能打开的硬件设备；现在先探测声音服务器，有则只列 `default` / `pipewire` / `pulse`。退出时音频线程的 `join` 改为 3 秒有界等待。
- **[0.4.0] - 2026-09-26** —— 修复常驻内存涨到上百 MB 不回落：流式缓冲改为只保留读指针附近 4 MiB 尾部窗口，窗口外经 `pread` 回读磁盘，切歌/停止/退出主动取消旧任务（5 次切歌 RSS 323.7 → 3.0 MiB）；修播放位置跳回开头（下载完成不再重载、断流不再当作播完）；下载先写 `.part`，完整才改名。打开歌单时首页改为取最后一页，新增 `docs/MAINTENANCE.md`。
- **[0.3.10] - 2026-09-25** —— 修复歌手列表只显示第一组（`/artist/lists` 按首字母分组返回，解析把组包装当成了歌手，实际应有 1054 位）与点进歌手零首歌（`/artist/audios` 的 `data` 本身就是歌曲数组）；API 端口上跑着别的服务时给出可读错误 `AppError::NonJsonBody`。新增一键发版 `scripts/release`、构建中间产物回收 `scripts/clean` 与 `docs/RELEASE.md`。
- **[0.3.9] - 2026-09-25** —— `CONTRIBUTING.md` 新增「Release」一节，把工具脚本必须先提交、上游 commit 在两处固定、发版后必须下载回验等坑写成步骤；修正内存数字（7.0 MiB / 14–17 MiB）；补充「一个上游可能有多种字段布局」。
- **[0.3.8] - 2026-09-25** —— `kugou-tui-install-api` 改用 `npm install --omit=dev`，不再装一堆用不上的 devDependencies（同机实测：全量 311 个包，仅生产 120 个）；`docs/INSTALL.md` 新增「方式三：预编译包」，Release tarball 起包含三个脚本与全部文档。
- **[0.3.7] - 2026-09-25** —— 支持「发行版提供的接口服务」：包把服务与生产依赖装进 `/usr/share/kugou-tui/api/<source>`（只读），脚本优先使用，可用 `KUGOU_API_SYSTEM_ROOT` 覆盖。修复全新安装时启动器静默退出（`pipefail` + `set -e` 下 `sed | head` 因配置文件不存在而失败）。
- **[0.3.6] - 2026-09-25** —— 修复启动器脚本被安装到系统目录后找不到客户端二进制：两个脚本原先只按「脚本目录 `/../target/release/kugou-tui`」定位，复制到 `/usr/bin` 后变成 `/usr/target/...`；现在增加 `PATH` 回退，仍可用 `KUGOU_TUI_BIN` / `KUGOU_API_BIN` 显式指定。
- **[0.3.5] - 2026-09-25** —— 修复网易云歌单内点歌永远 404（首页路径绕过音源分发，直接调了酷狗的 `/playlist/track/all/new`）、歌单与歌手页歌曲无歌手无专辑且时长 `00:00`（`/playlist/track/all` 与 `/artists` 用 `ar`/`al`/`dt` 而非 `artists`/`album`/`duration`）、云端歌单总报「未登录」（服务端不切分 `Set-Cookie` 里的 `;;`）；写入接口不再重试。README 拆出精简首页版（295 → 129 行），新增 `docs/INSTALL.md`、`docs/LICENSES.md`、`docs/DISCLAIMER.md`。
- **[0.3.4] - 2026-09-25** —— 新增可选输出设备：设置页最后一项显示**实际打开的声卡**，可在「系统默认」与枚举设备间切换（写入 `audio_device`），切换时短暂暂停后从原位置续播，新设备打不开则继续用旧的；候选列表按「能给出默认输出配置」过滤、排除 `null`、按名去重。「播放但没声音」现在能一眼看出声音交给了谁。
- **[0.3.3] - 2026-09-24** —— 新增系统托盘：注册为 `org.kde.StatusNotifierItem`，悬停显示当前曲目，右键弹出菜单（`com.canonical.dbusmenu`），三级降级静默跳过；菜单里新增「最小化/显示窗口」，借 niri 的 `toggle-window-minimized`，按 OSC 0 设置的标题精确匹配窗口。修复音频后端错误污染 TUI 画面（进入 TUI 前把 stderr 重定向到日志）。README 换成真实终端截图 `assets/screenshot-0.3.3.jpg`。
- **[0.3.2] - 2026-09-24** —— 修复 `f` 无法通过 `[keymap]` 重绑（`parse_key` 的 F1–F12 分支把单字母 `f` 也匹配了）；`scripts/kugou-api-install` 改用 `git fetch --depth 1 origin <sha>` 固定上游 commit；`scripts/kugou-api` 新增 restart、PID 管理与启动预检。CONFIGURATION.md 补上 `[keymap]` 自定义快捷键一节。
- **[0.3.1] - 2026-09-24** —— 修复可视化空态把「已停止」说成「已暂停」（两者 Space 的行为不同：停止是从头播，暂停是继续）；侧边栏「清缓存」快捷键与目录名粘成一个词（`C清理 kugou-tui`），改为 `[C]` 括号风格。
- **[0.3.0] - 2026-09-24** —— 新增瞬时网络故障自动重试（读接口共 3 次尝试，间隔 300 ms → 900 ms；超时、业务错误码、其他 4xx、200 非 JSON 不重试），写接口显式不重试（`get_json_mutating` / `get_json_uncached_mutating`）；帮助面板可滚动（`j`/`k`、`↑`/`↓`、`PgUp`/`PgDn`、`g`/`G`）；启动不再无条件打印「已连接」，连通性由真实请求结果驱动。
- **[0.2.0] - 2026-09-22** —— 新增设置页（`,` 打开，立即写回 `config.toml`）、6 套主题、曲目下载（`W`）、逐字歌词（KRC 的 `<offset,duration,0>char`）、分块并发下载（128kbps 实测 1.41s → 1.10s）、下一首预取、`cover_fill` 配置与每日领取概念版 VIP。移除「歌词」「封面」两个标签页，数字键 `8` / `9` 改派给音源与设置（`1`–`7` 与 `0` 不变）。
- **[0.1.0] - 2026-09-20** —— 首个可用版本：MPRIS 桌面集成（`org.mpris.MediaPlayer2.kugou-tui`）、音频可视化（第 6 个标签，数据来自音频线程记录的真实峰值）、歌词翻译/音译（解析 KRC 的 `[language:base64]` 标签，请求从 `fmt=lrc` 改为 `fmt=krc`）。修复音源回退错乱、搜索结果标题为空（`/search` 的字段是 `FileName`）、可视化页方向键完全无响应、音源 profile 污染。依赖的 KuGouMusicApi 锁定 `a5a9801`。
