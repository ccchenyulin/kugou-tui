//! 全进程唯一的事件总线。
//!
//! # 为什么只留一条通道
//!
//! 参与方有三个：键盘输入线程、音频线程、Tokio 网络任务。如果各自维护一条通道，
//! 主循环就要做多路复用（`select!`），而 Tokio 的 `mpsc` 与 crossbeam 的
//! `select!` 无法直接混用，代码会迅速变脏。
//!
//! 所以统一成一条 `crossbeam_channel::unbounded::<Event>()`：
//!
//! * `unbounded` —— 发送端永不阻塞，因此可以从异步任务里同步调用，不需要 `await`；
//! * 主循环 `recv_timeout(tick)` —— 有事件立刻醒（按键零延迟），无事件就按 tick 刷新。
//!
//! 队列长度由「用户按键速率 + 音频 4Hz 位置上报 + 网络任务完成」决定，天然有界。

use std::path::PathBuf;

use crossbeam_channel::{Receiver, Sender, unbounded};
use ratatui::crossterm::event::{KeyEvent, MouseEvent};

use crate::api::model::{Artist, Lyric, Playlist, RankBoard, Song};
use crate::audio::engine::AudioEvent;
use crate::audio::streaming::StreamingBuffer;
use crate::error::AppError;

/// 歌单歌曲请求的发起方。
///
/// 歌单广场与云端歌单共用同一条请求路径，但结果要落到各自的歌曲面板。
/// 用枚举记录发起方、而不是在结果回来时读「当前标签页」，是因为请求是异步的——
/// 用户完全可能在结果回来之前切走标签页，那样结果就会写错面板。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistSource {
    /// 歌单广场。
    Plaza,
    /// 云端（个人）歌单。
    Cloud,
}

/// 一次异步任务的产出。
///
/// 每个变体都自带「这次请求是针对什么」的上下文（关键词、歌单、歌手、歌曲 hash），
/// 这样即使用户在结果返回前已经切歌或切换视图，主循环也能判断该不该消费这批数据。
#[derive(Debug)]
pub enum Loaded {
    Search {
        keyword: String,
        songs: Vec<Song>,
        /// true 表示追加到现有结果后面（「加载更多」），false 表示替换。
        append: bool,
    },
    /// 歌单广场 / 搜索结果里的歌单列表。
    Playlists {
        title: String,
        items: Vec<Playlist>,
    },
    PlaylistTracks {
        playlist: Playlist,
        songs: Vec<Song>,
        source: PlaylistSource,
    },
    Artists(Vec<Artist>),
    ArtistSongs {
        artist: Artist,
        songs: Vec<Song>,
    },
    RankBoards(Vec<RankBoard>),
    RankTracks {
        board: RankBoard,
        songs: Vec<Song>,
    },
    /// 当前登录用户的云端歌单。
    CloudPlaylists(Vec<Playlist>),
    Lyric {
        hash: String,
        lyric: Lyric,
    },
    /// 已经拿到播放直链。
    StreamReady {
        song: Box<Song>,
        url: String,
        start_at_ms: u64,
        /// 是否为试听片段。为真时播完不应被当作「正常结束」而自动切歌。
        is_trial: bool,
        /// 完整版拿不到的原因（如「需要开通会员或单独购买该专辑」）。
        reason: Option<String>,
    },
    /// 下载进度。已按 256 KiB 节流，不会淹没事件通道。
    DownloadProgress {
        received: u64,
        total: Option<u64>,
    },
    /// 音频已落盘，可以交给音频线程播放。
    StreamCached {
        song: Box<Song>,
        path: PathBuf,
        start_at_ms: u64,
    },
    /// 自动探测到的设备指纹，需要回写配置。
    DeviceFingerprint(String),
    /// 音频缓存已占用字节数。
    CacheUsage(u64),
    /// 登录二维码已就绪：`content` 是二维码内容（一段 URL）。
    LoginQr {
        key: String,
        content: String,
    },
    /// 扫码状态提示（等待扫码 / 待确认 / 已过期）。
    LoginStatus {
        message: String,
    },
    /// 扫码成功，带回登录令牌。
    /// 扫码登录成功。`token` 为 `None` 表示登录态由服务端持有（如网易云），
    /// 客户端不需要也不应该保存凭据。
    LoginSucceeded {
        token: Option<String>,
        userid: Option<String>,
        /// 服务端下发的登录 cookie（网易云走这条路，见 `QrCheck::cookie`）。
        /// 有它就写进配置并在本次会话热更新，之后的请求才带得上身份。
        cookie: Option<String>,
    },
    /// 登录失败。
    LoginFailed {
        message: String,
    },
    /// 封面已解码。`hash` 用于丢弃过期结果（用户已切歌）。
    CoverReady {
        hash: String,
        /// 解码后的原图。图片协议要按显示区域重新编码，所以传解码结果而不是
        /// 原始字节——省掉主线程再解一次。
        image: image::DynamicImage,
    },
    /// 当前账号的会员信息摘要（用于界面显示）。
    VipStatus {
        label: String,
    },
    /// 当前登录用户的资料（昵称 / 头像 / 等级 / 听歌时长）。
    ///
    /// 装箱：`UserInfo` 里几个 `String` 让它比别的变体大一圈，而 `Loaded`
    /// 是每帧都要搬运的枚举。
    UserInfo(Box<crate::api::cloud::UserInfo>),
    /// 头像已下载并解码。协议要回到主线程才能建（\`Picker\` 不是 Send）。
    AvatarReady {
        image: image::DynamicImage,
    },
    /// 流式缓冲已经攒够开头，可以开播了（边下边播）。
    StreamPrerolled {
        song: Box<Song>,
        buffer: StreamingBuffer,
        start_at_ms: u64,
    },
    /// 云端写操作（加歌/删歌）的提示信息。
    CloudNotice(String),
    /// 云端歌单的内容变了（加歌 / 删歌成功），需要重新拉取。
    ///
    /// 少了这一步的表现是：提示「已收藏」，但歌单里看不到这首歌、歌曲数也不变
    /// ——用户只能手动按 `R` 刷新。收到它时重新载入该歌单的歌曲，并刷新歌单列表。
    CloudPlaylistChanged {
        playlist: Box<crate::api::model::Playlist>,
    },
    /// 异步任务失败。
    Failed {
        context: String,
        error: AppError,
    },
}

