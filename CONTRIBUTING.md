# 贡献指南

感谢你愿意参与 `kugou-tui`。这份文档说明怎么把项目跑起来、代码有哪些约定，
以及提交前该跑哪些检查。

## 先把环境跑起来

1. **Rust 1.86+**（本项目用 edition 2024；下限由 `ratatui-image` 11.x 决定）。
2. **Node.js 12+**，用于运行 [KuGouMusicApi](https://github.com/MakcRe/KuGouMusicApi)。
   它是**独立仓库**，不在本仓库里（无 submodule、无 vendor），得单独拉一份。
   最省事的是用仓库自带的脚本：

   ```bash
   ./scripts/kugou-api-install kugou   # clone + npm install + 起两个实例
   ```

   手动来一遍也可以，但**要固定到 README 里那个经过验证的提交**——上游是活跃
   仓库，接口字段会漂移，跟 master 可能某天就解析不出歌名或歌词：

   ```bash
   git clone https://github.com/MakcRe/KuGouMusicApi.git
   cd KuGouMusicApi
   git checkout a5a98013cce79fe0ae2ad65fc84b68176ebcfc1e
   npm install && npm start            # 注意是 npm start，不是 npm run dev
   ```

3. 编译并运行本客户端：

   ```bash
   cargo build --release
   ./target/release/kugou-tui
   ```

不登录也能浏览和播放，所以一般开发调试不需要扫码。

## 提交前请跑这三条

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

CI 会用同样的命令。**clippy 是 `-D warnings` 的**，任何告警都会让构建失败。

## 代码结构

```
src/
├── main.rs          入口：参数解析、日志、启动 App
├── cli.rs           clap 命令行参数
├── config.rs        TOML 配置持久化（含默认值与合法取值常量）
├── error.rs         统一错误类型 AppError
├── logger.rs        极简文件日志（不引入 tracing）
├── event.rs         事件总线与 Loaded 枚举
├── keymap.rs        按键 → Action 的映射，以及帮助面板用的 CHEATSHEET
├── util.rs          时间格式化、随机数等小工具
├── api/
│   ├── client.rs    HTTP 客户端：鉴权参数、错误码解析、响应缓存绕开
│   ├── model.rs     JSON → 领域模型的解析（多候选键名都在这里）
│   ├── catalog.rs   浏览类接口：搜索 / 歌单 / 歌手 / 排行榜
│   ├── cloud.rs     登录与云端歌单写操作
│   └── lyric.rs     歌词获取与 KRC/LRC 解码
├── app/
│   ├── mod.rs       App 装配、主循环、输入线程
│   ├── state.rs     AppState 与各列表结构
│   ├── update.rs    事件 → 状态变更（唯一改状态的地方）
│   └── queue.rs     播放队列与播放模式
├── audio/
│   ├── engine.rs    rodio 播放引擎（独立线程 + 原子量）
│   ├── download.rs  音频下载（写 .part 再原子重命名）
│   └── cache.rs     缓存目录管理与 LRU 回收
└── ui/
    ├── mod.rs       布局
    ├── theme.rs     配色（真彩 / 16 色两套）
    ├── widgets.rs   列表行、面板、二维码等渲染原语
    └── views/       侧边栏、列表、播放条、歌词、帮助
```

## 约定

### 注释写「为什么」，不写「是什么」

代码本身已经说明了它在做什么，注释应该补充**读代码看不出来的背景**：
某个字段为什么有两个名字、某处为什么要加时间戳、某个魔法数字的来历。
每个模块顶部的 `//!` 文档用来讲这个模块的设计取舍。

### UI 只用 ASCII 标记

界面里的符号（选中标记 `>`、播放中 `*`）一律用 ASCII。花式 Unicode 符号在缺字体的
终端里会变成豆腐块或让列宽算错。

### 文本按「显示宽度」截断

中文、全角标点占 2 列。所有截断都要走 `display_width` / `truncate_to_width`，
直接按 `chars().count()` 切会让中英混排列错位。

### 新增依赖要说明理由

本项目的目标之一是"轻量"（release 二进制约 **7.0 MiB**、常驻约 **14–17 MiB**，
实测见 [docs/DESIGN.md](docs/DESIGN.md#低资源占用)）。加依赖前请说明：
它解决什么问题、有没有几十行代码能替代、会引入多大的依赖树。
`tracing` / `chrono` / `rand` 目前都被刻意排除在外。

### 解析上游响应要宽容

KuGouMusicApi 是逆向封装，**响应结构会漂移，文档也和实测不一致**。约定：

- 同一语义按**候选键名列表**依次尝试（`pick_string` / `pick_i64` 等）。
- 类型不假设：`AlbumID` 可能是数字、字符串或 `null`。
- 单条解析失败只丢这一条，不影响整页。
- 新增解析逻辑时，请顺手把与文档不一致的地方记进
  [docs/DESIGN.md](docs/DESIGN.md#接口适配) 的「上游文档说 / 实际是」表格。
- **同一个上游可能有多套字段布局**，别只认你调试时看到的那一套。网易云就是例子：
  `/search` 给 `artists` / `album` / `duration`，而 `/playlist/track/all` 与
  `/artists` 给 `ar` / `al` / `dt`——只认前者的话搜索页正常，歌单页与歌手页的歌
  会全部没有歌手、没有专辑、时长显示 `00:00`。**每种布局都要有单元测试钉住。**

### 状态只在主线程改

`app/update.rs` 是唯一修改 `AppState` 的地方；音频线程只通过原子量共享
播放位置、时长、音量。这样不需要加锁，也让状态变更可追溯。

## 怎么加一个新接口

以"加一个浏览类接口"为例：

1. `api/catalog.rs`（或 `cloud.rs`，如果是写操作）加一个 `async fn`，用
   `self.get_json_uncached(...)` 或 `get_json(...)`。
2. `api/model.rs` 加对应的解析函数，按上面的宽容约定写。
3. `event.rs` 的 `Loaded` 枚举加一个变体。
4. `app/update.rs` 加 fetch 方法 + `Loaded` 分支的处理。
5. 需要新按键的话：`keymap.rs` 加 `Action` 变体、按键映射、`CHEATSHEET` 条目，
   以及 `action_from_name` 里的动作名（用户靠这个名字在 `[keymap]` 里重绑）；
   最后同步 `docs/CONFIGURATION.md` 的「自定义键位」动作名清单。
6. 需要展示的话：`app/state.rs` 加列表字段，`ui/` 加渲染。
7. 补单元测试——尤其是解析部分。

## 验证方式

**不要只靠文档验证。** 本项目踩过的坑几乎都来自"文档这么说，实际不这样"。
改动涉及接口时，请：

- 用 `curl` 打真实服务，把原始响应存下来对照；
- 起应用实机点一遍，确认到"能看到具体内容"为止（不只是"列表加载成功"）。

调试按键问题时可以打开详细日志：

```bash
KUGOU_TUI_DEBUG=1 ./target/release/kugou-tui
```

日志会记录每个按键被映射成了什么动作，据此判断是"按键没到程序"还是"映射成了空动作"。
日志路径见 `kugou-tui --print-config`。

## 提交与 PR

- commit message 用一句话说明改了什么；涉及多个改动请拆成多个 commit。
- PR 里请说明：**改了什么、为什么改、怎么验证的**（尤其是接口相关的改动，
  附上一小段真实响应的关键字段会很有帮助）。
- 如果改动会影响用户可见行为，请同步更新 `README.md`。

## 发版

按顺序走，**别跳步**——下面每条都是踩过的。

### 1. 改版本号与变更日志

```bash
# Cargo.toml 里的 version
# CHANGELOG.md 顶部加一节，格式照 [Keep a Changelog]
```

`Cargo.lock` 会跟着变，一起提交。

### 2. 提交、推送、打 tag、发 Release

```bash
git add -A && git commit -m "chore: 版本 X.Y.Z"
git push origin main
git tag -a vX.Y.Z -m "版本 X.Y.Z …"    # 用附注 tag
git push origin vX.Y.Z

cargo build --release
./scripts/make-release-tarball          # 产出 dist/ 里的 tarball 并打印 sha256
gh release create vX.Y.Z --title "vX.Y.Z" --notes-file <说明> \
  dist/kugou-tui-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz
```

**顺序很重要**：`make-release-tarball` 这类工具脚本要在**打 tag 之前**就提交进去，
否则 checkout 那个 tag 拿不到它。

**Release 的 tarball 必须含三个脚本与文档**，不只是二进制——非 Arch 用户拿不到
AUR 包（包里直接带了接口服务），只能靠这个 tarball 把服务配起来，少了脚本
`kugou-tui-install-api` 就跑不了。`make-release-tarball` 已经把这套内容固化下来。

### 3. 更新 AUR 包

AUR 仓库在 `~/aur/kugou-tui`（不在本仓库里）。发新版后要改：

```bash
cd ~/aur/kugou-tui
# 1. PKGBUILD 里的 pkgver
# 2. source 里 tarball 的 sha256sums —— 取新 tag 的：
#      curl -sL -o /tmp/t.tar.gz \
#        "https://github.com/sijin-xb/kugou-tui/archive/refs/tags/vX.Y.Z.tar.gz"
#      sha256sum /tmp/t.tar.gz
makepkg --printsrcinfo > .SRCINFO    # 元数据变了就必须重新生成，否则 AUR 页面不更新
makepkg -f                            # 构建验证，别直接推
git add PKGBUILD .SRCINFO && git commit -m "upgpkg: X.Y.Z-1" && git push
```

### 上游提交号钉在两个地方，改一处不够

接口服务的验证过的提交写在：

- `scripts/kugou-api-install` 的 `PINNED[kugou]`
- AUR 的 `PKGBUILD` 里的 `_api_pin`

**两处要一起改。** 换 `_api_pin` 时还要重新生成 AUR 仓库里的
`kugou-api-package-lock.json`（上游那个提交只有 `pnpm-lock.yaml`，没有
`package-lock.json`，而构建用的是 `npm ci`）。生成命令写在 PKGBUILD 的注释里。

### 4. 发完自己验一遍

**下载回来核对，不要只看本地文件。** 0.3.7 那版就是因为只看本地目录，漏掉了
tarball 里的脚本，一直到有人模拟新用户才发现。

```bash
curl -sL -o /tmp/t.tar.gz "https://github.com/sijin-xb/kugou-tui/releases/download/vX.Y.Z/<资产名>"
tar tzf /tmp/t.tar.gz          # 确认有 scripts/ 与 docs/
tar xzf /tmp/t.tar.gz && ./<解压出的目录>/kugou-tui --version
```

## 法律与合规

本项目只做客户端，不实现任何接口。提交代码时请确保：

- 不包含任何破解、绕过鉴权或规避版权保护措施的实现；
- 不引入会上传用户数据的代码（本项目无任何遥测，登录凭据只存在本机）；
- 涉及第三方服务的改动，请在 PR 里说明用途与合规影响。
