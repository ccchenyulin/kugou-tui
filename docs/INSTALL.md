# 安装

本文补充 [README](../README.md)：环境要求、各平台安装方式、第三方接口服务部署、启动器脚本与环境变量、装完之后做什么。

**先看这条**：接口实现**内嵌在二进制里**，默认路径（`--api native`）**不需要 Node.js**。只有 `--api node` 回退（或网易云音源）才需要本地第三方接口服务，见「[部署第三方接口服务](#部署第三方接口服务)」。

---

## 环境要求

| 项目 | 要求 |
|---|---|
| Node.js | **仅 `--api node` 回退需要**，≥ 12（带 npm），用于运行 KuGouMusicApi。`--api native` 或 `--no-default-features` 构建不需要 |
| Rust 工具链 | 1.90+（edition 2024）。下限来自**依赖**（`quantette` 要求 1.90），不是本项目代码 |
| 音频输出 | rodio 支持的后端：Linux 为 ALSA / PipeWire / PulseAudio，Windows 为 WASAPI，macOS 为 CoreAudio |
| 终端 | UTF-8；完整主题配色与逐字渐变需要**真彩色（24 位）**。老终端可用 `--basic-color` 回退到 16 色 |
| 系统 | **Linux**（主要平台，在 CachyOS 上测试）、**Windows 10/11 x86_64**、**macOS**（CI 覆盖，未在真机长期使用） |
| 构建依赖（Linux） | `alsa-lib` 开发头文件 + `pkg-config`（`cpal` 通过 `alsa-sys` 链接）。Debian/Ubuntu：`libasound2-dev pkg-config`；Arch：`alsa-lib`。桌面发行版通常已有，**干净的容器 / 服务器没有** |
| 构建依赖（Windows） | 只需 MSVC 工具链。**不需要 CMake、NASM、OpenSSL** |
| 构建依赖（macOS） | 只需 Xcode 命令行工具（`xcode-select --install`） |
| D-Bus（可选） | 有会话总线时自动启用 MPRIS 与系统托盘；没有则跳过，不影响播放。Windows 上这段代码不编译 |
| 系统托盘（可选） | 需要提供 `org.kde.StatusNotifierWatcher` 的状态栏（Quickshell / waybar / KDE 都有）。比面板晚启动也没关系，托盘线程每 5 秒重试注册 |

---

## 安装方式

### 方式一：`cargo install`（crates.io）

```bash
cargo install kugou-tui
kugou-tui --api native      # 内嵌后端，不需要 Node
```

内嵌后端开箱即用，无需首次配置服务。只有 Node 回退（或网易云音源）需要下面的准备：首次启动会自行准备并拉起接口服务（下载固定的上游提交 → `npm install --omit=dev` → 启动，实测约 25 秒），之后启动探测到端口就复用。

| 项目 | 说明 |
|---|---|
| 首次启动（仅 `--api node`） | 需要网络。想提前做：`kugou-tui --api node --api-start`（它拉起的服务留在后台；`--api-stop` 停止） |
| 服务装在哪 | 优先 `/usr/share/kugou-tui/api/kugou`（发行版打包）和已存在的 `~/KuGouMusicApi`；两者都没有才装到 `~/.local/share/kugou-tui/api/kugou` |
| 自己管理服务 | 设 `api_auto_start = false`，或传 `--no-api-start` |
| 退出行为 | 它启动的实例随退出而停止，不留常驻 node；已在运行的实例（你或其他实例启动的）不动 |
| 彻底移除引导代码 | `cargo install kugou-tui --no-default-features`（关闭 `node-bootstrap` feature；二进制里不含下载器 / npm / node 拉起代码） |

> 这条路径不含仓库的 `scripts/*`（不属于 crate）。要那套一键脚本，用下面的预编译包。

### 方式二：从源码构建

```bash
git clone https://github.com/sijin-xb/kugou-tui.git
cd kugou-tui
cargo build --release
./target/release/kugou-tui --help
```

release 产物约 **11.7 MiB**（`opt-level="z"` + fat LTO + strip）。Windows 上把二进制路径换成 `.\target\release\kugou-tui.exe`；前置条件与脚本见「[在 Windows 上构建与运行](#在-windows-上构建与运行)」。

### 方式三：预编译包（GitHub Release）

[Releases](https://github.com/sijin-xb/kugou-tui/releases) 提供三个平台，内容一致（二进制 + 脚本 + 全部文档）：

| 平台 | 文件 |
|---|---|
| Linux x86_64 | `kugou-tui-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| Windows x86_64 | `kugou-tui-<version>-x86_64-pc-windows-msvc.zip` |
| macOS arm64 | `kugou-tui-<version>-aarch64-apple-darwin.tar.gz` |

> Intel Mac 没有预编译包（CI runner 是 arm64），请从源码构建。

**Linux**：

```bash
tar xzf kugou-tui-<version>-x86_64-unknown-linux-gnu.tar.gz
cd kugou-tui-<version>-x86_64-unknown-linux-gnu

# 把二进制和脚本链进 PATH。`scripts/kugou-tui` 与二进制同名，链接时改名
mkdir -p ~/.local/bin
ln -s "$PWD/kugou-tui"                 ~/.local/bin/kugou-tui
ln -s "$PWD/scripts/kugou-api"         ~/.local/bin/kugou-api
ln -s "$PWD/scripts/kugou-api-install" ~/.local/bin/kugou-tui-install-api
ln -s "$PWD/scripts/kugou-tui"         ~/.local/bin/kugou-tui-launch

kugou-tui-install-api kugou   # 只需一次：拉取并配置接口服务
kugou-tui                     # 开始播放
```

**Windows**：

```powershell
Expand-Archive kugou-tui-<version>-x86_64-pc-windows-msvc.zip -DestinationPath .
cd kugou-tui-<version>-x86_64-pc-windows-msvc
.\scripts\kugou-api-install.ps1   # 只需一次
.\scripts\kugou-tui.ps1           # 开始播放
```

**macOS**（脚本是 bash，用法与 Linux 相同）：

```bash
tar xzf kugou-tui-<version>-aarch64-apple-darwin.tar.gz
cd kugou-tui-<version>-aarch64-apple-darwin
./scripts/kugou-api-install kugou
./scripts/kugou-tui
```

> 包自包含（含脚本与文档），但**不含接口服务本身**：`kugou-api-install` 需要 `node` / `npm` / `git` 和网络。Windows zip 里也带了 bash 脚本（给 Git Bash / WSL 用）；Linux / macOS 的 tar 包不含 PowerShell 脚本。

## 在 Windows 上构建与运行

日常使用**不需要手动启动服务**：`scripts/` 下的 PowerShell 脚本与 Unix 上的 bash 脚本一一对应。

| Windows | Unix 对应 | 用途 |
|---|---|---|
| `scripts/kugou-api-install.ps1` | `kugou-api-install` | 拉取接口服务 + 装依赖（一次） |
| `scripts/kugou-tui.ps1` | `kugou-tui` | 启动器：需要时确保服务在跑，然后启动播放器 |
| `scripts/kugou-api.ps1` | `kugou-api` | 服务生命周期：`start` / `stop` / `restart` / `status` / `logs` |
| `scripts/build-windows.ps1` | `make-release-tarball` | 构建 + 打包 zip |

这些脚本**只依赖 PowerShell 5.1**（Windows 自带），不需要单独装 pwsh 7。它们与启动器共用 PID 文件（缓存目录下的 `api-<instance>.pid`，内容为 `<PID> <port>`），因此启动器拉起的实例能被 `kugou-api.ps1` 识别并停止，不用在任务管理器里按名字猜 node 进程。

### 1. 安装工具链

| 项目 | 做法 |
|---|---|
| Rust | [rustup.rs](https://rustup.rs) → 选 `x86_64-pc-windows-msvc` |
| MSVC 工具链 | Visual Studio Build Tools，勾选「使用 C++ 的桌面开发」；装了 VS 则已有 |
| Node.js | [nodejs.org](https://nodejs.org) |
| Git | [git-scm.com](https://git-scm.com)；安装脚本用它克隆 |

> **不需要 CMake / NASM / OpenSSL**：Windows 上 TLS 用系统自带的 SChannel 而非 rustls（rustls 的 `aws-lc-sys` 是 C 代码，需要 CMake + NASM）。见 `Cargo.toml` 里的 `cfg(windows)` 注释。

### 2. 构建

```powershell
git clone https://github.com/sijin-xb/kugou-tui.git
cd kugou-tui
cargo build --release
.\target\release\kugou-tui.exe --help

.\scripts\build-windows.ps1        # 打包：dist\kugou-tui-<version>-x86_64-pc-windows-msvc.zip
```

### 3. 拉取接口服务（只需一次）

```powershell
.\scripts\kugou-api-install.ps1
# 等价于：克隆到 %USERPROFILE%\KuGouMusicApi → 固定到已验证的提交 → npm install --omit=dev

.\scripts\kugou-api-install.ps1 -Dir D:\apps\KuGouMusicApi   # 自定义目录
.\scripts\kugou-api-install.ps1 netease                      # 网易云音源
```

### 4. 开始播放

```powershell
.\scripts\kugou-tui.ps1
.\scripts\kugou-tui.ps1 -s 海阔天空      # 参数原样透传给播放器

.\scripts\kugou-tui.ps1 --dry-run        # 只打印决策，不启动任何东西
# 配置        : C:\Users\you\AppData\Roaming\kugou-tui\config.toml
# 当前音源    : kugou_concept
# 接口后端    : native
# 接口服务    : 不需要（内嵌后端（--api native）在进程内实现酷狗接口，不需要 Node.js）
```

默认 `--api native` 不需要本地服务，启动器直接进播放器。只有 `--api node` 才走「检查 → 启动 → 等就绪」流程（约 20 秒超时），也只有这时 dry-run 才会打印端口与目录：

```powershell
.\scripts\kugou-tui.ps1 --dry-run --api node
# 接口后端    : node
# 探测地址    : http://127.0.0.1:3001（端口 3001，平台 lite）
# 服务目录    : C:\Users\you\KuGouMusicApi
# 服务在跑吗  : 没起
```

或者完全手动，每个终端一条命令（只有 `--api node` 路径需要）：

```powershell
cd $env:USERPROFILE\KuGouMusicApi; $env:PORT=3000; node app.js
.\target\release\kugou-tui.exe
```

### 5. 终端要求

**用 Windows Terminal，不要用旧的「命令提示符」窗口。** 程序需要 `ENABLE_VIRTUAL_TERMINAL_PROCESSING`（备用屏幕、真彩色、鼠标上报）和 UTF-8 输出，Windows Terminal 默认都提供；旧的 conhost 显示中文和颜色会明显错乱。

Windows Terminal 里封面回退成**半块字符画**（它不支持 Kitty / iTerm2 图形协议，程序检测到后自动降级，属预期行为；启用 `lite_mode` 可跳过这部分开销）。装了 Nerd Font 的话，设一个环境变量，图标就不会回退成 ASCII：

```powershell
$env:KUGOU_TUI_NERD_FONT = "1"
```

### 6. 与 Linux 的功能差异

| 能力 | Linux | Windows |
|---|---|---|
| 播放、搜索、歌词、封面、缓存 | 有 | 有，相同 |
| MPRIS（桌面媒体控制、`playerctl`） | 有 | 无，D-Bus 接口，Windows 没有 |
| 系统托盘（StatusNotifierItem） | 有 | 无，同上 |
| 最小化窗口 | 有，仅 niri（入口在**托盘菜单**里） | 无，走合成器 IPC，无对应机制 |
| 配置文件位置 | `~/.config/kugou-tui/config.toml` | `%APPDATA%\kugou-tui\config.toml` |
| 缓存与日志位置 | `~/.cache/kugou-tui/` | `%LOCALAPPDATA%\kugou-tui\` |
| 启动器 / 服务安装脚本 | `scripts/` 下的 bash 版本 | `scripts/` 下的 `.ps1` 版本（功能对齐） |

三个「无」都是**降级而非故障**：代码不编译，界面上也不会出现死入口。用 `--print-config` 确认（会打印「系统托盘 : 不可用（windows 无 D-Bus）」）。

### 7. 改代码时怎么确认没弄坏 Windows

CI 有一个 `windows-latest` 任务，每次 push 都在真实 Windows 上跑 clippy + test + build + 打包。想先在本地检查（不需要 Windows 机器）：

```bash
rustup target add x86_64-pc-windows-msvc
cargo check  --target x86_64-pc-windows-msvc --all-targets
cargo clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings
```

最后链接一步需要 MSVC 工具链；在 Linux 上跑到 `error: linker link.exe not found` 就说明 Rust 侧全部通过。各平台分支为何存在，见 [MAINTENANCE.md](MAINTENANCE.md) 的「平台分支在哪」。

---

## 在 macOS 上构建与运行

**支持程度：代码与 Linux 共用 `cfg(unix)` 分支；CI 在 `macos-latest` 上跑 clippy + test + build，但未在真机长期使用。**

```bash
git clone https://github.com/sijin-xb/kugou-tui.git
cd kugou-tui && cargo build --release

./scripts/kugou-api-install kugou
./scripts/kugou-tui
```

`aws-lc-sys`（随 rustls 引入）是 C 代码，由 Xcode 的 clang 编译；macOS 不需要 ALSA 依赖，也不需要单独的 CMake。

> **脚本刻意避开 GNU 专有工具**（macOS 自带 BSD 工具链 + bash 3.2）：不用 `readlink -f`、`dirname --`、`seq`、`setsid`、`ss`、`declare -A`。替代写法与理由在脚本文件顶部的「portability」一节，改脚本前先读。
>
> 目录遵循 macOS 上 `dirs` 的取值（`~/Library/Application Support` 与 `~/Library/Caches`，**不是** `~/.config` / `~/.cache`），否则启动器会去找一个永远不存在的 `config.toml`。
>
> 从浏览器下载的 tar 包解出的二进制带 `com.apple.quarantine` 属性；包未签名，直接运行会被 Gatekeeper 杀掉（终端只显示 `Killed: 9`，看起来像程序自身的 bug）。启动器会检测并警告；手动修复是 `xattr -d com.apple.quarantine ./kugou-tui`。

### 与 Linux 的差异

| 项目 | macOS 上的行为 |
|---|---|
| 音频后端 | CoreAudio（cpal 自动选择）；无需配置设备 |
| MPRIS / 系统托盘 | **不可用**（D-Bus 接口；macOS 默认没有会话总线）；启动时打一条 WARN 并跳过 |
| 最小化窗口 | 不可用（走 niri 的合成器 IPC；没有 `NIRI_SOCKET` 时入口直接不出现） |
| 配置文件 | `~/Library/Application Support/kugou-tui/config.toml`（**不是** `~/.config`） |
| 缓存与日志 | `~/Library/Caches/kugou-tui/` |
| 默认下载目录 | `~/Music` |
| 封面 | iTerm2 用图形协议；Terminal.app 回退成半块字符画（预期） |
| Nerd Font 检测 | 没有 fontconfig，`fc-list` 检测必然失败 → 回退 ASCII。装了 Nerd Font 就设 `KUGOU_TUI_NERD_FONT=1` |

---

## 部署第三方接口服务

> **只有 `--api node` 回退（或网易云音源）需要这一节。** 默认内嵌后端（`--api native`）在进程内实现接口，与这里无关。

**用 `cargo install` 或预编译包装的，基本可以跳过**：程序启动时探测服务，缺失就自动安装并启动。剩下的是想手动控制或用网易云服务的情况。

酷狗接口**协议**来自独立仓库 [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi)（本仓库没有 submodule，也没有 vendor 目录）；内嵌后端是它的纯 Rust 移植，`--api node` 回退则直接跑那个 Node 服务。

### 一键

```bash
./scripts/kugou-api-install kugou
```

它克隆到 `~/KuGouMusicApi`，跑 `npm install`，再调 `scripts/kugou-api start` 拉起标准版（:3000）与概念版（:3001）两个实例。不带参数运行会列出每个音源的仓库、**固定的提交**和当前运行状态。

### 手动

```bash
git clone https://github.com/MakcRe/KuGouMusicApi.git
cd KuGouMusicApi
git checkout a5a98013cce79fe0ae2ad65fc84b68176ebcfc1e   # 固定到已验证的提交
npm install
npm start          # 注意是 npm start，不是 npm run dev
```

> **为什么要固定提交**：上游是活跃仓库，接口字段会变，master 某天可能解析不出歌名或歌词。同一个提交哈希也写在 `scripts/kugou-api-install` 里，它用浅拉取（`git fetch --depth 1 origin <sha>`）。更换已验证提交时，两处都要改。
>
> **概念版（lite）**必须带 `platform=lite` 启动：`platform=lite PORT=3001 npm start`。不带这个环境变量，概念版搜索会返回错误结果。
>
> `npm run dev` 用 nodemon（devDependency），在只装了生产依赖的环境里直接失败。

服务默认监听 `http://127.0.0.1:3000`；用 `curl -s "http://127.0.0.1:3000/register/dev"` 验证。**只用酷狗标准版的话，这一个实例就够。**

### 网易云音源

用另一个服务（NeteaseCloudMusicApi），需要单独部署；怎么部署、怎么配置见 [USER_GUIDE.md](USER_GUIDE.md#网易云音源)。

---

## 启动器脚本（可选）

| 脚本 | 用途 |
|---|---|
| `scripts/kugou-tui` | 启动播放器；**需要**本地接口服务时，若未运行则启动并等就绪 |
| `scripts/kugou-api` | 一次启动**两个**接口实例（标准版 + 概念版） |

> `scripts/kugou-tui` 感知后端：默认的 `--api native` 在进程内实现酷狗接口，直接进播放器，**不探测端口也不拉起进程**。只有 `--api node`（或配置里 `api_backend = "node"`）才走下面的「检查 → 启动 → 等就绪」流程。网易云与汽水音乐音源的接口不在内嵌实现里，仍需本地服务（汽水音乐直连公网，是唯一例外）。

```bash
ln -s "$PWD/scripts/kugou-tui" "$PWD/scripts/kugou-api" ~/.local/bin/

kugou-api start     # 启动两个实例（已在运行的跳过）；打印 PID / 日志路径 / URL
kugou-api status    # 查看状态（端口被别人占用会如实报告）
kugou-api logs      # 跟随日志（`kugou-api logs lite` 跟概念版）
kugou-api restart   # 先停再启（改端口/配置后用）
kugou-api stop      # 停止（按 PID，精确）
kugou-api help      # 完整文档
```

`start` 会先做一遍预检并补齐缺失项：`node`、KuGouMusicApi 目录、`node_modules`（缺失时自动 `npm install --omit=dev`）、客户端二进制（缺失时自动 `cargo build --release`）。任何一步失败都带着明确原因中止，不留半死进程。不想让它碰编译就设 `KUGOU_API_SKIP_BUILD=1`（`stop` / `status` 本来就不触发编译）。

参数解析优先级为**环境变量 > 配置文件 > 默认值**。配置文件是可选的 `<配置根>/kugou-tui/api.env`（Linux：`~/.config/kugou-tui/api.env`；macOS：`~/Library/Application Support/kugou-tui/api.env`；两者都可用 `KUGOU_TUI_CONFIG_DIR` 覆盖），每行一个 `KEY=VALUE`。例如 `KUGOU_STANDARD_PORT=3100 kugou-api restart`。

`kugou-api` 识别：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `KUGOU_API_DIR` | `/usr/share/kugou-tui/api/kugou`（由包提供时）或 `$HOME/KuGouMusicApi` | 服务目录 |
| `KUGOU_API_SYSTEM_ROOT` | `/usr/share/kugou-tui/api` | 包放置服务的位置（用于非 `/usr` 前缀打包） |
| `KUGOU_API_LOG_DIR` | `$XDG_CACHE_HOME/kugou-tui` | 日志与 PID 文件目录 |
| `KUGOU_API_HOST` | `127.0.0.1` | 监听地址 |
| `KUGOU_STANDARD_PORT` | `3000` | 标准版端口 |
| `KUGOU_LITE_PORT` | `3001` | 概念版端口 |
| `KUGOU_API_BIN` | `<repo>/target/release/kugou-tui`，不存在则用 `PATH` 上的 | 客户端二进制路径 |
| `KUGOU_API_SKIP_BUILD` | 空 | 设为 `1` 跳过构建预检 |
| `KUGOU_API_CONFIG` | `$XDG_CONFIG_HOME/kugou-tui/api.env` | 配置文件路径 |

`scripts/kugou-tui`（启动器）识别另一组：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `KUGOU_API_BACKEND` | 配置里的 `api_backend`，回退到 `native` | 与程序同名的变量。为 `native` 时启动器完全不碰本地服务；`--api <node\|native>` 优先级高于它 |
| `KUGOU_API_BASE` | **未设置** | 只有设置时才向程序传 `--api-base` 并用它探测；未设置时让程序读配置里选中的音源 |
| `KUGOU_API_DIR` | `$HOME/KuGouMusicApi` | 服务目录，用于自动拉起（仅 `--api node` 相关） |
| `KUGOU_API_LOG` | `$XDG_CACHE_HOME/kugou-tui/api.log` | 服务日志路径 |
| `KUGOU_TUI_BIN` | 自动探测 | 手动指定二进制路径。默认 `script_dir/../target/release/kugou-tui`，其次 `PATH` 上的 |
| `API_PORT` | `3000` | 自动拉起服务时用的端口 |

`kugou-tui --dry-run` 会打印算出的后端以及是否需要服务；用它排查「为什么没启动服务 / 为什么启动了」：

```
接口后端    : native
接口服务    : 不需要（内嵌后端（--api native）在进程内实现酷狗接口，不需要 Node.js）
```

### 避免常驻 node

node 进程只存在于 `--api node` 回退路径；默认的 `--api native` 从不拉起它，可以跳过这一节。

`scripts/kugou-tui` 用 `setsid` 把服务留在后台，退出时**不**回收（这样下次启动是瞬时的）。要避免常驻进程，两条路：

- 用程序内置的引导：`kugou-tui` 每次启动探测一次端口，没有就启动，**退出时只停它自己启动的实例**（脚本启动的不动）。要常驻服务就用 `--api-start` / `--api-stop`。
- 想在启动前顺便 `git pull` 服务代码（程序不做这件事），写个 shell 函数串起「pull → 缺依赖就装 → 没运行就启动 → 进播放器 → 退出时回收自己启动的」。第三步的关键细节：fish 里用 `env PORT=$port nohup node app.js --port=$port &`（不要用 `setsid`，否则 `$last_pid` 是 setsid 的而不是 node 的，`kill` 会打不中真正的服务）。

---

## 装完之后

**不登录也能听**；只有搜索和云端歌单需要账号。

1. 启动：`kugou-tui`（默认内嵌后端，没有服务要准备）。
2. 按 **`3`** 进歌单广场 → `Enter` 打开歌单 → 移动光标 → `Enter` 播放。

> 只有 `--api node` 回退才需要先确认 KuGouMusicApi 在跑（`curl -s http://127.0.0.1:3000/`）；启动器会自动拉起。

数字键映射：`1` 首页、`2` 搜索、`3` 歌单、`4` 歌手、`5` 排行榜、`6` 云端、`7` 播放队列、`8` 音源、`9` 设置、`0` 可视化（完整表见 [KEYBINDINGS.md](../KEYBINDINGS.md)）。搜索按 `2`；首次需要按 `L` 扫码登录。

> 首次启动会自动获取设备指纹 `dfid` 并写进配置（获取播放直链需要它），无需手动操作。

---

## 另见

- 完整快捷键表：[KEYBINDINGS.md](../KEYBINDINGS.md)
- 界面布局与音源配置：[USER_GUIDE.md](USER_GUIDE.md)
- 配置文件每个字段：[CONFIGURATION.md](CONFIGURATION.md)
- 装完之后出问题：[FAQ.md](FAQ.md)
- 排查「内存只涨不降」：`KUGOU_TUI_MEM_TRACE=1 kugou-tui`，日志里每 5 秒一行 RSS
