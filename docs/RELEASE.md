# 发版与清理

覆盖**版本与标签规则**、**发版产物包含什么**、**发版流程的顺序与依赖**、**每条清理路径何时触发、清掉什么**。

实现：`scripts/release`（发版流程）、`scripts/clean`（构建产物清理）、`src/audio/streaming.rs` + `src/audio/cache.rs`（运行期清理）。只描述**已验证的行为**；未验证的会点明。

---

## 1. 版本与标签规则

| 项 | 规则 | 由谁强制 |
|---|---|---|
| 版本 | `Cargo.toml` 的 `version`，纯数字 `X.Y.Z`（不带 `-alpha` 后缀） | `scripts/release` 预检 |
| 标签 | 固定 `vX.Y.Z`，**附注标签**（`git tag -a`），消息里写 release notes | `scripts/release` |
| 标签唯一性 | 本地与远端都检查；已存在则中止 | `scripts/release` |
| 更新日志 | `CHANGELOG.md` 必须有 `## [X.Y.Z]` 小节 | `scripts/release` |
| 二进制自报版本 | 构建出的二进制 `--version` 必须与 `Cargo.toml` 一致 | `scripts/release` |

**标签不可重来。** 版本用过就换新的；不要删了重打 —— crates.io 的版本同样不可逆。

---

## 2. 发版产物包含什么

产物是 tarball：`kugou-tui-<version>-x86_64-unknown-linux-gnu.tar.gz`（0.6.2 实测 5.3 MB），由 `scripts/make-release-tarball` 产出。它**必须自包含**：这是**不用 `cargo install` 的用户唯一可用的路径** —— tarball 不带接口服务，所以它得把搭起服务所需的一切都装上。

| 内容 | 为什么必须在 |
|---|---|
| `kugou-tui`（二进制，约 11.7 MiB，已 strip） | 主程序 |
| `scripts/kugou-api` | 启停接口服务 |
| `scripts/kugou-api-install` | **拉取并配置服务** —— 没有它用户搭不起服务，流程就断了 |
| `scripts/kugou-tui` | 启动器（按需拉起服务） |
| `README.md`、`KEYBINDINGS.md`、`CHANGELOG.md`、`LICENSE` | 基础文档 |
| `docs/*.md` | 完整文档 |

`scripts/release` 会逐项检查这张清单，缺任何一项就中止。

> 0.3.7 及更早的 Release **只有二进制和 README**，缺了三个脚本 —— 用户解压后跑不了 `kugou-api-install`。手工拼目录很容易漏东西，所以这一步现在是脚本。

**不包含**：接口服务本身（独立项目，用户按 `docs/INSTALL.md` 自行拉取）。用 `cargo build --release` 构建。

### 三平台包由 CI 产出

`scripts/release` 在开发机上跑，产出 `x86_64-unknown-linux-gnu` 的 tarball；另外两个平台**在开发机上构建不了 —— 这是物理限制，不是疏忽**：Windows 的 `.exe` 需要 MSVC 工具链，macOS 二进制需要 Apple SDK。

所以三个平台都走 `.github/workflows/release.yml`：`v*` 标签推送时，在 `ubuntu-latest` / `windows-latest` / `macos-latest` 上各构建一份、打包，挂到同一个 Release 上。这样发版**不需要开发机在场**，产物也不依赖任何特定机器。

| 平台 | 产物 | 打包脚本 |
|---|---|---|
| Linux x86_64 | `kugou-tui-<version>-x86_64-unknown-linux-gnu.tar.gz` | `make-release-tarball`（CI 与本地） |
| Windows x86_64 | `kugou-tui-<version>-x86_64-pc-windows-msvc.zip` | `build-windows.ps1`（CI） |
| macOS arm64 | `kugou-tui-<version>-aarch64-apple-darwin.tar.gz` | `make-release-tarball`（CI） |

> **Linux 有两个来源。** 本地 `scripts/release` 仍会构建并上传一份（它同时校验产物内容）；CI 也构建一份同名产物，被 `upload-release-asset` 的 `--clobber` 覆盖 —— 后到的赢。所以**两份 sha256 不保证一致**：rustc 小版本与目标机器的 C 工具链都会影响 `aws-lc-sys` 的构建。本地发版时记录的 hash 和之后从 Release 下载的 hash 对不上是正常的，不是文件被换过。想让 hash 只有一个来源，就把 Linux 的打包上传从 `scripts/release` 里去掉、完全交给 CI —— 那是发版流程的改动，单独做。

