# Installation

This expands on [README](../README.md): requirements, per-platform install paths, third-party API
service deployment, launcher scripts and environment variables, and what to do after installing.

**Read this first**: the API implementation is **embedded in the binary**; the default path
(`--api native`) **needs no Node.js**. A local third-party API service is only needed for the
`--api node` fallback (or the Netease source) — see
"[Deploying the third-party API service](#deploying-the-third-party-api-service)".

---

## Requirements

| Item | Requirement |
|---|---|
| Node.js | **Only needed for the `--api node` fallback**, ≥ 12 (with npm), to run KuGouMusicApi. Not needed for `--api native` or `--no-default-features` builds |
| Rust toolchain | 1.90+ (edition 2024). The floor comes from a **dependency** (`quantette` requires 1.90), not this project's code |
| Audio output | Backends supported by rodio: ALSA / PipeWire / PulseAudio on Linux, WASAPI on Windows, CoreAudio on macOS |
| Terminal | UTF-8; **true color (24-bit)** for full theme colors and per-character gradients. Older terminals can fall back to 16 colors with `--basic-color` |
| OS | **Linux** (primary; tested on CachyOS), **Windows 10/11 x86_64**, **macOS** (covered by CI, not long-term real-hardware use) |
| Build deps (Linux) | `alsa-lib` dev headers + `pkg-config` (`cpal` links it via `alsa-sys`). Debian/Ubuntu: `libasound2-dev pkg-config`; Arch: `alsa-lib`. Usually present on desktop distros, **not in clean containers / servers** |
| Build deps (Windows) | MSVC toolchain only. **No CMake, NASM or OpenSSL** |
| Build deps (macOS) | Xcode command line tools only (`xcode-select --install`) |
| D-Bus (optional) | With a session bus, MPRIS and the system tray are enabled automatically; without it they are skipped and playback is unaffected. On Windows this code is not compiled |
| System tray (optional) | Requires a status bar providing `org.kde.StatusNotifierWatcher` (Quickshell / waybar / KDE all have it). Starting after the panel is fine — the tray thread retries registration every 5 seconds |

---

## Install paths

### Path 1: `cargo install` (crates.io)

```bash
cargo install kugou-tui
kugou-tui --api native      # embedded backend, no Node needed
```

The embedded backend works out of the box, with no first-time service setup. Only the Node fallback
(or the Netease source) needs the setup below: the first launch prepares and starts the API service
itself (download the pinned upstream commit → `npm install --omit=dev` → start, ~25 seconds in
testing), and later launches reuse it once the port is detected.

| Item | Notes |
|---|---|
| First launch (only `--api node`) | Needs network. To do it ahead of time: `kugou-tui --api node --api-start` (the service it starts stays in the background; `--api-stop` stops it) |
| Where the service is installed | Prefers `/usr/share/kugou-tui/api/kugou` (distro package) and an existing `~/KuGouMusicApi`; only if neither exists does it install to `~/.local/share/kugou-tui/api/kugou` |
| Managing the service yourself | Set `api_auto_start = false`, or pass `--no-api-start` |
| Exit behavior | An instance it started stops on exit, leaving no resident node; instances already running (started by you or another instance) are left alone |
| Removing the bootstrap code entirely | `cargo install kugou-tui --no-default-features` (disables the `node-bootstrap` feature; the binary contains no downloader / npm / node-spawn code) |

> This path does not include the repo's `scripts/*` (they are not part of the crate). For that
> one-shot script set, use the prebuilt package below.

### Path 2: Build from source

```bash
git clone https://github.com/sijin-xb/kugou-tui.git
cd kugou-tui
cargo build --release
./target/release/kugou-tui --help
```

The release artifact is about **11.7 MiB** (`opt-level="z"` + fat LTO + strip). On Windows, replace
the binary path with `.\target\release\kugou-tui.exe`; prerequisites and scripts are in
"[Building and running on Windows](#building-and-running-on-windows)".

### Path 3: Prebuilt binaries (GitHub Release)

