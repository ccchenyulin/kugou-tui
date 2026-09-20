//! MPRIS：把播放器注册成一个标准的 Linux 媒体播放器。
//!
//! # 为什么需要它
//!
//! TUI 跑在终端里，桌面组件（媒体控件、状态栏小组件、`playerctl`）只认 D-Bus 上的
//! `org.mpris.MediaPlayer2`。不注册的话，它们在系统里完全看不到这个播放器——
//! 外表看就像"这个程序不会放音乐"。
//!
//! # 结构
//!
//! * 本模块在 session bus 上注册两个接口：`org.mpris.MediaPlayer2`（身份）
//!   与 `org.mpris.MediaPlayer2.Player`（播放控制 + 元数据）
//! * **命令方向**：D-Bus 方法被调用 → 转成 [`Action`] → 经 EventBus 送进主循环。
//!   状态只在主线程改，MPRIS 不直接碰播放状态。
//! * **状态方向**：主循环每次 tick 调用 [`MprisHandle::update`] 刷新共享快照，
//!   D-Bus 属性读取时从快照取，不需要反向调用主线程。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::connection::Builder as ConnectionBuilder;
use zbus::interface;
use zbus::zvariant::OwnedValue;

use crate::audio::engine::PlaybackState;
use crate::event::{Event, EventBus};
use crate::keymap::Action;

/// D-Bus 上的总线名与对象路径。
///
/// 名字里的 `kugou-tui` 必须是合法的 bus name（不能有下划线以外的问题字符）。
const BUS_NAME: &str = "org.mpris.MediaPlayer2.kugou-tui";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";

/// 桌面组件读取的播放信息快照。
///
/// 由主线程写入、MPRIS 线程读取，所以套 Mutex。字段都不大，锁竞争可忽略。
#[derive(Debug, Clone, Default)]
pub struct TrackInfo {
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    /// 封面 URL。酷狗的 `sizable_cover` 含 `{size}` 占位符，需要替换成具体像素值。
    pub art_url: Option<String>,
    /// 当前位置（微秒，MPRIS 的单位）。
    pub position_us: i64,
    /// 总时长（微秒）。
    pub duration_us: i64,
    pub status: PlaybackState,
}

impl TrackInfo {
    /// MPRIS 的 PlaybackStatus：Playing / Paused / Stopped。
    fn status_str(&self) -> &'static str {
        match self.status {
            PlaybackState::Playing => "Playing",
            PlaybackState::Paused => "Paused",
            PlaybackState::Stopped | PlaybackState::Loading => "Stopped",
        }
    }
}

/// 主循环持有它，用来刷新 D-Bus 上看到的播放信息。
#[derive(Debug, Clone)]
pub struct MprisHandle {
    info: Arc<Mutex<TrackInfo>>,
}

impl MprisHandle {
    /// 更新快照。每次 tick 调一次即可——桌面组件轮询频率远低于此。
    pub fn update(&self, info: TrackInfo) {
        if let Ok(mut guard) = self.info.lock() {
            *guard = info;
        }
    }
}

/// D-Bus 上的 Player 接口实现。
struct Player {
    info: Arc<Mutex<TrackInfo>>,
    bus: EventBus,
}

impl Player {
    /// 把语义动作送进主循环。状态只在主线程修改。
    fn dispatch(&self, action: Action) {
        self.bus.send(Event::Action(action));
    }

