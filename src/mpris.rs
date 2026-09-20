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
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

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
    /// D-Bus 连接是否真的建起来了。注册是异步的，失败时（没有 session bus 等）
    /// 主线程不该继续往一个没人读的快照里写。
    connected: Arc<std::sync::atomic::AtomicBool>,
}

impl MprisHandle {
    /// 更新快照。每次 tick 调一次即可——桌面组件轮询频率远低于此。
    pub fn update(&self, info: TrackInfo) {
        if let Ok(mut guard) = self.info.lock() {
            *guard = info;
        }
    }

    /// D-Bus 注册是否已成功。未成功时桌面集成不可用，但播放不受影响。
    pub fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::Relaxed)
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

    /// 绝对定位（微秒）。桌面组件**拖进度条**走的是这个，不是 `seek`。
    ///
    /// 之前只实现了 `seek`（相对跳转），结果 TUI 里能拖、DMS 里拖不动——
    /// 因为拖进度条的语义是"跳到某处"，拿相对步进凑不出来。
    async fn set_position(&self, track_id: OwnedObjectPath, position_us: i64) {
        // trackid 只用来校验，我们只有一条轨，不严格比对也能安全处理
        let _ = track_id;
        if position_us < 0 {
            return;
        }
        self.dispatch(Action::SeekTo(position_us as u64 / 1_000));
        // Seeked 信号暂未发送：它只是"建议"，多数客户端（含 DMS、playerctl）
        // 靠轮询 Position 也能同步。等把 SignalContext 正确注入后再补。
    }

    /// `Seeked` 信号：成功跳转后要广播新位置，客户端才会同步显示。
    #[zbus(signal)]
    async fn seeked(ctxt: &SignalEmitter<'_>, position_us: i64) -> zbus::Result<()>;

    /// 相对跳转（微秒）。正负皆可。
    ///
    /// 一次投递一个带数值的动作。早先拆成「N 次 ±5 秒」去凑，拖一次 1 小时的
    /// 进度条就是 720 条事件——主循环单帧只处理 64 条，界面会被自己的事件队列
    /// 饿死。
    async fn seek(&self, offset_us: i64) {
        let step_ms = offset_us / 1_000;
        if step_ms == 0 {
            return;
        }
        self.dispatch(Action::SeekBy(step_ms));
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
        owned(zbus::zvariant::Str::from(
            info.art_url.clone().unwrap_or_default(),
        )),
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
    let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let connected_thread = Arc::clone(&connected);

    let info_clone = Arc::clone(&info);
    // 信号循环需要独立的一份引用：info_clone 会被 move 进 Player
    let info_signal = Arc::clone(&info);
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

            let result: Result<(), zbus::Error> = async {
                let connection = ConnectionBuilder::session()?
                    .name(BUS_NAME)?
                    .serve_at(OBJECT_PATH, player)?
                    .serve_at(OBJECT_PATH, MediaPlayer2)?
                    .build()
                    .await?;

                // 到这一步才说明桌面组件真的能看到我们了
                connected_thread.store(true, std::sync::atomic::Ordering::Relaxed);

                // 属性变化信号。
                //
                // `#[zbus(property)]` 只提供读取，不会在值变化时自动发
                // `PropertiesChanged`。纯轮询的客户端（playerctl）无所谓，
                // 但依赖信号更新的桌面组件会反应滞后甚至不更新。
                // 所以这里定时比对快照，变了就发信号。
                //
                // 0.5 秒足够：媒体控件不需要更实时，而这个循环只是读一次锁。
                let iface = connection
                    .object_server()
                    .interface::<_, Player>(OBJECT_PATH)
                    .await?;
                let mut last: Option<TrackInfo> = None;

                loop {
                    // lock() 返回 Result；锁中毒时取 inner，宁可显示旧值也别卡住循环
                    let current = match info_signal.lock() {
                        Ok(guard) => Some(guard.clone()),
                        Err(poisoned) => Some(poisoned.into_inner().clone()),
                    };

                    let changed = match (&last, &current) {
                        (Some(prev), Some(cur)) => {
                            prev.title != cur.title
                                || prev.artists != cur.artists
                                || prev.album != cur.album
                                || prev.art_url != cur.art_url
                                || prev.status_str() != cur.status_str()
                                // 位置一直在走，只有跳变超过 1 秒才发信号，
                                // 否则每半秒一次太吵
                                || (prev.position_us - cur.position_us).abs() > 1_000_000
                        }
                        _ => true,
                    };

                    if changed {
                        last = current;
                        let ctxt = iface.signal_emitter();
                        // 生成的 *_changed 是实例方法，需要通过接口引用调用
                        let player_ref = iface.get().await;
                        let _ = player_ref.playback_status_changed(ctxt).await;
                        let _ = player_ref.metadata_changed(ctxt).await;
                        let _ = player_ref.position_changed(ctxt).await;
                    }

                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }

                // loop 永不退出，这里只是为了满足类型推断（! 可 coerce 成 ()）
                #[allow(unreachable_code)]
                Ok(())
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

    Some(MprisHandle { info, connected })
}