#[derive(Debug)]
pub enum Event {
    /// 原始按键。
    ///
    /// 刻意不在输入线程里翻译成语义动作：按键的含义取决于当前是否在输入框里，
    /// 只有主循环知道这个状态。让输入线程保持「哑」的，也避免了共享可变状态。
    Key(KeyEvent),
    /// 终端尺寸变化。
    ///
    /// 不携带尺寸：布局每帧都从 `Frame::area()` 重算，这个事件的作用只是把主循环
    /// 从 `recv_timeout` 里立刻唤醒，让用户拖动窗口时画面马上跟上。
    Resize,
    /// 鼠标事件（点击 / 滚轮 / 拖动）。
    Mouse(MouseEvent),
    /// 音频线程上报。
    Audio(AudioEvent),
    /// 网络任务完成。
    Loaded(Box<Loaded>),
    /// 定时心跳：推进进度条与歌词，没有事件时也会到达。
    Tick,
    /// 语义动作（来自 MPRIS 等外部控制源）。
    ///
    /// 与 [`Event::Key`] 分开：按键要经主循环按当前焦点翻译，而这里传进来的
    /// 已经是明确的语义动作（播放 / 下一首 …），直接执行即可。
    Action(crate::keymap::Action),
}

/// 事件总线的发送端。可以自由克隆，跨线程移动。
///
/// 接收端不放在这里——它只属于主循环。crossbeam 的通道在接收端全部丢弃后才
/// 断开，把接收端也塞进可克隆的总线里会让「断开」永远不发生。
#[derive(Debug, Clone)]
pub struct EventBus {
    sender: Sender<Event>,
}

impl EventBus {
    /// 创建总线，返回发送端与唯一的接收端。
    pub fn new() -> (Self, Receiver<Event>) {
        let (sender, receiver) = unbounded();
        (Self { sender }, receiver)
    }

    /// 发送一个事件。接收端已关闭（进程正在退出）时静默忽略。
    pub fn send(&self, event: Event) {
        let _ = self.sender.send(event);
    }

    /// 发送一次异步产出。
    pub fn emit(&self, loaded: Loaded) {
        self.send(Event::Loaded(Box::new(loaded)));
    }

    /// 上报一次异步失败，`context` 说明是哪个操作失败了。
    pub fn fail(&self, context: impl Into<String>, error: AppError) {
        self.emit(Loaded::Failed {
            context: context.into(),
            error,
        });
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new().0
    }
}
