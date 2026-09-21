//! 音源抽象层。
//!
//! # 为什么按「标准版 / 概念版」划分
//!
//! 酷狗的两个平台不是同一个皮肤，而是**两套独立的会员与鉴权体系**：
//!
//! - 平台由 KuGouMusicApi 服务端的 `platform` 环境变量决定（`lite` = 概念版），
//!   服务端据此切换 `appid` / `clientver`，因此**必须各跑一个服务实例**；
//! - 上游文档明确写了「不同版本的平台的 token 是不通用的」——标准版的登录态
//!   拿到概念版去用，会员不会被识别，会退化成试听片段；
//! - 设备标识 `dfid` 同样是平台相关的。
//!
//! 所以这里的「音源」= 「服务地址 + 登录态 + 设备标识」三元组，切换时整体替换。
//!
//! # 界面与业务怎么用它
//!
//! [`Source`] 只暴露三个能力：搜索、取歌单歌曲、取播放直链。两个平台走的是同一套
//! 接口语义（都是 KuGouMusicApi），因此这里不需要按平台分派——差异全部收敛在
//! `api_base` / `cookie` / `device_id` 上。这也是它比接一个全新音源便宜得多的原因。
//!
//! # 当前接入状态
//!
//! 已经打通并验证的：配置持久化（`config.rs`）、切换动作（`update.rs` 的
//! `App::switch_source`）、界面显示与 `--print-config`。
//!
//! 还没做的：运行时目前**直接持有 `ApiClient`**，没有套这层 `Source`。因为两个
//! 平台共用同一个客户端，`Source` 在当下只是转发，套上去要改所有 `self.api.xxx()`
//! 调用点，收益为零、风险不为零。等接入**非酷狗**音源（那时才需要按 kind 分派）
//! 时再套，改动才划算。
//!
//! 所以本文件**只保留数据模型**（[`SourceKind`] / [`SourceProfile`] / [`SourceSet`]）：
//! 地址、登录态、设备指纹这三样差异就是全部，已由 `Config::switch_source` 统一切换。
//! 曾有一个 `Source` 行为包装（转发 search / playlist_tracks / stream_url），但两个平台
//! 接口语义完全一致，它只会多一层无意义转发，且从未被调用——按死代码删除。
//! 将来接入**非酷狗**音源时，再按 [`SourceKind`] 分派各自的请求与解析实现。
//!
//! 所以本文件**只保留数据模型**（[`SourceKind`] / [`SourceProfile`] / [`SourceSet`]）：
//! 地址、登录态、设备指纹这三样差异就是全部，且已由 `Config::switch_source` 统一切换。
//! 曾有一个 `Source` 行为包装（转发 search / playlist_tracks / stream_url），但因为
//! 两个平台接口语义完全一致，它只会多一层无意义转发，且从未被使用——已按死代码删除。
//! 将来接入**非酷狗**音源时，再按 [`SourceKind`] 分派各自的请求与解析实现，改动只在本文件与调用点。
//!

pub mod netease;

use serde::{Deserialize, Serialize};

use crate::api::client::ApiClient;
use crate::api::model::{Artist, Lyric, Playlist, RankBoard, Song};
use crate::error::Result;

/// 音源种类。
///
/// # 新增一个音源要动哪里
///
/// 1. 在这里加一个变体；
/// 2. 在下面 `label` / `default_api_base` / `capability` / `platform_env` 里补分支；
/// 3. 在 [`SourceSet`] 里加一个配置字段；
/// 4. 在 [`crate::source::dispatch`] 的分派函数里加分支，指向新模块的实现。
///
/// 业务层只通过 `SourceKind` 的方法调用，不感知具体音源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// 酷狗音乐标准版。默认 `http://127.0.0.1:3000`（服务端不设 `platform`）。
    #[default]
    Kugou,
    /// 酷狗概念版。默认 `http://127.0.0.1:3001`（服务端 `platform=lite`）。
    KugouConcept,
    /// 网易云音乐（NeteaseCloudMusicApi）。
    Netease,
}

