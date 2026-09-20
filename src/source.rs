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
//! 因此本文件暂时 `allow(dead_code)`：它是预留的接入点，不是废弃代码。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::api::ApiClient;
use crate::api::model::Song;
use crate::error::{AppError, Result};

/// 音源种类（酷狗的两个平台）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// 酷狗音乐标准版。默认 `http://127.0.0.1:3000`（服务端不设 `platform`）。
    #[default]
    Kugou,
    /// 酷狗概念版。默认 `http://127.0.0.1:3001`（服务端 `platform=lite`）。
    KugouConcept,
}

impl SourceKind {
    pub const ALL: [SourceKind; 2] = [SourceKind::Kugou, SourceKind::KugouConcept];

    /// 界面显示名。
    pub fn label(self) -> &'static str {
        match self {
            SourceKind::Kugou => "酷狗",
            SourceKind::KugouConcept => "酷狗概念版",
        }
    }

    /// 该音源默认的服务地址。
    ///
    /// 两个平台各占一个端口：它们需要不同的 `platform` 环境变量，
    /// 而一个 Node 进程只能加载一份 `.env`。
    pub fn default_api_base(self) -> &'static str {
        match self {
            SourceKind::Kugou => "http://127.0.0.1:3000",
            SourceKind::KugouConcept => "http://127.0.0.1:3001",
        }
    }

    /// 该音源服务端的 `platform` 取值，用于启动脚本与文档提示。
    pub fn platform_env(self) -> Option<&'static str> {
        match self {
            SourceKind::Kugou => None,
            SourceKind::KugouConcept => Some("lite"),
        }
    }

    /// 轮转到下一个音源（切换入口用）。
    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

/// 一个音源的连接与身份信息。
///
/// **三个字段都是平台相关的，不能跨音源复用。**
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceProfile {
    /// 该音源 API 服务的地址。
    pub api_base: String,
    /// 登录态，形如 `token=xxx; userid=xxx`。平台间不通用。
    pub cookie: Option<String>,
    /// 设备指纹 `dfid`，同样是平台相关的。
    pub device_id: Option<String>,
}

impl SourceProfile {
    pub fn new(kind: SourceKind) -> Self {
        Self {
            api_base: kind.default_api_base().to_string(),
            cookie: None,
            device_id: None,
        }
    }
}

/// 全部音源的配置，以及当前选中的那个。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSet {
    pub kugou: SourceProfile,
    pub kugou_concept: SourceProfile,
    /// 当前选中的音源。
    pub active: SourceKind,
}

impl Default for SourceSet {
    fn default() -> Self {
        Self {
            kugou: SourceProfile::new(SourceKind::Kugou),
            kugou_concept: SourceProfile::new(SourceKind::KugouConcept),
            active: SourceKind::Kugou,
        }
    }
}

impl SourceSet {
    pub fn profile(&self, kind: SourceKind) -> &SourceProfile {
        match kind {
            SourceKind::Kugou => &self.kugou,
            SourceKind::KugouConcept => &self.kugou_concept,
        }
    }

    pub fn profile_mut(&mut self, kind: SourceKind) -> &mut SourceProfile {
        match kind {
            SourceKind::Kugou => &mut self.kugou,
            SourceKind::KugouConcept => &mut self.kugou_concept,
        }
    }
}

/// 一个已连接的音源。
///
/// 两个平台共用同一套接口语义，因此这里不需要按 kind 分派——差异已经在
/// `api_base` / `cookie` / `device_id` 里了。新增一个**非酷狗**音源时，才需要
/// 在这里按 kind 分派到各自的实现。
pub struct Source {
    pub kind: SourceKind,
    client: ApiClient,
}

impl Source {
    /// 按音源配置建立连接。
    pub fn new(kind: SourceKind, profile: &SourceProfile, proxy: Option<&str>) -> Result<Self> {
        if profile.api_base.trim().is_empty() {
            return Err(AppError::Config(format!(
                "「{}」音源还没有配置服务地址，请在配置文件里填 `api_base`",
                kind.label()
            )));
        }

        let client = ApiClient::new(&profile.api_base, profile.cookie.clone(), proxy)?;
        Ok(Self { kind, client })
    }

    pub fn client(&self) -> &ApiClient {
        &self.client
    }

    /// 取设备指纹（两个平台都需要，且各自的 dfid 不通用）。
    pub async fn fetch_device_id(&self) -> Result<String> {
        self.client.fetch_device_fingerprint().await
    }

    /// 搜索单曲。
    pub async fn search(&self, keyword: &str, page: u32, page_size: u32) -> Result<Vec<Song>> {
        self.client.search_songs(keyword, page, page_size).await
    }

    /// 取歌单内全部歌曲（内部自动翻页）。
    pub async fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Song>> {
        self.client.playlist_tracks_all(playlist_id).await
    }

    /// 取播放直链（完整版优先，拿不到才退成试听片段）。
    pub async fn stream_url(&self, song: &Song, quality: &str) -> Result<String> {
        self.client
            .song_stream_url(song, quality)
            .await
            .map(|stream| stream.url)
    }
}