> **Intel Mac 不在范围内**：`macos-latest` 是 arm64 机器，它的二进制在 Intel 上跑不了。要覆盖就得在同一个 runner 上用 `--target x86_64-apple-darwin` 交叉编译 —— 包名不同（`x86_64-apple-darwin`），多一个 job；现在没做。

### 打标签前就能验证打包脚本

`release.yml` 只在标签推送后才跑，而**标签不可逆** —— 打包脚本漏了文件，要么发个坏包，要么认了（0.3.7 就撞过）。所以打包脚本在推标签之前有两个验证点，**都在 `release.yml` 之外**：

1. `scripts/release` 的**第一阶段** —— 本地跑 `make-release-tarball`，逐项检查内容清单，然后停下等确认；
2. `release.yml` 自带的 **dry-run**（见下）—— 把三平台的构建、打包、产物传递端到端跑一遍，只跳过 Release 创建与产物挂载。平台特有分支（macOS 的 BSD 工具、Windows 的 PowerShell 打包脚本）只有真跑才能验证。

> `ci.yml` **这些都不做**：它那三列只跑 `clippy + test` —— 没有 release 构建、没有发版打包、没有产物传递。这是有意的：release 构建用 `lto = "fat"` 和 `codegen-units = 1`，每次推送都为三个平台跑一遍，耗时是数倍，而产物没人用（要二进制的人该去 Releases 页）。原因记在 `ci.yml` 头部注释里。

### dry-run：不打标签跑完整条流水线

改 `release.yml` 没法用「推个标签试试」来验证 —— 标签不可逆。所以 `workflow_dispatch` 多了一个 `dry_run` 输入：整条流水线照跑（三平台构建、打包、产物上传、取回、资产清单检查），**只跳过最后的 Release 创建与产物挂载**；远端一个字节都不写。

它检出 `main`，而版本检查拿 `TAG` 与二进制的 `--version` 比对，所以传的标签必须与 `main` 当前版本一致：

```bash
gh workflow run release.yml --ref <branch> -f tag=v0.6.1 -f dry_run=true
```

### 回填产物（`workflow_dispatch`）

给**已经发过**的版本回填产物时，手动指定标签 —— 比如 0.4.2 发版时这个 workflow 还不存在：

```bash
gh workflow run release.yml -f tag=v0.4.2
```

回填路径检出 `main`（`scripts/release-notes` 只在 main 上更新，旧标签里的副本可能还不认识要回填的版本）。`publish` 先看 Release 是否存在：存在就跳过创建、直接上传产物（`--clobber` 覆盖同名文件）；不存在就创建一个。

### 时序：三个平台全绿后才出现 Release

三个构建 job（`linux` / `windows` / `macos`）**只构建打包**，各自把包作为 artifact 上传，都不碰 Release。第四个 job `publish` `needs` 那三个，取回产物、检查清单（2 个 tarball + 1 个 zip），然后**一次性**创建 Release 并挂上全部产物。workflow 级权限是 `contents: read`；写权限只给 `publish`。因此发版页面上不会出现「Release 已存在但产物还没到」的窗口，构建失败则根本不创建 Release。

> 以前是反的：第一个 job `prepare` 先创建 Release，然后三个平台各自加产物 —— 任何一个平台构建失败，都会留下一个已发布、产物却永远补不齐的 Release。

本地 `scripts/release` 仍可用，且做得更多（校验 CHANGELOG 有对应版本小节、把产物下载回来校验）。它的顺序是「推标签 → 创建 Release → 上传产物」，会与 `publish` 撞车：两者都是幂等的 —— Release 存在时 `publish` 跳过创建，`upload-release-asset` 用 `--clobber` —— **谁先到谁创建，两种顺序都不报错**。

zip 里也带那三个 **bash** 脚本（在 Git Bash / WSL 下仍可用），macOS 的 tarball 同理 —— 包里装的是「在别的平台上也能用的东西」，而不是「本平台的原生脚本」，后者由源码仓库提供。

---

## 3. 发版推送的目标仓库

| 目标 | URL | 推什么 | 凭据 |
|---|---|---|---|
| GitHub（源码与 Release） | `github.com/sijin-xb/kugou-tui` | `main` 分支、`vX.Y.Z` 标签、Release 产物 | `gh` 已登录 |

---

## 4. 执行顺序与依赖

`./scripts/release` 一条命令做完全部。分两个阶段；**第二阶段全是不可逆的外部动作**，所以脚本在第一阶段后停下，列出将要执行的每一项动作，等确认。

### 第一阶段：本地（可重复，无外部影响）

