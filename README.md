# kugou-tui

在终端里听酷狗：逐字歌词、真实频谱、单进程二进制，常驻内存约 19 MiB。

![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.90%2B-orange.svg)
![Platform](https://img.shields.io/badge/platform-linux%20%7C%20windows-lightgrey.svg)
![crates.io](https://img.shields.io/crates/v/kugou-tui.svg)

![kugou-tui：歌单广场、正在播放、逐字歌词与播放队列](assets/screenshot-0.6.2.png)

酷狗接口内嵌在二进制里，默认 `--api native` 不需要 Node.js；只有 `--api node` 回退和网易云音源才要本机跑一份第三方接口服务。

## 安装

```bash
cargo install kugou-tui
kugou-tui
```

从源码构建就 `git clone` 之后 `cargo build --release`，产物约 11.7 MiB。需要 Rust 1.90+；Linux 另需 `alsa-lib` 开发头文件和 `pkg-config`（Debian/Ubuntu 是 `libasound2-dev pkg-config`，Arch 是 `alsa-lib`），Windows 只需 MSVC 工具链，macOS 只需 Xcode 命令行工具。

Release 页提供 Linux x86_64、Windows x86_64、macOS arm64 三个包，内含二进制、`scripts/`、`README.md`、`CHANGELOG.md`、`LICENSE`。Intel Mac 请从源码构建。Windows 把下面的 `./scripts/` 换成 `.\scripts\` 下的 `.ps1`。

```bash
tar xzf kugou-tui-<版本>-x86_64-unknown-linux-gnu.tar.gz
cd kugou-tui-<版本>-x86_64-unknown-linux-gnu
./scripts/kugou-api-install kugou   # 只需一次
./scripts/kugou-tui
```

## 快速开始

按 `2` 搜索，`Enter` 播放，按 `L` 扫码登录拉云端歌单。不登录也能听，只有搜索和云端歌单需要账号。

## 按键

输入框获得焦点时字母键只输入文本，`Esc` 退出输入模式。

| 按键 | 动作 |
|---|---|
| `1`–`9` `0` | 主页 搜索 歌单 歌手 排行榜 云端 队列 音源 设置 可视化 |
| `j` `k` `↑` `↓` | 上下移动（首页与可视化页改为切歌） |
| `Enter` `Esc` | 播放 / 进入 · 返回 |
| `Space` `n` `p` | 播放暂停 · 下一首 · 上一首 |
| `←` `→` | 后退 / 前进 5 秒 |
| `+` `-` `m` | 音量 · 静音 |
| `r` `y` `W` | 循环模式 · 切音质 · 下载当前曲目 |
| `l` `[` `]` | 歌词面板 · 延迟 / 提前 100 ms |
| `a` `i` `A` | 加入队列 · 插播下一首 · 整段加入 |
| `x` `X` `o` | 从队列移除 · 清空 · 升降序 |
| `s` `S` `d` `D` `N` | 收藏 · 同步队列 · 移除 · 删歌单 · 新建 |
| `L` `V` `C` | 扫码登录 · 领当日概念版 VIP · 清音频缓存 |
| `/` `R` `M` `b` `c` `f` | 搜索框 · 刷新 · 加载更多 · 排行榜 · 云端歌单 · 歌手筛选 |
| `?` `q` | 帮助面板（40 条） · 退出 |

`;` 或鼠标右键打开歌曲菜单。双击列表行等同 `Enter`，滚轮移动选中项，单击进度条或歌词行跳转，设置页里 `←` `→` 改值。自定义快捷键写配置文件里的 `[keymap]`。

## 配置

配置文件在 `~/.config/kugou-tui/config.toml`（Windows `%APPDATA%\kugou-tui\`，macOS `~/Library/Application Support/kugou-tui/`），`--print-config` 打印当前生效值。

常用项：`api`、`api_base`、`api_auto_start`、`api_dir`、`volume`、`cache_dir`、`cache_limit`（默认 512 MiB）、`page_size`、`tick_ms`、`audio_device`、`lite_mode`（关封面与频谱，刷新降到 5fps）、`lyric_offset_ms`、`ws_port`、`[keymap]`。命令行参数与配置项一一对应，`--help` 有完整列表。

## 音源

`8` 进音源页：`Enter` 启用或禁用，`E` 设为默认，`J` `K` 调优先级。

- **酷狗概念版**：接口与标准版不同，登录态不通用，切过去要按 `L` 重新扫码。`V` 领当日 VIP，登录后和切音源时会自动领一次。
- **网易云**：需要本机跑一份 Meting-API，配 `[sources.netease]`。
- **汽水音乐**：`--sodam-cookie` 手填 cookie，不支持扫码。VIP 整曲还要签名头，或指向一台签名服务。

`--api node` 回退会自行下载固定提交的 KuGouMusicApi、装依赖、拉起服务（首次约 25 秒），退出时随程序停止。想自己管就设 `api_auto_start = false`，或用 `./scripts/kugou-api start|stop|status|logs`。

## 排障

- 内存只涨不降：`KUGOU_TUI_MEM_TRACE=1 kugou-tui`，日志每 5 秒一行 RSS（`Threads:` 正常 7–9，多了就是线程或任务泄漏）。日志在 `~/.cache/kugou-tui/kugou-tui.log`，`KUGOU_TUI_DEBUG=1` 打开调试输出。
- 搜不到歌：没登录。搜索和云端歌单要账号，歌单广场不用。
- 托盘图标不出现：需要 `org.kde.StatusNotifierWatcher`（Quickshell / waybar / KDE 都有），托盘线程每 5 秒重试注册。
- 封面是色块、中文显示成方框：前者是终端不支持 Kitty / iTerm2 / sixel 图形协议，回退成半块字符画；后者是缺 Nerd Font 或中文字体，设 `KUGOU_TUI_NERD_FONT=1` 强制启用图标字体。

## 开发

提交前跑 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。发版走 `./scripts/release`。

## 许可与免责

本项目以 **AGPL-3.0-or-later** 授权，正文见 `LICENSE`。

1. **仅供个人学习与技术研究使用。** 本项目不提供、不托管、不分发任何音乐内容。
2. **接口协议与内置资源来自第三方逆向工程项目。** 内嵌的酷狗接口实现逐项对照 [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi) 移植；汽水音源数据层取自 [sodahub-org/libresoda](https://github.com/sodahub-org/libresoda)。本项目不拥有任何音乐版权，也不对这些接口或第三方资源的可用性、稳定性或合法性负责。
3. **第三方服务合规风险。** 通过非官方接口访问酷狗可能违反其服务条款，存在账号被限制或封禁的风险。不鼓励、也不协助批量下载、分发或商业使用。
4. **版权。** 尊重音乐版权，支持正版。缓存文件请自行清理。
5. **地区与法律。** 请勿在违反当地法律法规的情况下使用；使用后果由使用者自行承担。
6. **隐私。** 登录 cookie 只存在本地配置文件里（权限 `0600`），只发往酷狗官方域名或你自己部署的接口服务。日志里的 `token` / `userid` / `dfid` / `mid` 一律脱敏，没有遥测。

感谢 [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi)（接口实现来源）、[MoeKoeMusic](https://github.com/MoeKoeMusic/MoeKoeMusic)（歌词与歌单设计参考）、[libresoda](https://github.com/sodahub-org/libresoda)（汽水数据层）、[ratatui](https://github.com/ratatui/ratatui) / [rodio](https://github.com/RustAudio/rodio) / [tokio](https://github.com/tokio-rs/tokio) / [ratatui-image](https://github.com/benjajaja/ratatui-image)。