/// 一个音源具备哪些能力。
///
/// 不是所有音源都能提供全部功能：例如第三方服务往往没有「云端歌单同步」对应的
/// 登录态，元数据也不完整。UI 据此决定要不要展示某个面板，而不是等调用失败了
/// 再报错——那样用户只会看到一个「不支持」的提示，却不知道为什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// 能否取播放直链。
    pub stream: bool,
    /// 能否取歌词。
    pub lyric: bool,
    /// 搜索结果里是否带封面地址。
    pub cover: bool,
    /// 是否支持扫码登录。
    pub login: bool,
    /// 登录态是否由**客户端**持有（需要存进配置）。
    ///
    /// 酷狗是这样：token 由客户端保存，每次请求带上。
    /// 网易云的 NeteaseCloudMusicApi 则是服务端自己管 cookie，客户端拿不到
    /// token——这时登录成功只意味着「服务端那边登上了」，不该去写 config.cookie。
    pub client_token: bool,
    /// 是否支持「歌单广场 / 榜单 / 歌手」这类目录浏览。
    pub catalog: bool,
    /// 是否支持云端歌单的读写（收藏、同步、增删改）。
    pub cloud: bool,
}

impl SourceKind {
    pub const ALL: [SourceKind; 3] = [
        SourceKind::Kugou,
        SourceKind::KugouConcept,
        SourceKind::Netease,
    ];

    /// 界面显示名。
    pub fn label(self) -> &'static str {
        match self {
            SourceKind::Kugou => "酷狗",
            SourceKind::KugouConcept => "酷狗概念版",
            SourceKind::Netease => "网易云",
        }
    }

    /// 该音源具备的能力。
    pub fn capability(self) -> Capability {
        match self {
            // 酷狗两个平台共用 KuGouMusicApi，能力完全一致
            SourceKind::Kugou | SourceKind::KugouConcept => Capability {
                stream: true,
                lyric: true,
                cover: true,
                login: true,
                client_token: true,
                catalog: true,
                cloud: true,
            },
            // 网易云：NeteaseCloudMusicApi 提供扫码登录与完整的云端歌单接口，
            // 读（列表 + 曲目）与写（加歌/删歌/建/删歌单）都已支持。
            // 目录类（歌单广场、歌手、排行榜）不走这一套，仍然不可用。
            SourceKind::Netease => Capability {
                stream: true,
                lyric: true,
                cover: true,
                login: true,
                client_token: false,
                catalog: false,
                cloud: true,
            },
        }
    }

    /// 该音源默认的服务地址。
    ///
    /// 酷狗两个平台各占一个端口：它们需要不同的 `platform` 环境变量，
    /// 而一个 Node 进程只能加载一份 `.env`。
    pub fn default_api_base(self) -> &'static str {
        match self {
            SourceKind::Kugou => "http://127.0.0.1:3000",
            SourceKind::KugouConcept => "http://127.0.0.1:3001",
            SourceKind::Netease => "http://127.0.0.1:3002",
        }
    }

    /// 该音源服务端的 `platform` 取值，用于启动脚本与文档提示。
    pub fn platform_env(self) -> Option<&'static str> {
        match self {
            SourceKind::Kugou => None,
            SourceKind::KugouConcept => Some("lite"),
            SourceKind::Netease => None,
        }
    }

    /// 该音源是否需要酷狗那套设备指纹（`dfid`）。
    ///
    /// 只有酷狗用得上：它把 dfid 拼进 cookie 一起发给上游做风控校验。
    /// 其它平台没有这个机制，客户端也就不该去请求。
    pub fn uses_device_fingerprint(self) -> bool {
        matches!(self, SourceKind::Kugou | SourceKind::KugouConcept)
    }

    /// 扫码要用哪个 App。登录提示里会念出这个名字。
    ///
    /// 不能写死「酷狗」：登录提示是在**选定音源之后**才显示的，
    /// 对着网易云的用户说「用酷狗 App 扫码」纯属误导。
    pub fn scan_app(self) -> &'static str {
        match self {
            SourceKind::Kugou | SourceKind::KugouConcept => "酷狗",
            SourceKind::Netease => "网易云音乐",
        }
    }

    /// 对应的第三方服务项目，用于启动脚本与文档提示。
    pub fn service_name(self) -> &'static str {
        match self {
            SourceKind::Kugou | SourceKind::KugouConcept => "KuGouMusicApi",
            SourceKind::Netease => "NeteaseCloudMusicApi",
        }
    }
}

/// 一个音源的连接与身份信息。
///
/// **三个字段都是平台相关的，不能跨音源复用。**
// 刻意不 derive Default：见下面的手写实现（默认必须是「已启用」）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceProfile {
    /// 该音源 API 服务的地址。
    pub api_base: String,
    /// 登录态，形如 `token=xxx; userid=xxx`。平台间不通用。
    pub cookie: Option<String>,
    /// 设备指纹 `dfid`，同样是平台相关的。
    pub device_id: Option<String>,
    /// 是否启用。禁用的音源不参与轮转，也不在音源管理页被优先展示。
    ///
    /// 默认 `true`：老配置文件里没有这个字段，不能因为升级就把音源关掉。
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// 优先级，数字小的排前面。
    ///
    /// 只在音源管理页调整顺序时用到；取值相同则按 [`SourceKind::ALL`] 的声明顺序，
    /// 保证排序稳定、结果可预期。
    #[serde(default)]
    pub priority: u32,
}

