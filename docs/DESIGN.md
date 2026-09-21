# 设计要点

## 设计要点

### 线程模型

```
input thread ──┐
               ├─→ EventBus ─→ main loop ─→ draw (60 fps 上限，实际事件驱动)
async tasks ───┘                    │
                                    └─→ 更新 AppState
audio thread ───────────────────────→ 原子量（位置/时长/音量）
```

- **输入线程**只做 `event::read()`，把按键与鼠标事件塞进通道，不做任何业务判断。
- **主线程**是唯一修改状态的地方，因此不需要锁。
- **播放位置、时长、音量**用原子量共享：主线程直接读，零分配、零锁。
- **Tokio 只开 2 个 worker**：并发上限就是"搜索 + 歌词 + 取链 + 下载"这几条链。

### 低资源占用

实测（release 构建，Linux 6.x x86_64，**100×34 终端**，读 `/proc/<pid>/status` 的
`VmRSS`；已含 MPRIS 的 D-Bus 连接开销）：

| 场景 | 常驻内存 |
|---|---|
| 启动后空闲 | 13.8 MiB |
| 歌单页（列表已加载） | 14.3 MiB |
| 打开歌单（歌曲已加载） | 14.8 MiB |
| 播放中 | 16.8 MiB |

分块并发下载**不额外吃内存**：各块是 seek 到自己的偏移直接写同一个临时文件，
不在内存里拼装整首歌，所以 65 MB 的 Hi-Res 播放时内存还是十几 MiB。

内存与"在放什么"基本无关：128 kbps 的 MP3 和 30 MiB 的 FLAC 常驻内存都是十几 MiB，
因为音频落盘播放，内存里只有解码缓冲。二进制本体约 6 MiB。

主要取舍：事件驱动而非忙轮询（主循环 `recv_timeout(tick)`，默认 200 ms）；
音频完全落盘；下载进度每 256 KiB 才上报一次；不引入 `tracing` / `chrono` / `rand`，
用几十行代码替代它们各自的依赖树。

### 接口适配

KuGouMusicApi 是逆向封装，响应结构会漂移，而且**上游文档与实测不一致**的地方不少。
本项目的做法是"多候选键名 + 类型宽容 + 单条失败不连累整页"。以下是拿真实服务核对后
确认与文档不一致的地方：

| 上游文档说 | 实际是 |
|---|---|
| `/login/qr/create` 返回 `qrcode` / `qrimg` | 返回 `data.url` / `data.base64` |
| 歌单广场在 `data.info` | 在 `data.special_list` |
| 歌手列表元素含 `songcount` | 该字段恒为 `0`，不可用于展示 |
| 歌手列表是扁平数组 | 实际嵌套在 `data.info[].singer[]` |
| `/song/url` 返回 `data.url` | 实际在**顶层** `url` / `backupUrl`（数组），`data` 不存在 |
| 建议 `npm run dev` | `npm start` 才是 `node app.js` |

同一首歌在搜索、歌单、排行榜三个接口里用**三套完全不同的字段名**，这是解析层最花功夫
的地方，也因此写了较多单元测试：

| | 搜索 `/search` | 歌单 `/playlist/track/all` | 排行榜 `/rank/audio` |
|---|---|---|---|
| 歌名 | `SongName` | `name`（含"歌手 - "前缀） | `songname` |
| hash | `FileHash` | `hash` | 顶层没有，在 `audio_info.hash_128` |
| 时长 | `Duration`（秒） | `timelen`（毫秒） | `audio_info.duration_128` |
| 专辑 | `AlbumName` | `albuminfo.name` | `album_info.album_name` |
| 歌手 | `Singer[]` | `singerinfo[]` | `authors[]` / `author_name` |

```bash
cargo test     # 覆盖上述解析路径，以及队列 / 缓存 / 按键的边界情况
```

---
