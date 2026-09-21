//! 应用编排层。
//!
//! [`App`] 把三个独立的部分接在一起，并驱动主循环：
//!
//! ```text
//!  ┌────────────┐   Event    ┌──────────────────────────────────┐
//!  │ 输入线程    │ ─────────▶ │                                  │
//!  ├────────────┤            │            EventBus              │
//!  │ 音频线程    │ ─────────▶ │        (crossbeam channel)       │
//!  ├────────────┤            │                                  │
//!  │ Tokio 任务  │ ─────────▶ │                                  │
//!  └────────────┘            └───────────────┬──────────────────┘
//!                                            │ recv_timeout(tick)
//!                                            ▼
//!                             ┌──────────────────────────────┐
//!                             │  主循环（本线程）              │
//!                             │  1. terminal.draw(ui::render) │
//!                             │  2. handle_event(...)         │
//!                             │  3. drain_events()            │
//!                             └──────────────────────────────┘
//! ```
//!
//! 主循环是**唯一**修改 [`AppState`] 的地方，因此不需要任何锁来保护 UI 状态。
//! 跨线程共享的只有「音频位置/音量」这类原子量与一条无锁通道。

/// 猜终端支持哪种图形协议。
///
/// **刻意不用** `Picker::from_query_stdio()`。它向终端发一串查询序列再阻塞读
/// stdin，而那个读**没有自己的超时**：终端一旦不回应（tmux、部分终端、某些 SSH
/// 组合），读线程就永远卡在 `stdin().read()` 上，之后用户按的每一个键都被它
/// 吞掉——实测表现是整个键盘失灵，`q` 都退不出去。它顺手开关的那次 raw mode
/// 还会和 `ratatui::init()` 打架。
///
/// 改成只看环境变量：kitty 与 iTerm2 都会留下明确痕迹，猜不出就退回半块字符。
/// 代价是自动检测不到 sixel（这类终端很少），换来的是绝不会抢走输入、也绝不会
/// 让启动多等两秒。
fn detect_image_picker() -> ratatui_image::picker::Picker {
    use ratatui_image::picker::{Picker, ProtocolType};

    let mut picker = Picker::halfblocks();
    let is_kitty = std::env::var_os("KITTY_WINDOW_ID").is_some()
        || std::env::var("TERM").is_ok_and(|term| term.contains("kitty"));
    let is_iterm2 = std::env::var("TERM_PROGRAM").is_ok_and(|value| value == "iTerm.app");

    if is_kitty {
        picker.set_protocol_type(ProtocolType::Kitty);
    } else if is_iterm2 {
        picker.set_protocol_type(ProtocolType::Iterm2);
    }
    picker
}

pub mod queue;
pub mod settings;
pub mod state;
pub mod update;

use std::time::{Duration, Instant};

use anyhow::Context;
use crossbeam_channel::{Receiver, RecvTimeoutError};

use crate::api::ApiClient;
use crate::app::state::Tab;
use crate::audio::engine::PlaybackState;
use crate::audio::{AudioCache, AudioHandle, Downloader};
use crate::config::Config;
use ratatui::crossterm::execute;

use crate::event::{Event, EventBus, Loaded};
use crate::logger::tlog;

pub use state::AppState;

/// 主循环每帧最多处理的事件数在 [`update`] 里定义。
pub struct App {
    pub state: AppState,

    /// 事件总线的发送端。音频线程、输入线程、网络任务各持有一份克隆。
    bus: EventBus,
    /// 接收端只有主循环持有。
    receiver: Receiver<Event>,

    api: ApiClient,
    audio: AudioHandle,
    cache: AudioCache,
    downloader: Downloader,

    /// 网络运行时。只在需要发起请求时 `spawn`，主线程不 `block_on`。
    runtime: tokio::runtime::Runtime,

    /// 上一帧的时间戳，用来算出真实经过时长（dt），供动画做时间无关的缓动。
    last_frame_at: Instant,

    /// MPRIS 句柄。没有 D-Bus 时为 `None`（不影响播放，只是桌面集成不可用）。
    mpris: Option<crate::mpris::MprisHandle>,

    /// 终端图形协议的探测器（kitty / iTerm2 / sixel，都没有就退到半块字符）。
    ///
    /// 只在启动时探测一次：探测要临时开关 raw mode 并向终端发查询序列，
    /// 每帧做一次既慢又会打断输入。`None` 表示探测失败（输出不是终端等），
    /// 此时封面只能走字符画兜底。
    picker: Option<ratatui_image::picker::Picker>,
}