| # | 步骤 | 依赖 |
|---|---|---|
| 1 | 预检（版本格式、分支、工作区干净、与 origin 同步、标签未被占用、CHANGELOG 有对应小节） | — |
| 2 | 清理上一轮的中间产物（调用 `scripts/clean`） | 1 |
| 3 | `cargo clippy --all-targets` + `cargo test` | 2 |
| 4 | `cargo build --release`，并校验二进制自报版本 | 3 |
| 5 | 构建发版包，并逐项检查内容清单 | 4 |
| 6 | 列出第二阶段动作，等待确认 | 5 |

### 第二阶段：外部（不可逆）

| # | 步骤 | 依赖 | 备注 |
|---|---|---|---|
| 7 | `git push origin main` | 6 | 网络动作，带退避重试 |
| 8 | `git tag -a vX.Y.Z` + `git push origin vX.Y.Z` | 7 | **不可逆** |
| 9 | `gh release create` 并挂上发版包 | 8 | 幂等：已存在则改用 `gh release upload --clobber` |
| 10 | 把 Release 产物**下载回来**，校验内容与 sha256 | 9 | 不能只信本地文件 |

### 失败与恢复

- 第一阶段任何一步失败：什么都没发出去，修好重跑。
- 第二阶段中途失败：脚本会说明停在哪一步。**不要删标签重来**；保留已推送的东西，手工补齐缺的部分（第 9–10 步都幂等，可以单独重跑）。
- 第 10 步失败**必须停下排查** —— 它意味着本地构建出的东西与用户下载到的不一致。

---

## 5. 清理逻辑

### 5.1 构建与打包产物（已实现：`scripts/clean`）

**何时触发**

1. 发版开始**之前** —— 由 `scripts/release` 自动调用，保证构建从干净状态开始
2. 发版成功**之后** —— 也可以再手动跑
3. **手动** —— 磁盘紧张时随时

**默认清掉什么**

| 默认清理 | 位置 | 实测占用 |
|---|---|---|
| 是 | `target/package/` | 4.7 MiB |
| 是 | `dist/` 里**非当前版本**的 tarball | 3.2 MiB / 版本 |

每次约 **8 MiB**；留几个版本就是几百 MiB。

**明确不动**（要清得加显式开关）

| 开关 | 额外清什么 | 为什么不默认 |
|---|---|---|
| `--all` | 整个 `target/` | 会毁掉增量编译，下次全量重建约 3 分钟。**但它是这里最大的一项**：实测 `target/` 有 **26 GiB**（`debug/` 25 GiB —— clippy 与测试产物；`release/` 1.2 GiB）。磁盘紧张时这一项比其他所有加起来还管用 |
| `--cache` | `~/.cache/kugou-tui`（含已下载的歌曲） | 那是**用户数据**，不是构建产物 |

`--dry-run` 只报告不删除。每次删除前都会确认「这确实是我们要删的东西」—— 宁可什么都不删。

### 5.2 运行期内存（已实现：`src/audio/streaming.rs`）

边下边播的缓冲**只留尾部窗口**（`MAX_KEPT_BYTES = 4 MiB`；超过就回收到 `TARGET_KEPT_BYTES = 1 MiB`），窗口之外的字节用定位读（Unix `pread` / Windows `seek_read`）从磁盘上的 `.part` 文件读回。常驻内存因此与曲目大小解耦：64 MiB 的 Hi-Res 文件只占几 MiB，往回 seek 到已回收区域照样能播（那部分在磁盘上）。设计与实测见 [DESIGN.md](DESIGN.md#边下边播)。

触发与水位：`push` 时检查「只有内存窗口超过 `max` 才回收一次」，且一次降到 `target` —— 不是每次 push 都做，因为回收是一次 `Vec::split_off` 加一次分配，频繁调用代价高。这个「只在溢出时动手、一次降到底」的模式与磁盘缓存的 `EVICT_TARGET_RATIO`（降到 80%）一致。

只丢**读位置之前**的字节；之后的永不丢（解码器马上要读）。回看窗口由「落后读位置多远」定义，与时钟无关，所以没有定时回收。

守护测试：`streaming::tests::keeps_only_a_bounded_window_in_memory`（断言窗口长度而非 RSS —— 单元测试与并行测试、分配器缓存共用一个进程，`VmRSS` 断言不了；见 [MAINTENANCE.md](MAINTENANCE.md) §5）。RSS 层面的对照实验在进程外做，方法与基线见 [MAINTENANCE.md](MAINTENANCE.md) §6。
