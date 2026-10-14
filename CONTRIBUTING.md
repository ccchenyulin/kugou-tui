# 贡献指南

## 搭好环境

1. **Rust 1.90+**（edition 2024；下限由一个**依赖**顶上去：`quantette` 需要 1.90）。
2. **Node.js 12+（可选）**：只有调试 `--api node` 回退或跑网易云音源时才需要。默认 `--api native` 在进程内
   实现酷狗接口，**不需要 Node.js**，不登录也能浏览和播放。回退路径的接口服务
   [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi) 是独立仓库（无 submodule、无 vendor），单独拉取，最省事：

   ```bash
   ./scripts/kugou-api-install kugou   # 克隆 + npm install + 启动两个实例
   ```

   手动也行，但**必须固定到已验证的 commit**（上游活跃，接口字段会漂）：

   ```bash
   git clone https://github.com/MakcRe/KuGouMusicApi.git && cd KuGouMusicApi
   git checkout a5a98013cce79fe0ae2ad65fc84b68176ebcfc1e
   npm install && npm start            # 注意：npm start，不是 npm run dev
   ```

3. 构建并运行：

   ```bash
   cargo build --release && ./target/release/kugou-tui
   ```

一般开发不需要扫码登录。

## 提交前必跑这三条

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

**clippy 带 `-D warnings`**：任何警告都算失败。CI（`.github/workflows/ci.yml`）在 Linux / Windows / macOS 三列
各跑 clippy + test，`cargo fmt --check` 与 no-default-features 组合只在 Linux 列跑（都不依赖平台）。三列都要保留：
路径展开、配置目录、缓存文件命名、音频后端只有真跑起来才暴露。

**CI 只验证，不产出二进制**：不跑 `cargo build --release`、不打包、不传产物。要能跑的二进制用 Releases 页面
（`release.yml` 在推标签时产出）或本地 `cargo build --release`，原因见 `ci.yml` 头部。

改平台相关代码时，额外确认 Windows 目标能构建：

```bash
rustup target add x86_64-pc-windows-msvc
cargo check --target x86_64-pc-windows-msvc --all-targets
```

> MSRV 是 **1.90**，写在 `Cargo.toml` 的 `rust-version`，由**依赖**顶上去，随 `cargo update` 漂移。
> 改依赖后确认下限：`cargo +1.90.0 check --locked --all-targets`。

## 代码结构

```
src/
├── main.rs          入口：分配器策略、参数解析、日志、接口服务引导、启动 App
├── bootstrap.rs     本地接口服务（KuGouMusicApi）自动引导；node-bootstrap 特性关闭时用
│                    bootstrap_disabled.rs 替身（全部报错）
├── cli.rs           clap 命令行参数
├── config.rs        TOML 配置持久化（含默认值与合法值常量）
├── error.rs         统一错误类型 AppError
├── logger.rs        极简文件日志（不用 tracing）+ 内存内 tracing
├── event.rs         事件总线与 Loaded 枚举
├── keymap.rs        按键 → Action 映射，以及帮助面板用的 CHEATSHEET
├── mpris.rs         MPRIS（桌面媒体控制），仅 Unix
├── tray.rs          系统托盘（StatusNotifierItem），仅 Unix
├── window.rs        niri 窗口控制（最小化 / 显示）
├── util.rs          时间格式化、随机数、小工具
├── api/
│   ├── traits.rs    统一音源接口
│   ├── client.rs    HTTP 客户端：鉴权参数、错误码解析、响应缓存绕过
│   ├── model.rs     JSON → 领域模型解析（所有多候选键名都在这里）
│   ├── catalog.rs   浏览接口：搜索 / 歌单 / 歌手 / 排行榜
│   ├── cloud.rs     登录与云端歌单写操作
│   ├── lyric.rs     歌词拉取与 KRC/LRC 解码
│   ├── node.rs      `--api node` 回退：基于本地 KuGouMusicApi 的实现
│   └── native/      内嵌纯 Rust 酷狗接口（默认后端）：mod.rs 26 条路由在进程内实现、
│                    sign.rs 两套签名方案（Android / Web）、crypto.rs RSA/AES/MD5、
│                    device.rs 设备指纹、krc.rs KRC 歌词解密、transport.rs 出站请求构造
├── app/             mod.rs App 组装、主循环、输入线程；state.rs AppState 与列表结构；update.rs 事件→
│                    状态变更（唯一改状态处）；playback.rs 播放控制；cloud.rs 登录、云端歌单与音源切换；
│                    desktop.rs 推送 MPRIS 与托盘；navigation.rs 标签页与焦点；queue.rs 播放队列与模式；
│                    search.rs 搜索；session.rs 会话持久化；settings.rs 设置页
├── audio/           engine.rs rodio 播放引擎（独立线程 + 原子量）；streaming.rs 内存内流窗口与落盘；
│                    download.rs 音频下载（先写 .part 再原子改名）；cache.rs 缓存目录管理与 LRU 回收；
│                    resample.rs 高解析母带抗混叠重采样；downmix.rs 多声道下混；levels.rs 电平采集与
│                    频谱计算；spectrum.rs FFT
├── source/          音源抽象（酷狗 / 概念版 / 网易云 / 汽水）：netease.rs 网易云（基于本地
│                    NeteaseCloudMusicApi）、sodam/ 汽水（直连公网 + CDP 签名页）
└── ui/              mod.rs 布局；theme.rs 配色（真彩色 / 16 色）；icons.rs 图标（Nerd Font / ASCII）；
                     widgets.rs 渲染原语（列表行、面板、二维码等）；views/ 侧栏、列表、播放栏、歌词、
                     帮助、设置、音源、可视化

crates/
└── libresoda/       汽水音乐数据面（独立 crate，包名 libresoda-fork）
```