impl App {
    /// 装配所有子系统。
    pub fn new(config: Config) -> anyhow::Result<Self> {
        config.ensure_cache_dir().context("创建音频缓存目录失败")?;

        let runtime = tokio::runtime::Builder::new_multi_thread()
            // 2 个 worker 足够：并发上限就是「搜索 + 歌词 + 取链 + 下载」这几条链
            .worker_threads(2)
            .thread_name("kugou-net")
            .enable_all()
            .build()
            .context("创建网络运行时失败")?;

        let (bus, receiver) = EventBus::new();

        let api = ApiClient::new(
            &config.api_base,
            config.cookie_header(),
            config.proxy.as_deref(),
        )
        .context("初始化 API 客户端失败")?;

        // 自定义键位要在第一次读键之前装好，否则首个按键会落到默认键表。
        // 装了多少条只在日志里记，不打扰界面。
        let _custom_keys = crate::keymap::install_custom(&config.keymap);

        let audio = AudioHandle::spawn(bus.clone(), config.volume);
        let cache = AudioCache::new(config.cache_dir.clone(), config.cache_limit_mib);
        let downloader = Downloader::new(config.proxy.as_deref()).context("初始化下载器失败")?;

        // 先取一份克隆给 MPRIS：bus 随后会被 move 进 App，之后就借不到了
        let mpris = crate::mpris::spawn(bus.clone());

        let state = AppState::new(config);

        let mut app = Self {
            state,
            bus,
            receiver,
            api,
            audio,
            cache,
            downloader,
            runtime,
            last_frame_at: Instant::now(),
            mpris,
            picker: None,
        };

        app.picker = Some(detect_image_picker());

        app.announce_readiness();
        // 放在 announce_readiness 之后：这种故障比「未登录」严重，提示不能被覆盖
        if app.audio.spawn_failed() {
            app.state.error("音频线程启动失败，播放不可用（详见日志）");
        }
        app.ensure_device_fingerprint();
        app.refresh_cache_usage();
        app.fetch_vip_status();

        let tab = app.state.tab;
        app.ensure_tab_loaded(tab);

        Ok(app)
    }

    /// 启动主循环，返回后终端已恢复。
    pub fn run(&mut self) -> anyhow::Result<()> {
        let mut terminal = ratatui::init();

        // 开启鼠标捕获：点击列表、滚轮翻页、点击进度条都要它。失败不影响键盘使用，
        // 有些终端/远程会话不支持，忽略即可。
        let _ = execute!(
            std::io::stdout(),
            ratatui::crossterm::event::EnableMouseCapture
        );

        // 终端进入 raw 模式后再启动输入线程，避免首个按键被行缓冲吃掉
        spawn_input_thread(self.bus.clone());

        let loop_result = self.event_loop(&mut terminal);

        // 先关鼠标捕获再恢复终端，否则有些终端会残留鼠标上报
        let _ = execute!(
            std::io::stdout(),
            ratatui::crossterm::event::DisableMouseCapture
        );
        ratatui::restore();
        self.shutdown();

        loop_result
    }

    /// 主循环。
    ///
    /// 一帧的顺序是「先渲染再取事件」：这样 `recv_timeout` 的等待时间被用来
    /// 呈现上一帧的结果，用户感知到的按键延迟就等于一次渲染加一次唤醒，
    /// 而不是「等满一个 tick 才响应」。
    /// 当前这一帧该等多久。
    ///
    /// 平时用配置的 `tick_ms`（默认 200ms，省电）。但可视化页在播放时柱子要连续
    /// 起落，5fps 会明显卡顿，所以临时提到 ~30fps（33ms）。
    ///
    /// 为什么不到 60fps：终端一次重绘是整屏 diff，16ms 与 33ms 的观感差别很小，
    /// 代价却是翻倍的 CPU。这里按「看起来顺」而不是「数字好看」取值。
    fn frame_interval(&self) -> Duration {
        // 30fps。终端里再往上（60fps）看不出差别，但重绘成本是线性的，
        // 白白吃掉「低资源占用」这个卖点，所以取这个折中值。
        const ANIMATED_TICK_MS: u64 = 33;
        /// 简易模式的刷新间隔下限（5fps）。再慢界面就拖了。
        const LITE_TICK_MS: u64 = 200;
        let base = self.state.config.tick_ms;
        let animating =
            self.state.tab == Tab::Visualizer && self.state.playback == PlaybackState::Playing;

        // 简易模式：不做动画提速，并且把刷新压到 5fps（200ms）——省下的都是
        // CPU 与重绘，听歌不受影响。
        let millis = if self.state.config.lite_mode {
            base.max(LITE_TICK_MS)
        } else if animating && base > ANIMATED_TICK_MS {
            ANIMATED_TICK_MS
        } else {
            base
        };
        Duration::from_millis(millis)
    }

