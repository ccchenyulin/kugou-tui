# 常见问题

## 接口服务

**问：启动时说找不到 node？** **答：**只在 `--api node` 回退路径上出现——它运行第三方服务 KuGouMusicApi，用 Node.js 写成。装上 Node.js（≥ 12，带 npm）重启即可；程序自己会准备并拉起该服务，不用克隆任何仓库。**默认的内嵌后端（`--api native`）不需要 Node.js**，不打算用回退就加 `--api native`；连引导代码都不想要，就用 `--no-default-features` 构建。

**问：首次启动卡了半分钟，正常吗？** **答：**只在 `--api node` 下如此。首次运行会下载 KuGouMusicApi（固定提交）并跑 `npm install --omit=dev`，实测约 25 秒；之后每次启动探测到端口就复用。想提前做：`kugou-tui --api node --api-start`。内嵌后端没有这一步，启动即用。

**问：服务起不来 / 端口被占？** **答：**看日志 `~/.cache/kugou-tui/api-standard.log`（概念版为 `api-lite.log`）。端口被别的程序占用时程序不会去抢，直接报错退出。用 `kugou-tui --print-config` 看它探测哪个地址，或用 `-a http://127.0.0.1:3100` 切换。想完全自己管服务，设 `api_auto_start = false`（或 `--no-api-start`）。

**问：怎么配置概念版音源？** **答：**平台由服务端的 `platform` 环境变量决定（`lite` = 概念版），而一个 Node 进程只能加载一份配置，所以**概念版需要独立实例**：

```bash
cd KuGouMusicApi && platform=lite PORT=3001 node app.js
# 或者用仓库脚本一次启动两个：kugou-api start
```

> 用 `cargo install` 装的程序**不需要手动做这件事**：切到概念版时它会自动拉起该实例。

**问：切到「酷狗概念版」后为什么还是只能听试听？** **答：**多半是**概念版没登录**。两个平台的 token 不通用（上游文档原文：「不同版本的平台的 token 是不通用的」）。在标准版扫的登录态拿到概念版用，服务端不认，会员信息为空，播放就降级成试听。切到概念版后按 `L` **重新扫码**。验证命令：

```bash
curl -s -H "Cookie: $(rg '^cookie' ~/.config/kugou-tui/config.toml | sed 's/^cookie = "//; s/"$//')" \
  "http://127.0.0.1:3001/user/vip/detail" | jq '.data'
```

返回 `status: 0` 说明这个实例还没登录。

**问：怎么知道接口服务通没通？** **答：**先看侧边栏第一块的标题：`--api native` 下是「接口 · 内嵌」——接口就在进程里，不存在连不通，这一问只对 `--api node` 成立。node 下标题是「连接 · 已连通 / 未连通 / 未验证」，它由**真实请求的结果**驱动，不是启动时猜的：`未验证` 表示此刻还没发过请求，`未连通` 表示最近一次请求被拒绝或超时。显示 `未连通` 就是服务没跑或地址不对（用 `--print-config` 看它连的哪个地址）。

> 业务类错误码如「需要登录」「页码越界」算**已连通**——服务答了，只是拒绝了那次请求。

## 播放

**问：不登录能听吗？** **答：**能。浏览歌单广场 / 歌手 / 排行榜和播放都不需要登录，只有搜索和云端歌单需要。

**问：搜索没反应 / 提示需要登录？** **答：**未鉴权的 `/search` 返回 `error_code: 152`；按 `L` 扫码。注意 KuGouMusicApi 在业务层失败时会把错误码放进响应体，而 HTTP 状态可能是 `502`，别看到 502 就以为服务挂了。

**问：获取播放地址失败，提示「本次请求需要验证」？** **答：**通常是缺 `dfid`。程序首次启动会自动获取；配置里没有就删掉配置让它重新获取。用 MV 的 hash 或 `hash_multitrack` 去取音频地址也会报这个，但正常路径用 `audio_info.hash_128`，不会触发。

**问：一首歌开始播放要等很久？** **答：**实现是边下边播：拿到直链后先缓冲开头足够的部分（128 KB，约 8 秒音频）就开播，其余在后台下载到磁盘。所以等待是「缓冲开头」而不是「下整首」。想更快：**降低音质**（`320` 约 10 MB，`flac` 约 30 MB，`high` 约 65 MB，受文件体积的物理限制）；直链支持 Range 时会**最多 4 路并发分块下载**；当前曲目开播后会**预取队列的下一首**；同一首再播直接命中缓存。

> 边界情况：流的读指针走到尚未下载的位置时会**阻塞等数据**，表现为声音停顿后继续（而不是提前结束）。网络抖动严重时能听出来。

**问：进度条在走、状态是「播放中」，但完全没声音？** **答：**按 `,` 打开设置页，看最后一项「输出设备」——它显示的是**实际打开的那张卡**，几乎可以肯定不是你正在听的那张：

```bash
ls -l /proc/$(pgrep -x kugou-tui)/fd | grep snd   # 有输出 = 直连硬件，绕过了 PipeWire
pactl list sink-inputs                            # 播放时看不到自己 = 确实被绕过
```