    fn with_info<R>(&self, f: impl FnOnce(&TrackInfo) -> R) -> R {
        match self.info.lock() {
            Ok(guard) => f(&guard),
            // 锁中毒时给个默认值，宁可显示空也不要让 D-Bus 调用挂住
            Err(poisoned) => f(&poisoned.into_inner()),
        }
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    async fn play(&self) {
        // 只在非播放时才切换，避免"正在播放时收到 Play"反而暂停
        let playing = self.with_info(|info| info.status == PlaybackState::Playing);
        if !playing {
            self.dispatch(Action::PlayPause);
        }
    }

    async fn pause(&self) {
        let playing = self.with_info(|info| info.status == PlaybackState::Playing);
        if playing {
            self.dispatch(Action::PlayPause);
        }
    }

    async fn play_pause(&self) {
        self.dispatch(Action::PlayPause);
    }

    async fn next(&self) {
        self.dispatch(Action::Next);
    }

    async fn previous(&self) {
        self.dispatch(Action::Prev);
    }

    async fn stop(&self) {
        // 我们没有独立的"停止"动作；退而暂停，比什么都不做强。
        let playing = self.with_info(|info| info.status == PlaybackState::Playing);
        if playing {
            self.dispatch(Action::PlayPause);
        }
    }

    /// 相对跳转（微秒）。正负皆可。
    async fn seek(&self, offset_us: i64) {
        let step_ms = offset_us / 1_000;
        if step_ms == 0 {
            return;
        }
        // 只有前后步进，没有绝对定位的动作。多次触发会串行执行，够用。
        let count = step_ms.abs() / 5_000;
        let action = if step_ms > 0 {
            Action::SeekForward
        } else {
            Action::SeekBackward
        };
        for _ in 0..count.max(1) {
            self.dispatch(action);
        }
    }

    #[zbus(property)]
    async fn playback_status(&self) -> String {
        self.with_info(|info| info.status_str().to_string())
    }

    #[zbus(property)]
    async fn metadata(&self) -> HashMap<String, OwnedValue> {
        self.with_info(build_metadata)
    }

    #[zbus(property)]
    async fn position(&self) -> i64 {
        self.with_info(|info| info.position_us)
    }

    #[zbus(property)]
    async fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn can_seek(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn can_control(&self) -> bool {
        true
    }
}

/// 组装 MPRIS 的 Metadata（`a{sv}`）。
///
/// 字段名是 MPRIS 规定的 `xesam:` 前缀。注意 `mpris:artUrl` 要给**完整 URL**，
/// 酷狗返回的 `sizable_cover` 里的 `{size}` 必须替换掉，否则拿到的是无效地址。
/// 把值转成 MPRIS 元数据用的 `OwnedValue`。
///
/// zvariant 只给部分类型实现了 `From`，其余走 `TryFrom`。统一走 `try_from`：
/// `From<T>` 有个 blanket 实现转成 `TryFrom<T>`，所以两种情况都能覆盖。
/// 转换失败（理论上不会发生）时退回空串，不让 D-Bus 调用因此报错。
fn owned<T>(value: T) -> OwnedValue
where
    OwnedValue: std::convert::TryFrom<T>,
{
    OwnedValue::try_from(value)
        .ok()
        .unwrap_or_else(|| OwnedValue::from(zbus::zvariant::Str::from("")))
}

fn build_metadata(info: &TrackInfo) -> HashMap<String, OwnedValue> {
    let mut map: HashMap<String, OwnedValue> = HashMap::new();

    map.insert(
        "mpris:trackid".to_string(),
        OwnedValue::from(zbus::zvariant::ObjectPath::from_static_str_unchecked(
            "/org/kugou_tui/Track/1",
        )),
    );
    map.insert("mpris:length".to_string(), owned(info.duration_us));
    map.insert(
        "mpris:artUrl".to_string(),
        owned(zbus::zvariant::Str::from(expand_cover(
            info.art_url.as_deref(),
        ))),
    );
    map.insert(
        "xesam:title".to_string(),
        owned(zbus::zvariant::Str::from(info.title.clone())),
    );
    map.insert(
        "xesam:album".to_string(),
        owned(zbus::zvariant::Str::from(info.album.clone())),
    );
    map.insert("xesam:artist".to_string(), {
        // Vec<String> 没有直接到 OwnedValue 的转换，先转成 zvariant 的 Array
        let names: Vec<&str> = info.artists.iter().map(|name| name.as_str()).collect();
        let array = zbus::zvariant::Array::from(&names);
        OwnedValue::try_from(zbus::zvariant::Value::from(array))
            .ok()
            .unwrap_or_else(|| owned(zbus::zvariant::Str::from("")))
    });

    map
}

/// 展开封面 URL 里的 `{size}` 占位符。
///
/// 酷狗的 `sizable_cover` 形如：
/// `http://imge.kugou.com/stdmusic/{size}/20200819/20200819143052763547.jpg`
/// 不替换的话这就是个 404。取 400 像素：桌面控件一般显示得不大，没必要拉原图。
fn expand_cover(url: Option<&str>) -> String {
    match url {
        Some(url) if url.contains("{size}") => url.replace("{size}", "400"),
        Some(url) => url.to_string(),
        None => String::new(),
    }
}

/// 身份接口。桌面组件靠它显示播放器名字。
struct MediaPlayer2;

#[interface(name = "org.mpris.MediaPlayer2")]
impl MediaPlayer2 {
    #[zbus(property)]
    async fn identity(&self) -> String {
        "kugou-tui".to_string()
    }

    #[zbus(property)]
    async fn can_quit(&self) -> bool {
        false // 退出应该由用户在终端里按 q，别让桌面组件把程序关了
    }

    #[zbus(property)]
    async fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property)]
    async fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

/// 启动 MPRIS 服务。
///
/// 失败不阻塞播放：没有 D-Bus（比如纯 tty 环境）时只是少了桌面集成，
/// 程序该放歌还是放歌。所以这里返回 `Option`。
pub fn spawn(bus: EventBus) -> Option<MprisHandle> {
    let info = Arc::new(Mutex::new(TrackInfo::default()));

    let info_clone = Arc::clone(&info);
    // 用独立线程跑 tokio 运行时：zbus 的连接是异步的，而我们的网络运行时
    // 只有 2 个 worker 且可能被下载占满，不适合再塞一个长驻连接。
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(_) => return,
        };

        runtime.block_on(async move {
            let player = Player {
                info: info_clone,
                bus,
            };

            let result = async {
                let connection = ConnectionBuilder::session()?
                    .name(BUS_NAME)?
                    .serve_at(OBJECT_PATH, player)?
                    .serve_at(OBJECT_PATH, MediaPlayer2)?
                    .build()
                    .await?;
                // 连接必须一直持有，drop 掉就等于从总线注销了。
                // 这里挂起直到进程结束。
                let _connection = connection;
                std::future::pending::<()>().await;
                Ok::<(), zbus::Error>(())
            }
            .await;

            if let Err(error) = result {
                crate::logger::tlog!(
                    crate::logger::LEVEL_WARN,
                    "MPRIS 注册失败（不影响播放）：{error}"
                );
            }
        });
    });

    Some(MprisHandle { info })
}