[Releases](https://github.com/sijin-xb/kugou-tui/releases) has all three platforms, with identical
contents (binary + scripts + all docs):

| Platform | File |
|---|---|
| Linux x86_64 | `kugou-tui-<version>-x86_64-unknown-linux-gnu.tar.gz` |
| Windows x86_64 | `kugou-tui-<version>-x86_64-pc-windows-msvc.zip` |
| macOS arm64 | `kugou-tui-<version>-aarch64-apple-darwin.tar.gz` |

> Intel Macs have no prebuilt package (the CI runner is arm64); build from source.

**Linux**:

```bash
tar xzf kugou-tui-<version>-x86_64-unknown-linux-gnu.tar.gz
cd kugou-tui-<version>-x86_64-unknown-linux-gnu

# Link the binary and scripts into PATH. `scripts/kugou-tui` has the same name as the binary, so rename it when linking
mkdir -p ~/.local/bin
ln -s "$PWD/kugou-tui"                 ~/.local/bin/kugou-tui
ln -s "$PWD/scripts/kugou-api"         ~/.local/bin/kugou-api
ln -s "$PWD/scripts/kugou-api-install" ~/.local/bin/kugou-tui-install-api
ln -s "$PWD/scripts/kugou-tui"         ~/.local/bin/kugou-tui-launch

kugou-tui-install-api kugou   # one-time: fetch and configure the API service
kugou-tui                     # start playing
```

**Windows**:

```powershell
Expand-Archive kugou-tui-<version>-x86_64-pc-windows-msvc.zip -DestinationPath .
cd kugou-tui-<version>-x86_64-pc-windows-msvc
.\scripts\kugou-api-install.ps1   # one-time
.\scripts\kugou-tui.ps1           # start playing
```

**macOS** (scripts are bash, same usage as Linux):

```bash
tar xzf kugou-tui-<version>-aarch64-apple-darwin.tar.gz
cd kugou-tui-<version>-aarch64-apple-darwin
./scripts/kugou-api-install kugou
./scripts/kugou-tui
```

> The package is self-contained (includes scripts and docs) but **does not include the API service
> itself**: `kugou-api-install` needs `node` / `npm` / `git` and network (it clones the service and
> installs dependencies).
>
> The Windows zip also carries the bash scripts (for Git Bash / WSL); the Linux / macOS tarballs do
> not include PowerShell scripts.

### Path 4: AUR (planned, not yet published)

```bash
paru -S kugou-tui          # or yay -S kugou-tui
```

The target is "install and just run `kugou-tui`": the package puts the **API service with its
production dependencies** into `/usr/share/kugou-tui/api/kugou/`, which the program prefers, so no
npm install is needed.

> Bundling the service is possible because it **writes no files into its own directory** (no
> `writeFile` / `mkdirSync` in the source), so running it from a read-only directory works fine —
> verified in practice.
>
> The Netease service is not bundled (only needed for the Netease source); use
> `kugou-tui-install-api netease`.

---

## Building and running on Windows

Day-to-day use **needs no manual service start**: the PowerShell scripts under `scripts/` mirror the
bash scripts on Unix.

| Windows | Unix counterpart | Purpose |
|---|---|---|
| `scripts/kugou-api-install.ps1` | `kugou-api-install` | Fetch the API service + install dependencies (once) |
| `scripts/kugou-tui.ps1` | `kugou-tui` | Launcher: ensures the service is running when needed, then starts the player |
| `scripts/kugou-api.ps1` | `kugou-api` | Service lifecycle: `start` / `stop` / `restart` / `status` / `logs` |
| `scripts/build-windows.ps1` | `make-release-tarball` | Build + package the zip |

These scripts **depend only on PowerShell 5.1** (bundled with Windows); no separate pwsh 7 needed.
They share the launcher's PID files (`api-<instance>.pid` in the cache directory, containing
`<PID> <port>`), so instances the launcher started are recognized and stoppable by
`kugou-api.ps1` — no guessing node processes by name in Task Manager.

### 1. Install the toolchain