根因几乎总是系统的 ALSA 配置：`/etc/asound.conf` 或 `~/.asoundrc` 把 `default` 写死到某张卡（如 `pcm.!default { type hw card 2 }`），而**卡号会变**。把 `default` 指回 PipeWire（`type pipewire`，或删掉该文件）。见 [CONFIGURATION.md](CONFIGURATION.md#输出设备audio_device)。

**问：歌词不显示 / 不同步？** **答：**歌词来自接口；没有就是没有。只是时间轴偏了就用 `[` / `]` 按 100 ms 步进调整，偏移量会存进配置。

**问：长时间听之后内存只涨不降？** **答：**0.4.7 修过一轮（mmap 阈值设得过高，封面位图落到阈值以下进了 brk arena，free 后页没还给系统）。如果看着还是高，用 `KUGOU_TUI_MEM_TRACE=1 kugou-tui` 启动；日志里每 5 秒一行 RSS，同时带「流式任务数 / 队列长度 / 缓存容量」，用来区分「长期持有的东西」和「还没归还的页」。另外 `lite_mode` 会关掉封面解码，那是内存占用最大的一块。

## 界面与数据

**问：方向键 / 数字键没反应？** **答：**先确认是不是在搜索框输入模式里——那里字母键是输入文字，按 `Esc` 退出输入模式再试。确认不是的话，用 `KUGOU_TUI_DEBUG=1 kugou-tui` 启动；日志会记录每个键映射到什么操作，能区分「键没到程序」还是「映射到了空操作」。

**问：滚轮滚的是页面而不是列表？** **答：**程序启用了鼠标捕获。如果终端额外做了鼠标转发（某些 tmux 配置），可能冲突；用终端自己的设置关掉。

**问：为什么打开几百首的歌单要几秒？** **答：**歌单接口**每页硬上限 30 条**——传 `pagesize` 100 甚至 500，服务端照样只回 30 条，且响应里的 `count` 只回显当前页的条数，不是总数。所以客户端只能翻页：实测 385 首的歌单要 13 次请求。排行榜和歌手歌曲同理。搜索结果和歌单广场则按 `page_size`（默认 30）一次取一页。

**问：歌单显示的歌曲数和实际不符？** **答：**歌单元数据里的 `percount` 和 `songcount` 实测恒为 `0`，不能用来显示。界面上的「N 首」是**实际取到的条数**，可信。

**问：缓存占空间太多？** **答：**默认上限 512 MiB，超过就按 LRU 从旧到新回收。改 `cache_limit_mib`；`--cache-limit 0` 表示不限。

## 平台相关

**问：Windows 上怎么装？为什么 `./scripts/kugou-api-install` 跑不了？** **答：**那些脚本是 bash 的，PowerShell 里跑不了。Windows 上用对应的 `.ps1`：

```powershell
.\scripts\kugou-api-install.ps1
.\scripts\kugou-tui.ps1
```

完整步骤（工具链前置条件、终端要求、平台差异表）见 [INSTALL.md](INSTALL.md#在-windows-上构建与运行)。

**问：Windows 构建报 `linker link.exe not found` / `cmake not found` / `nasm not found`？** **答：**第一个是 MSVC 工具链没装（装 Visual Studio Build Tools 并勾选「使用 C++ 的桌面开发」）。后两个**不该出现**：Windows 上 TLS 用系统自带的 SChannel 而非 rustls，那一堆 `aws-lc-sys` 的 C 代码根本不会编译——真遇到了，说明你构建的不是本仓库当前代码。

**问：Windows 上没有托盘图标 / `playerctl`？封面为什么是色块马赛克？** **答：**都不是故障。托盘和 MPRIS 是 D-Bus 接口；Windows 没有会话总线，相关代码**不编译**（`--print-config` 会打印「系统托盘 : 不可用」）。封面是**半块字符画**——程序检测到终端不支持 Kitty / iTerm2 图形协议就自动降级，Windows Terminal 两者都不支持（启用 `lite_mode` 可跳过这部分开销）。见 [INSTALL.md 的功能差异表](INSTALL.md#6-与-linux-的功能差异)。

**问：图标变成方块 / 问号？** **答：**终端字体缺字形。程序先探测 Nerd Font，没有就回退 ASCII 图标；但 **Windows 没有 fontconfig，探测必然失败**，装了 Nerd Font 也不会自动识别——设 `KUGOU_TUI_NERD_FONT=1` 强制启用（在 Linux / macOS 上这个变量也会覆盖探测结果）。

**问：支持 macOS 吗？** **答：****代码支持、CI 覆盖，但未在真机长期使用**。它走与 Linux 相同的 `cfg(unix)` 分支，音频走 CoreAudio，构建只需 Xcode 命令行工具。已知差异：没有 MPRIS / 系统托盘，没有窗口最小化（用的是 niri 的合成器 IPC），配置在 `~/Library/Application Support/kugou-tui/`，缓存在 `~/Library/Caches/kugou-tui/`（**不是** `~/.config` 和 `~/.cache`），Terminal.app 下封面是半块字符画（iTerm2 可用图形协议）。详见 [INSTALL.md](INSTALL.md#在-macos-上构建与运行)。
