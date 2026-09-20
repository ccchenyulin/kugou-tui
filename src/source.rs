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
//! 地址、登录态、设备指纹这三样差异就是全部，且已由 `Config::switch_source` 统一切换。
//! 曾有一个 `Source` 行为包装（转发 search / playlist_tracks / stream_url），但因为
//! 两个平台接口语义完全一致，它只会多一层无意义转发，且从未被使用——已按死代码删除。
//! 将来接入**非酷狗**音源时，再按 [`SourceKind`] 分派各自的请求与解析实现，改动只在本文件与调用点。
//!

use serde::{Deserialize, Serialize};

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