fn default_enabled() -> bool {
    true
}

/// 手写 Default 而不是 derive：`enabled` 必须默认是 **true**。
///
/// 配置文件里缺某个音源的段时，serde 会走这里的 Default —— 若用 derive，
/// bool 会拿到 false，新加的音源一上来就是禁用的，用户得先手动启用才能用。
impl Default for SourceProfile {
    fn default() -> Self {
        Self {
            api_base: String::new(),
            cookie: None,
            device_id: None,
            enabled: true,
            priority: 0,
        }
    }
}

impl SourceProfile {
    pub fn new(kind: SourceKind) -> Self {
        Self {
            api_base: kind.default_api_base().to_string(),
            cookie: None,
            device_id: None,
            enabled: true,
            priority: Self::default_priority(kind),
        }
    }

    /// 拼出可直接放进请求头的 cookie（必要时补上 dfid）。
    ///
    /// 与 `Config::cookie_header` 同样的规则，区别是这里读的是**本音源档案**
    /// 里的凭据。跨音源播放时要用它——队列里的歌可能来自另一个音源，
    /// 拿当前音源的 cookie 去请求是错的。
    pub fn cookie_header(&self) -> Option<String> {
        let base = self.cookie.as_deref().unwrap_or_default().trim();
        let has_dfid = base.split(';').any(|pair| pair.trim().starts_with("dfid="));

        match (base.is_empty(), has_dfid, self.device_id.as_deref()) {
            (true, _, Some(dfid)) => Some(format!("dfid={dfid}")),
            (true, _, None) => None,
            (false, false, Some(dfid)) => Some(format!("{base}; dfid={dfid}")),
            (false, _, _) => Some(base.to_string()),
        }
    }

    /// 默认优先级：按声明顺序拉开间距，方便 UI 把某个音源插到中间。
    fn default_priority(kind: SourceKind) -> u32 {
        SourceKind::ALL
            .iter()
            .position(|candidate| *candidate == kind)
            .unwrap_or(0) as u32
            * 10
    }
}

/// 全部音源的配置，以及当前选中的那个。
///
/// 字段是具名的而不是 `Vec`：这样老的配置文件（只有酷狗两个音源）仍能正常解析，
/// 缺的音源走 `Default`，不会因为升级丢掉已有的登录态与地址。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSet {
    #[serde(default)]
    pub kugou: SourceProfile,
    #[serde(default)]
    pub kugou_concept: SourceProfile,
    #[serde(default)]
    pub netease: SourceProfile,
    /// 当前选中的音源。
    pub active: SourceKind,
}

impl Default for SourceSet {
    fn default() -> Self {
        Self {
            kugou: SourceProfile::new(SourceKind::Kugou),
            kugou_concept: SourceProfile::new(SourceKind::KugouConcept),
            netease: SourceProfile::new(SourceKind::Netease),
            active: SourceKind::Kugou,
        }
    }
}

impl SourceSet {
    pub fn profile(&self, kind: SourceKind) -> &SourceProfile {
        match kind {
            SourceKind::Kugou => &self.kugou,
            SourceKind::KugouConcept => &self.kugou_concept,
            SourceKind::Netease => &self.netease,
        }
    }

    pub fn profile_mut(&mut self, kind: SourceKind) -> &mut SourceProfile {
        match kind {
            SourceKind::Kugou => &mut self.kugou,
            SourceKind::KugouConcept => &mut self.kugou_concept,
            SourceKind::Netease => &mut self.netease,
        }
    }

    /// 按优先级排序后的音源列表（禁用的也在，界面自行决定如何展示）。
    pub fn ordered(&self) -> Vec<SourceKind> {
        let mut kinds = SourceKind::ALL.to_vec();
        kinds.sort_by_key(|kind| {
            (
                self.profile(*kind).priority,
                SourceKind::ALL
                    .iter()
                    .position(|candidate| candidate == kind)
                    .unwrap_or(0),
            )
        });
        kinds
    }

    /// 已启用的音源，按优先级排序。切换音源时只在它们之间轮转。
    pub fn enabled(&self) -> Vec<SourceKind> {
        self.ordered()
            .into_iter()
            .filter(|kind| self.profile(*kind).enabled)
            .collect()
    }
}