| What | How |
|---|---|
| Rust | [rustup.rs](https://rustup.rs) → choose `x86_64-pc-windows-msvc` |
| MSVC toolchain | Visual Studio Build Tools, check "Desktop development with C++"; or you already have it with VS |
| Node.js | [nodejs.org](https://nodejs.org) |
| Git | [git-scm.com](https://git-scm.com); the install script uses it to clone |

> **No CMake / NASM / OpenSSL needed**: on Windows, TLS uses the system's SChannel instead of rustls
> (whose `aws-lc-sys` is C code requiring CMake + NASM). See the `cfg(windows)` comment in
> `Cargo.toml`.

### 2. Build

```powershell
git clone https://github.com/sijin-xb/kugou-tui.git
cd kugou-tui
cargo build --release
.\target\release\kugou-tui.exe --help

.\scripts\build-windows.ps1        # package: dist\kugou-tui-<version>-x86_64-pc-windows-msvc.zip
```

### 3. Fetch the API service (once)

```powershell
.\scripts\kugou-api-install.ps1
# equivalent to: clone to %USERPROFILE%\KuGouMusicApi → pin to the verified commit → npm install --omit=dev

.\scripts\kugou-api-install.ps1 -Dir D:\apps\KuGouMusicApi   # custom directory
.\scripts\kugou-api-install.ps1 netease                      # Netease source
```

### 4. Start playing

```powershell
.\scripts\kugou-tui.ps1
.\scripts\kugou-tui.ps1 -s 海阔天空      # arguments passed through to the player verbatim

.\scripts\kugou-tui.ps1 --dry-run        # only prints decisions, starts nothing
# 配置        : C:\Users\you\AppData\Roaming\kugou-tui\config.toml
# 当前音源    : kugou_concept
# 接口后端    : native
# 接口服务    : 不需要（内嵌后端（--api native）在进程内实现酷狗接口，不需要 Node.js）
```

The default `--api native` needs no local service, so the launcher goes straight to the player. Only
`--api node` runs the "check → start → wait ready" flow (~20 s timeout), and only then does dry-run
print the port and directory:

```powershell
.\scripts\kugou-tui.ps1 --dry-run --api node
# 接口后端    : node
# 探测地址    : http://127.0.0.1:3001（端口 3001，平台 lite）
# 服务目录    : C:\Users\you\KuGouMusicApi
# 服务在跑吗  : 没起
```

Or fully manually, one command per terminal (only needed on the `--api node` path):

```powershell
cd $env:USERPROFILE\KuGouMusicApi; $env:PORT=3000; node app.js
.\target\release\kugou-tui.exe
```

### 5. Terminal requirements

**Use Windows Terminal, not the legacy "Command Prompt" window.** The program needs
`ENABLE_VIRTUAL_TERMINAL_PROCESSING` (alternate screen, true color, mouse reporting) and UTF-8
output, which Windows Terminal provides by default; the old conhost is visibly broken for CJK text
and colors.

Album art falls back to **half-block character art** in Windows Terminal (it does not support the
Kitty / iTerm2 graphics protocol; the program detects this and downgrades automatically — expected
behavior; enable `lite_mode` to skip that cost). With a Nerd Font installed, set one environment
variable so icons don't fall back to ASCII:

```powershell
$env:KUGOU_TUI_NERD_FONT = "1"
```

### 6. Feature differences from Linux

| Capability | Linux | Windows |
|---|---|---|
| Playback, search, lyrics, album art, cache | ✅ | ✅ same |
| MPRIS (desktop media controls, `playerctl`) | ✅ | ❌ D-Bus interface, absent on Windows |
| System tray (StatusNotifierItem) | ✅ | ❌ same as above |
| Minimize window | ✅ niri only (entry in the **tray menu**) | ❌ uses compositor IPC, no equivalent |
| Config file location | `~/.config/kugou-tui/config.toml` | `%APPDATA%\kugou-tui\config.toml` |
| Cache and log location | `~/.cache/kugou-tui/` | `%LOCALAPPDATA%\kugou-tui\` |
| Launcher / service installer | bash versions under `scripts/` | `.ps1` versions under `scripts/` (feature-matched) |

All three ❌ are **degradations, not failures**: the code is not compiled and no dead entries appear
in the UI. Confirm with `--print-config` (prints "系统托盘 : 不可用（windows 无 D-Bus）").

### 7. Checking you haven't broken Windows when changing code

CI has a `windows-latest` job that runs clippy + test + build + packaging on real Windows on every
push. To check locally first (no Windows machine needed):

```bash
rustup target add x86_64-pc-windows-msvc
cargo check  --target x86_64-pc-windows-msvc --all-targets
cargo clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings
```

The final link step needs the MSVC toolchain; on Linux, reaching
`error: linker link.exe not found` means everything on the Rust side passed. For why each platform
branch exists, see "where the platform branches are" in [MAINTENANCE.md](MAINTENANCE.md).

---

## Building and running on macOS

**Support level: the code uses the same `cfg(unix)` branches as Linux; CI runs clippy + test + build
on `macos-latest`, but it has not been used long-term on real hardware.**

```bash
git clone https://github.com/sijin-xb/kugou-tui.git
cd kugou-tui && cargo build --release

./scripts/kugou-api-install kugou
./scripts/kugou-tui
```

`aws-lc-sys` (pulled in with rustls) is C code, compiled by Xcode's clang; macOS does not need the
ALSA dependencies or a separate CMake.

> **The scripts deliberately avoid GNU-specific tools** (macOS ships the BSD toolchain + bash 3.2):
> no `readlink -f`, `dirname --`, `seq`, `setsid`, `ss`, or `declare -A`. The replacements and
> rationale are in the "portability" section at the top of the script files — read it before editing
> them.
>
> Directories match `dirs`' values on macOS (`~/Library/Application Support` and
> `~/Library/Caches`, **not** `~/.config` / `~/.cache`) — otherwise the launcher would look for a
> `config.toml` that never exists.
>
> A binary extracted from a browser-downloaded tarball carries the `com.apple.quarantine`
> attribute; the package is unsigned, so running it directly gets killed by Gatekeeper (the terminal
> only shows `Killed: 9`, which looks like a bug in the program itself). The launcher detects and
> warns; the manual fix is `xattr -d com.apple.quarantine ./kugou-tui`.

### Differences from Linux

| Item | Behavior on macOS |
|---|---|
| Audio backend | CoreAudio (chosen automatically by cpal); no device setup needed |
| MPRIS / system tray | **Unavailable** (D-Bus interface; macOS has no session bus by default); logs a WARN at startup and skips |
| Minimize window | Unavailable (uses niri's compositor IPC; the entry simply doesn't appear when `NIRI_SOCKET` is absent) |
| Config file | `~/Library/Application Support/kugou-tui/config.toml` (**not** `~/.config`) |
| Cache and logs | `~/Library/Caches/kugou-tui/` |
| Default download directory | `~/Music` |
| Album art | iTerm2 uses the graphics protocol; Terminal.app falls back to half-block character art (expected) |
| Nerd Font detection | No fontconfig, so `fc-list` detection always fails → ASCII fallback. With a Nerd Font installed, set `KUGOU_TUI_NERD_FONT=1` |

---

## Deploying the third-party API service

> **This section is only needed for the `--api node` fallback (or the Netease source).** The default
> embedded backend (`--api native`) implements the API in-process and does not touch any of this.

**If you installed via `cargo install`, or used a prebuilt package / AUR, you can mostly skip this
section** — the program probes the service at startup and installs and starts it if missing. The
rest is for those who want manual control or the Netease service.

The Kugou API **protocol** comes from the standalone repo
[KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi) (this repo has no submodule and no vendor
directory); the embedded backend is a pure-Rust port of it, while the `--api node` fallback runs
that Node service directly.

### One-shot

```bash
./scripts/kugou-api-install kugou
```

It clones to `~/KuGouMusicApi`, runs `npm install`, then calls `scripts/kugou-api start` to bring up
both the standard (:3000) and concept (:3001) instances. Run without arguments to list each source's
repo, **pinned commit**, and current running state.

### Manual

```bash
git clone https://github.com/MakcRe/KuGouMusicApi.git
cd KuGouMusicApi
git checkout a5a98013cce79fe0ae2ad65fc84b68176ebcfc1e   # pin to the verified commit
npm install
npm start          # note: npm start, not npm run dev
```

> **Why pin the commit**: upstream is an active repo and API fields change, so master may one day
> fail to parse song titles or lyrics. The same commit hash is in `scripts/kugou-api-install`, which
> shallow-fetches it (`git fetch --depth 1 origin <sha>`). When changing the verified commit, update
> both places.
>
> **Concept (lite)** must be started with `platform=lite`: `platform=lite PORT=3001 npm start`.
> Without this environment variable, concept-version search returns wrong results.
>
> `npm run dev` uses nodemon (a devDependency) and fails outright in an environment with only
> production dependencies installed.

The service listens on `http://127.0.0.1:3000` by default; verify with
`curl -s "http://127.0.0.1:3000/register/dev"`. **For the Kugou standard version alone, this one
instance is enough.**

### Netease source

Uses a different service (NeteaseCloudMusicApi) that must be deployed separately; see
[USER_GUIDE.md](USER_GUIDE.md#netease-source) for how and how to configure it.

---

## Launcher scripts (optional)

| Script | Purpose |
|---|---|
| `scripts/kugou-tui` | Start the player; when a local API service **is** needed, starts it and waits for readiness if not running |
| `scripts/kugou-api` | Start **both** API instances (standard + concept) at once |

> `scripts/kugou-tui` is backend-aware: the default `--api native` implements the Kugou API
> in-process and goes straight to the player, **without probing ports or spawning processes**. Only
> `--api node` (or `api_backend = "node"` in the config) runs the "check → start → wait ready" flow
> below. The Netease and Soda sources' APIs are not in the embedded implementation and still need a
> local service (Soda connects directly to the public internet, the sole exception).

```bash
ln -s "$PWD/scripts/kugou-tui" "$PWD/scripts/kugou-api" ~/.local/bin/

kugou-api start     # start both instances (already-running ones are skipped); prints PID / log path / URL
kugou-api status    # show status (reports honestly if the port is held by someone else)
kugou-api logs      # follow logs (`kugou-api logs lite` follows the concept version)
kugou-api restart   # stop then start (use after changing ports/config)
kugou-api stop      # stop (precise, by PID)
kugou-api help      # full documentation
```

`start` runs a preflight pass and fills in what's missing: `node`, the KuGouMusicApi directory,
`node_modules` (auto `npm install --omit=dev` if missing), the client binary (auto
`cargo build --release` if missing). Any failure aborts with a clear reason, leaving no half-dead
process. To stop it from touching compilation, set `KUGOU_API_SKIP_BUILD=1` (`stop` / `status` never
trigger compilation anyway).

Arguments resolve as **environment variable > config file > default**. The config file is an
optional `<config root>/kugou-tui/api.env` (Linux: `~/.config/kugou-tui/api.env`; macOS:
`~/Library/Application Support/kugou-tui/api.env`; both overridable via `KUGOU_TUI_CONFIG_DIR`),
one `KEY=VALUE` per line. Example: `KUGOU_STANDARD_PORT=3100 kugou-api restart`.

`kugou-api` recognizes:

| Variable | Default | Notes |
|---|---|---|
| `KUGOU_API_DIR` | `/usr/share/kugou-tui/api/kugou` (when provided by the package) or `$HOME/KuGouMusicApi` | Service directory |
| `KUGOU_API_SYSTEM_ROOT` | `/usr/share/kugou-tui/api` | Where the package places the service (for packaging with a non-`/usr` prefix) |
| `KUGOU_API_LOG_DIR` | `$XDG_CACHE_HOME/kugou-tui` | Log and PID file directory |
| `KUGOU_API_HOST` | `127.0.0.1` | Listen address |
| `KUGOU_STANDARD_PORT` | `3000` | Standard version port |
| `KUGOU_LITE_PORT` | `3001` | Concept version port |
| `KUGOU_API_BIN` | `<repo>/target/release/kugou-tui`, or the one on `PATH` if absent | Client binary path |
| `KUGOU_API_SKIP_BUILD` | empty | Set to `1` to skip the build preflight |
| `KUGOU_API_CONFIG` | `$XDG_CONFIG_HOME/kugou-tui/api.env` | Config file path |

`scripts/kugou-tui` (the launcher) recognizes another set:

| Variable | Default | Notes |
|---|---|---|
| `KUGOU_API_BACKEND` | `api_backend` from the config, falling back to `native` | Same variable as the program's. With `native` the launcher never touches a local service; `--api <node\|native>` takes precedence over it |
| `KUGOU_API_BASE` | **unset** | Only when set does it pass `--api-base` to the program and probe with it; unset, it lets the program read the source selected in the config |
| `KUGOU_API_DIR` | `$HOME/KuGouMusicApi` | Service directory, used for auto-start (only relevant on `--api node`) |
| `KUGOU_API_LOG` | `$XDG_CACHE_HOME/kugou-tui/api.log` | Service log path |
| `KUGOU_TUI_BIN` | auto-detected | Manually specify the binary path. Defaults to `script_dir/../target/release/kugou-tui`, then the one on `PATH` |
| `API_PORT` | `3000` | Port used when auto-starting the service |

`kugou-tui --dry-run` prints the computed backend and whether a service is needed; use it to debug
"why didn't it start the service / why did it start one":

```
接口后端    : native
接口服务    : 不需要（内嵌后端（--api native）在进程内实现酷狗接口，不需要 Node.js）
```

### Avoiding a resident node

A node process only exists on the `--api node` fallback path; the default `--api native` never
spawns it, so you can skip this section.

`scripts/kugou-tui` uses `setsid` to leave the service in the background and does **not** reclaim it
on exit (so the next launch starts instantly). To avoid a resident process, two options:

- Use the program's built-in bootstrap: `kugou-tui` probes the port once per launch, starts the
  service if absent, and **on exit stops only the instance it started** (ones started by the script
  are untouched). Use `--api-start` / `--api-stop` for a resident service.
- To also `git pull` the service code before launch (the program doesn't do this), write a shell
  function chaining "pull → install deps if missing → start if not running → enter the player →
  collect on exit what you started". The key detail in the third step: in fish use
  `env PORT=$port nohup node app.js --port=$port &` (not `setsid`, otherwise `$last_pid` is
  setsid's, not node's, and `kill` misses the real service).

---

## What to do after installing

**You can listen without logging in**; only search and cloud playlists need an account.

1. Launch: `kugou-tui` (default embedded backend; no service to prepare).
2. Press **`3`** for the playlist square → `Enter` to open a playlist → move the cursor → `Enter`
   to play.

> Only on the `--api node` fallback do you need to first confirm KuGouMusicApi is running
> (`curl -s http://127.0.0.1:3000/`); the launcher starts it automatically.

Number-key map: `1` Home, `2` Search, `3` Playlists, `4` Artists, `5` Charts, `6` Cloud, `7` Queue,
`8` Sources, `9` Settings, `0` Visualizer (full table in [KEYBINDINGS.md](../KEYBINDINGS.md)). To
search, press `2`; the first time you need to press `L` to scan a QR code and log in.

> The first launch automatically fetches the device fingerprint `dfid` and writes it to the config
> (needed to get playback direct links); nothing for you to do.

---

## See also

- Full keybinding table: [KEYBINDINGS.md](../KEYBINDINGS.md)
- UI layout and source configuration: [USER_GUIDE.md](USER_GUIDE.md)
- Every config file field: [CONFIGURATION.md](CONFIGURATION.md)
- Problems after installing: [FAQ.md](FAQ.md)
- Debugging "memory only grows, never drops": `KUGOU_TUI_MEM_TRACE=1 kugou-tui`, one RSS line every
  5 seconds in the log