## 约定

### 注释解释「为什么」，不复述「做什么」

代码已说明它在做什么；注释只补**读代码看不出来的上下文**：某字段为什么有两个名字、某个时间戳为什么加在这里、某个魔数从哪来。模块顶部的 `//!` 文档说明该模块的设计取舍。

### 界面只用 ASCII 标记

界面符号（选中标记 `>`、正在播放 `*`）一律 ASCII。花哨的 Unicode 符号在缺字体的终端里变成豆腐块，或搞乱列宽计算。

### 按显示宽度截断文本

中文和全角标点占 2 列。所有截断必须走 `display_width` / `truncate_to_width`；用 `chars().count()` 切会让中英混排行错位。

### 新增依赖需要理由

本项目目标之一是「轻量」（release 二进制约 **11.7 MiB**，常驻约 **19 MiB**；测量见 [docs/DESIGN.md](docs/DESIGN.md#低资源占用)）。
加依赖前说清：解决什么问题、几十行能不能替代、带来多大的依赖树。`tracing` / `chrono` / `rand` 目前刻意排除。

### 宽容解析上游响应

KuGouMusicApi 是逆向出来的封装：**响应结构会漂，文档和现实不一致**。约定：

- 同一语义按**候选键名列表**依次尝试（`pick_string` / `pick_i64` 等）；不假设类型，`AlbumID` 可能是数字、
  字符串或 `null`；单个解析失败只丢这一项，不丢整页。
- 加解析逻辑时，把文档不一致记到 [docs/DESIGN.md](docs/DESIGN.md#接口适配) 的「上游文档说 / 实际是」表里。
- **同一上游可能有多种字段布局，每种都要有单元测试钉住。** 例：网易云 `/search` 给 `artists` / `album` /
  `duration`，而 `/playlist/track/all` 与 `/artists` 给 `ar` / `al` / `dt`，只认前者会让歌单与歌手页每首歌
  都没有歌手、没有专辑、时长 `00:00`。

### 状态只在主线程改

`app/update.rs` 是唯一修改 `AppState` 的地方；音频线程只通过原子量共享播放位置、时长和音量。这样避免加锁，状态变更也可追溯。

## 新增一个接口

1. 在 `api/catalog.rs`（写操作用 `cloud.rs`）加一个 `async fn`，调用 `self.get_json_uncached(...)` 或 `get_json(...)`。
2. 在 `api/model.rs` 加对应解析函数，遵守上面的宽容约定。
3. 在 `event.rs` 的 `Loaded` 枚举加一个变体。
4. 在 `app/update.rs` 加取数方法 + `Loaded` 分支处理。
5. 需要新按键：在 `keymap.rs` 加 `Action` 变体、键映射、`CHEATSHEET` 条目，并在 `REBINDABLE` 里加动作名
   （用户在 `[keymap]` 里按这个名字重绑）；最后同步 [docs/CONFIGURATION.md](docs/CONFIGURATION.md#自定义快捷键)
   的动作名列表。**动作名漏了测试会当场报出来**（`every_cheatsheet_shortcut_can_be_rebound_by_name` 比较
   两份列表；CONFIGURATION.md 那份只能手动同步，测试不覆盖）。
6. 需要显示：在 `app/state.rs` 加列表字段，在 `ui/` 加渲染。
7. 加单元测试，尤其是解析部分。

**酷狗接口有两个后端。** 默认 `--api native`（`api/native/`）在进程内实现路由；`--api node`（`api/node.rs`）
走本地 KuGouMusicApi；`api/traits.rs` 是共享接口，`app/` 只认它。新增或修改酷狗接口时两边都要改，并且：

- native 侧的签名/加密/设备指纹部分**出错时静默失败**，需要 KAT（实测基线值在 `tools/kat/`，用法见
  [tools/kat/README.md](tools/kat/README.md)）；
- 字段解析有分歧时，把「上游文档说 / 实际是」的差异记到 [docs/DESIGN.md](docs/DESIGN.md#接口适配)。

## 验证

**不要只看文档验证。** 这个项目踩的坑几乎都来自「文档这么说，现实不是」。改动涉及接口时：

- 用 `curl` 打真实服务，把原始响应存下来对比；
- 启动应用点一遍，确认「能看到具体内容」，而不只是「列表加载出来了」。

排查按键问题时打开 verbose 日志（会记录每个键映射到什么）：

```bash
KUGOU_TUI_DEBUG=1 ./target/release/kugou-tui
```

日志路径用 `kugou-tui --print-config` 查看。

## 提交与 PR

- commit message 一行说清改了什么；多个改动拆成多个 commit。
- PR 里说明：**改了什么、为什么、怎么验证的**（接口改动附一小段真实响应的关键字段很有用）。
- 改动影响用户可见行为时，同步更新 `README.md`。

## 发版

### 1. 先升版本号与更新日志

```bash
# Cargo.toml 版本号（纯数字 X.Y.Z；Cargo.lock 也会变，一起提交）
# 在 CHANGELOG.md 顶部加一节，格式按 [Keep a Changelog]
git add -A && git commit -m "chore: version X.Y.Z" && git push
```

### 2. 发布到 crates.io

```bash
cargo publish          # 需要已验邮箱的 crates.io 账号
```

`scripts/release` **不做这一步**。crates.io 渲染的是**该版本包里的 README**，只改 README 不发新版，页面不会变。

### 3. 打标签并创建 Release

```bash
./scripts/release              # 第一阶段本地构建，第二阶段确认后推送
./scripts/release --dry-run    # 只检查，不构建不推送
```

脚本自动完成：预检 → 清理上次中间产物 → clippy + test → 构建 → 打包并检查内容清单 → **停下等确认** →
push main → tag → push tag → create Release → download back to verify。

完整规范（版本与标签规则、产物清单、各步骤依赖、失败恢复）见 [docs/RELEASE.md](docs/RELEASE.md)。

### 最容易漏掉的两件事

1. **工具脚本必须在打标签前提交**：`scripts/make-release-tarball` 曾在打标签之后才提交，导致 checkout 该标签拿不到它。`scripts/release` 的预检要求工作区干净，机械上挡住了这种情况。
2. **上游 commit 只固定在 `scripts/kugou-api-install` 的 `pinned_of()` 一处**，改它要同步本文档「搭好环境」里的 commit 示例。

### 磁盘

一次发版会在 `target/package/` 和 `dist/` 留下约 10 MiB 中间产物。`scripts/release` 开始前会回收；也可以手动跑：

```bash
./scripts/clean --dry-run   # 先看要删什么
./scripts/clean             # 回收
./scripts/clean --all       # 连 target/ 一起清（下次全量重编约 3 分钟）
```

## 法律与合规

本项目是个人学习/研究工具：内嵌后端（`--api native`）从开源项目 [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi)
逐项移植酷狗接口的签名与解密，`--api node` 回退则调用该服务。贡献代码时确保：

- 不实现破解、绕过认证或规避版权保护的逻辑（移植的签名/解密仅用于用本人账号访问酷狗官方服务，不得用于绕过会员或版权限制）；
- 不上传用户数据的代码（本项目无遥测；登录凭据只存在本地）；
- 涉及第三方服务的改动在 PR 里说明目的与合规影响。