    fn event_loop(&mut self, terminal: &mut ratatui::DefaultTerminal) -> anyhow::Result<()> {
        loop {
            terminal
                .draw(|frame| crate::ui::render(frame, &mut self.state))
                .context("渲染失败")?;

            match self.receiver.recv_timeout(self.frame_interval()) {
                Ok(event) => self.handle_event(event),
                // 没有事件时用一次心跳推进进度条与歌词
                Err(RecvTimeoutError::Timeout) => self.handle_event(Event::Tick),
                Err(RecvTimeoutError::Disconnected) => break,
            }

            // 把同一帧内积压的事件一并消化，快速连按时画面才不会滞后
            self.drain_events();

            if self.state.should_quit {
                break;
            }
        }

        Ok(())
    }

    fn announce_readiness(&mut self) {
        let base = self.api.base().to_string();
        if self.state.logged_in {
            self.state.info(format!("已连接 {base}（已登录）"));
        } else {
            self.state
                .warn(format!("已连接 {base}（未登录，云端歌单不可用）"));
        }
    }

    /// 首次运行时自动探测设备指纹。
    ///
    /// `dfid` 是 `/song/url` 的必需参数，缺失时酷狗会返回「本次请求需要验证」。
    /// 探测失败不阻塞任何功能——只是取播放链接时会更依赖登录态。
    fn ensure_device_fingerprint(&mut self) {
        // `dfid` 是酷狗独有的设备指纹（`/register/dev`），网易云、QQ 音乐没有
        // 这个概念。对它们发这个请求只会白跑一趟，还会在配置里留下一个
        // 语义不明的 device_id，看着像是登录凭据。
        if !self
            .state
            .config
            .active_source_kind()
            .uses_device_fingerprint()
        {
            return;
        }
        if self.state.config.dfid.is_some() {
            return;
        }

        let api = self.api.clone();
        let bus = self.bus.clone();

        self.runtime.spawn(async move {
            match api.fetch_device_fingerprint().await {
                Ok(dfid) => bus.emit(Loaded::DeviceFingerprint(dfid)),
                Err(error) => tlog!(
                    crate::logger::LEVEL_WARN,
                    "获取设备指纹失败（不影响搜索与浏览）：{error}"
                ),
            }
        });
    }

    /// 收尾：保存配置、停掉音频线程。
    fn shutdown(&mut self) {
        if self.state.force_quit {
            tlog!(crate::logger::LEVEL_INFO, "强制退出，跳过配置保存");
        } else {
            self.persist_config();
        }
        self.audio.shutdown();
    }

    fn persist_config(&mut self) {
        self.state.config.volume = self.state.volume;
        self.state.config.playback_mode = self.state.queue.mode();
        self.state.config.cache_dir = self.cache.root().to_path_buf();
        // `--api-base` 只覆盖本次会话，不能落盘。
        //
        // 地址属于音源自己（见 `Config::sync_active_source` 的注释）。把临时值写进
        // 配置文件，就会出现「选中音源是概念版 :3001、顶层却写着 :3000」——启动器
        // 照顶层值去探活 / 拉服务就会找错端口，程序直接起不来。
        let active = self.state.config.active_source_kind();
        self.state.config.api_base = self.state.config.sources.profile(active).api_base.clone();

        match self.state.config.save() {
            Ok(()) => tlog!(
                crate::logger::LEVEL_INFO,
                "配置已保存到 {}",
                Config::path().display()
            ),
            Err(error) => tlog!(crate::logger::LEVEL_ERROR, "保存配置失败：{error}"),
        }
    }
}

/// 启动终端输入线程。
///
/// 线程阻塞在 `event::read()` 上，退出时不需要显式回收——`main` 返回会让整个
/// 进程结束，阻塞中的线程随之消失。
fn spawn_input_thread(bus: EventBus) {
    let result = std::thread::Builder::new()
        .name("kugou-input".to_string())
        .spawn(move || input_loop(bus));

    if let Err(error) = result {
        tlog!(
            crate::logger::LEVEL_ERROR,
            "启动输入线程失败，键盘将无响应：{error}"
        );
    }
}

fn input_loop(bus: EventBus) {
    use ratatui::crossterm::event::{self, Event as TerminalEvent, KeyEventKind};

    loop {
        match event::read() {
            // 只处理 Press：部分平台还会派发 Release/Repeat，全部处理会让按键翻倍
            Ok(TerminalEvent::Key(key)) if key.kind == KeyEventKind::Press => {
                bus.send(Event::Key(key));
            }
            Ok(TerminalEvent::Resize(_, _)) => {
                bus.send(Event::Resize);
            }
            Ok(TerminalEvent::Mouse(mouse)) => {
                bus.send(Event::Mouse(mouse));
            }
            // 焦点变化、鼠标等事件暂时不关心
            Ok(_) => {}
            Err(error) => {
                tlog!(crate::logger::LEVEL_ERROR, "读取终端输入失败：{error}");
                break;
            }
        }
    }
}