// ============================================================================
// 分派：业务层只调这里，不感知具体音源
//
// 新增音源 = 在本文件加枚举变体 + 在下面各方法加一个分支 + 写一个实现模块。
// ============================================================================

/// 给解析出来的歌曲盖上来源章。
///
/// 每个返回 `Vec<Song>` 的分派方法都要调它——队列允许跨音源，
/// 播放时必须知道每首歌该回哪个音源取链接。
fn stamp_songs(songs: &mut [Song], kind: SourceKind) {
    for song in songs {
        song.source = kind;
    }
}

/// 音源不支持某能力时的统一报错。
fn unsupported(what: &str) -> crate::error::AppError {
    crate::error::AppError::Other(format!("当前音源不支持{what}"))
}

impl SourceKind {
    /// 单曲搜索。
    pub async fn search_songs(
        self,
        client: &ApiClient,
        keyword: &str,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Song>> {
        let mut songs = match self {
            Self::Kugou | Self::KugouConcept => client.search_songs(keyword, page, page_size).await,
            Self::Netease => netease::search_songs(client, keyword, page, page_size).await,
        }?;
        stamp_songs(&mut songs, self);
        Ok(songs)
    }

    /// 取播放直链。
    pub async fn song_stream_url(
        self,
        client: &ApiClient,
        song: &Song,
        quality: &str,
    ) -> Result<crate::api::catalog::StreamUrl> {
        match self {
            Self::Kugou | Self::KugouConcept => client.song_stream_url(song, quality).await,
            Self::Netease => netease::song_stream_url(client, song, quality).await,
        }
    }

    /// 取封面图片地址。不支持或取不到时返回 \`Ok(None)\`。
    ///
    /// 酷狗与 QQ 音乐的搜索结果里直接带封面 URL；网易云的搜索结果**只有
    /// \`picId\` 没有 URL**，得再查一次 \`/song/detail\` 才能拿到——实测确认。
    pub async fn cover_url(self, client: &ApiClient, song: &Song) -> Result<Option<String>> {
        match self {
            Self::Kugou | Self::KugouConcept => Ok(song.cover.clone()),
            Self::Netease => netease::cover_url(client, song).await,
        }
    }

    /// 取歌词。
    pub async fn fetch_lyric(self, client: &ApiClient, song: &Song) -> Result<Lyric> {
        match self {
            Self::Kugou | Self::KugouConcept => client.fetch_lyric(song).await,
            Self::Netease => netease::fetch_lyric(client, song).await,
        }
    }

    /// 扫码登录第一步：取 key。
    pub async fn login_qr_key(self, client: &ApiClient) -> Result<String> {
        match self {
            Self::Kugou | Self::KugouConcept => client.login_qr_key().await,
            Self::Netease => netease::login_qr_key(client).await,
        }
    }

    /// 扫码登录第二步：取二维码内容。
    pub async fn login_qr_create(self, client: &ApiClient, key: &str) -> Result<String> {
        match self {
            Self::Kugou | Self::KugouConcept => client.login_qr_create(key).await,
            Self::Netease => netease::login_qr_create(client, key).await,
        }
    }

    /// 扫码登录第三步：轮询结果。
    pub async fn login_qr_check(
        self,
        client: &ApiClient,
        key: &str,
    ) -> Result<crate::api::cloud::QrCheck> {
        match self {
            Self::Kugou | Self::KugouConcept => client.login_qr_check(key).await,
            Self::Netease => netease::login_qr_check(client, key).await,
        }
    }

    // ---- 以下为酷狗专属能力 ----
    //
    // 其它音源直接报「不支持」，而不是返回空列表：第三方服务没有对应的登录态，
    // 返回空会让用户以为是网络问题或自己操作错了，明确说不支持反而省事。

    pub async fn plaza_playlists(
        self,
        client: &ApiClient,
        category_id: i64,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Playlist>> {
        match self {
            Self::Kugou | Self::KugouConcept => {
                client.plaza_playlists(category_id, page, page_size).await
            }
            other => Err(unsupported(&format!("歌单广场（{}）", other.label()))),
        }
    }

    pub async fn playlist_tracks_all(
        self,
        client: &ApiClient,
        global_id: &str,
        fresh: bool,
    ) -> Result<Vec<Song>> {
        let mut songs = match self {
            Self::Kugou | Self::KugouConcept => client.playlist_tracks_all(global_id, fresh).await,
            other => Err(unsupported(&format!("歌单歌曲（{}）", other.label()))),
        }?;
        stamp_songs(&mut songs, self);
        Ok(songs)
    }

    pub async fn artist_list(
        self,
        client: &ApiClient,
        kind: i64,
        hot_size: u32,
    ) -> Result<Vec<Artist>> {
        match self {
            Self::Kugou | Self::KugouConcept => client.artist_list(kind, hot_size).await,
            other => Err(unsupported(&format!("歌手列表（{}）", other.label()))),
        }
    }

    pub async fn artist_tracks_all(
        self,
        client: &ApiClient,
        artist_id: i64,
        sort: &str,
    ) -> Result<Vec<Song>> {
        let mut songs = match self {
            Self::Kugou | Self::KugouConcept => client.artist_tracks_all(artist_id, sort).await,
            other => Err(unsupported(&format!("歌手歌曲（{}）", other.label()))),
        }?;
        stamp_songs(&mut songs, self);
        Ok(songs)
    }

    pub async fn rank_boards(self, client: &ApiClient) -> Result<Vec<RankBoard>> {
        match self {
            Self::Kugou | Self::KugouConcept => client.rank_boards().await,
            other => Err(unsupported(&format!("排行榜（{}）", other.label()))),
        }
    }

    pub async fn rank_tracks_all(self, client: &ApiClient, rank_id: i64) -> Result<Vec<Song>> {
        let mut songs = match self {
            Self::Kugou | Self::KugouConcept => client.rank_tracks_all(rank_id).await,
            other => Err(unsupported(&format!("榜单歌曲（{}）", other.label()))),
        }?;
        stamp_songs(&mut songs, self);
        Ok(songs)
    }

    pub async fn user_playlists(self, client: &ApiClient) -> Result<Vec<Playlist>> {
        match self {
            Self::Kugou | Self::KugouConcept => client.user_playlists().await,
            Self::Netease => netease::user_playlists(client).await,
        }
    }

    pub async fn user_playlist_tracks_all(
        self,
        client: &ApiClient,
        list_id: i64,
        fresh: bool,
    ) -> Result<Vec<Song>> {
        let mut songs = match self {
            Self::Kugou | Self::KugouConcept => {
                client.user_playlist_tracks_all(list_id, fresh).await
            }
            Self::Netease => netease::user_playlist_tracks_all(client, list_id).await,
        }?;
        stamp_songs(&mut songs, self);
        Ok(songs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 跨音源队列的核心不变式：分派层必须给每首歌盖上来源章。
    ///
    /// 没有这个章，播放时就只能拿「当前音源」去取链接——而队列是允许跨音源的
    /// （酷狗搜几首入队 → 切到网易云），那些酷狗的歌会因为 hash 在网易云
    /// 的接口里查不到而全部播不了。
    #[test]
    fn stamp_marks_every_song_with_its_source() {
        let mut songs = vec![Song::default(), Song::default()];
        stamp_songs(&mut songs, SourceKind::Netease);
        assert!(
            songs.iter().all(|song| song.source == SourceKind::Netease),
            "每首歌都要带上来源音源"
        );
    }

    /// 盖章要覆盖解析时填的初始值：酷狗标准版与概念版共用同一套解析，
    /// 解析函数里填的是 `Kugou`，概念版必须被改写成 `KugouConcept`，
    /// 否则取链接会打到标准版的端口上。
    #[test]
    fn stamp_overrides_parse_time_default() {
        let mut songs = vec![Song::default()];
        assert_eq!(songs[0].source, SourceKind::Kugou, "默认是酷狗");
        stamp_songs(&mut songs, SourceKind::KugouConcept);
        assert_eq!(songs[0].source, SourceKind::KugouConcept, "应被改写");
    }

    /// 跨音源取链接要用**目标音源档案**里的凭据，不能拿当前音源的。
    #[test]
    fn profile_cookie_header_uses_own_credentials() {
        let mut profile = SourceProfile::new(SourceKind::Netease);
        assert_eq!(profile.cookie_header(), None, "没有凭据时不给 cookie");

        profile.cookie = Some("token=abc; userid=1".to_string());
        profile.device_id = Some("df-1".to_string());
        assert_eq!(
            profile.cookie_header().as_deref(),
            Some("token=abc; userid=1; dfid=df-1"),
            "应把 dfid 拼进去"
        );

        // 已经带了 dfid 就不要重复拼
        profile.cookie = Some("token=abc; dfid=own".to_string());
        assert_eq!(
            profile.cookie_header().as_deref(),
            Some("token=abc; dfid=own")
        );
    }
}
