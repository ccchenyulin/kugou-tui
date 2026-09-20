//! 播放引擎。
//!
//! # 线程模型
//!
//! 音频设备独占一个名为 `kugou-audio` 的线程。原因是 Linux 上 cpal 的
//! `Stream`（`rodio::MixerDeviceSink` 持有它）不是 `Send`，无法跨线程传递，
//! 所以「创建设备 → 创建 Player → 消费命令」必须都发生在同一个线程里。
//!
//! ```text
//!        主线程                          kugou-audio 线程
//!   ┌──────────────┐   AudioCmd       ┌──────────────────────┐
//!   │ App / UI     │ ───────────────▶ │ MixerDeviceSink      │
//!   │              │  (crossbeam)     │ Player (rodio)       │
//!   │              │                  │                      │
//!   │              │ ◀─────────────── │ Event::Audio(..)     │
//!   └──────────────┘   EventBus       └──────────────────────┘
//!          ▲                                    │
//!          │ 读原子量（位置/时长/音量/状态）        │ 写原子量
//!          └────────────────────────────────────┘
//! ```
//!
//! # 为什么位置信息走原子量而不是 channel
//!
//! UI 每帧（默认 200ms）都要读一次播放位置。如果走 channel，音频线程要按
//! 「音频帧率」或至少「UI 帧率」投递消息，主循环的消息队列会被位置更新淹没，
//! 白白增加分配与调度开销。用 4 个原子量共享，读写都是几条指令，零分配。
//!
//! 只有**离散事件**（装载完成、播放结束、出错）才走 channel。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use rodio::Source;

use crate::audio::levels::{AudioLevels, LevelMeter};
use crate::event::{Event, EventBus};
use crate::logger::tlog;

/// 音频线程的轮询间隔。
///
/// 200ms 让进度条视觉上连续，同时把空转开销压到可忽略：一次轮询只是读几个
/// 原子量加两次 `Mutex` 短锁，5 次/秒的代价约等于零。
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// 快进/快退步长（毫秒）。
pub const SEEK_STEP_MS: i64 = 5_000;

/// 音量调节步长。
pub const VOLUME_STEP: f32 = 0.05;

// ============================================================================
// 对外类型
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackState {
    /// 没有装载任何音源。
    #[default]
    Stopped,
    /// 已发出播放请求，正在下载或解码。
    Loading,
    Playing,
    Paused,
}

impl PlaybackState {
    fn code(self) -> u8 {
        match self {
            Self::Stopped => 0,
            Self::Loading => 1,
            Self::Playing => 2,
            Self::Paused => 3,
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Loading,
            2 => Self::Playing,
            3 => Self::Paused,
            _ => Self::Stopped,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Stopped => "已停止",
            Self::Loading => "缓冲中",
            Self::Playing => "播放中",
            Self::Paused => "已暂停",
        }
    }
}

/// 音频线程上报的离散事件。
#[derive(Debug)]
pub enum AudioEvent {
    /// 音源已装载，`duration_ms` 是解码器给出的真实时长（可能为 0）。
    Ready { duration_ms: u64 },
    /// 当前曲目自然播放结束，主循环据此切下一首。
    TrackFinished,
    /// 打开设备或解码失败。
    Failed(String),
}

/// 主线程 → 音频线程的命令。
#[derive(Debug)]
enum AudioCmd {
    /// 装载本地文件。`start_at_ms` 用于「恢复上次播放位置」。
    Load {
        path: PathBuf,
        start_at_ms: u64,
        /// 解码器报不出时长时的兜底，来自列表数据。
        expected_duration_ms: u64,
    },
    Toggle,
    Stop,
    SeekTo(u64),
    SeekBy(i64),
    SetVolume(f32),
    Shutdown,
}

// ============================================================================
// 共享快照
// ============================================================================

/// 音频线程与主线程共享的只读快照。
#[derive(Debug)]
struct Shared {
    state: AtomicU8,
    position_ms: AtomicU64,
    duration_ms: AtomicU64,
    /// `f32` 的位模式。
    volume_bits: AtomicU32,
}

impl Shared {
    fn new(volume: f32) -> Self {
        Self {
            state: AtomicU8::new(PlaybackState::Stopped.code()),
            position_ms: AtomicU64::new(0),
            duration_ms: AtomicU64::new(0),
            volume_bits: AtomicU32::new(volume.clamp(0.0, 1.0).to_bits()),
        }
    }

    fn state(&self) -> PlaybackState {
        PlaybackState::from_code(self.state.load(Ordering::Relaxed))
    }

    fn set_state(&self, state: PlaybackState) {
        self.state.store(state.code(), Ordering::Relaxed);
    }

    fn position_ms(&self) -> u64 {
        self.position_ms.load(Ordering::Relaxed)
    }

    fn duration_ms(&self) -> u64 {
        self.duration_ms.load(Ordering::Relaxed)
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    fn set_volume(&self, volume: f32) {
        self.volume_bits
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
}

// ============================================================================
// 句柄
// ============================================================================

/// 播放引擎句柄。可克隆地发送命令，读取状态则直接走原子量。
pub struct AudioHandle {
    command_tx: Sender<AudioCmd>,
    shared: Arc<Shared>,
    /// 播放电平。音频线程写，主线程读。
    levels: AudioLevels,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for AudioHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AudioHandle")
            .field("state", &self.state())
            .field("position_ms", &self.position_ms())
            .field("duration_ms", &self.duration_ms())
            .field("volume", &self.volume())
            .finish()
    }
}

impl AudioHandle {
    /// 启动音频线程。
    ///
    /// 设备打开失败不会让进程退出：错误通过 [`AudioEvent::Failed`] 上报，
    /// 界面照常可用（用户可以继续浏览、搜索、管理歌单）。
    pub fn spawn(bus: EventBus, initial_volume: f32) -> Self {
        let (command_tx, command_rx) = unbounded();
        let shared = Arc::new(Shared::new(initial_volume));
        let thread_shared = Arc::clone(&shared);

        // 先建好再分身：句柄和音频线程必须是**同一个** AudioLevels，
        // 否则主线程读到的永远是零，柱子不会动。
        let levels = AudioLevels::new();
        let thread_levels = levels.clone();

        let thread = thread::Builder::new()
            .name("kugou-audio".to_string())
            .spawn(move || run(command_rx, bus, thread_shared, thread_levels))
            .map_err(|error| {
                tlog!(crate::logger::LEVEL_ERROR, "启动音频线程失败：{error}");
                error
            })
            .ok();

        Self {
            command_tx,
            shared,
            levels,
            thread,
        }
    }

    /// 音频线程是否连启动都没成功（`thread::Builder::spawn` 失败）。
    ///
    /// 注意它只能反映**线程创建**这一步。设备打不开、解码失败这类问题发生在线程
    /// 内部，会通过 [`AudioEvent::Failed`] 上报到界面，不走这里。
    pub fn spawn_failed(&self) -> bool {
        self.thread.is_none()
    }

    /// 当前播放电平（0.0 ~ 1.0 的一串格子，旧的在前）。
    pub fn levels(&self) -> Vec<f32> {
        self.levels.snapshot()
    }

    /// 当前这段声音的频谱（`bands` 个 0.0 ~ 1.0 的能量值）。
    ///
    /// FFT 在这里（主线程）算，不在音频线程：它是一次纯计算，放进音频线程
    /// 会拖住采样供给，表现出来就是爆音。
    pub fn spectrum(&self, bands: usize) -> Vec<f32> {
        self.levels.spectrum(bands)
    }

    /// 标记为「缓冲中」。UI 在发起下载前调用，让用户立刻看到反馈。
    pub fn mark_loading(&self) {
        self.shared.set_state(PlaybackState::Loading);
        self.shared.position_ms.store(0, Ordering::Relaxed);
    }

    /// 装载并播放本地音频文件。
    pub fn load(&self, path: PathBuf, start_at_ms: u64, expected_duration_ms: u64) {
        self.shared
            .duration_ms
            .store(expected_duration_ms, Ordering::Relaxed);
        self.send(AudioCmd::Load {
            path,
            start_at_ms,
            expected_duration_ms,
        });
    }

    pub fn toggle(&self) {
        self.send(AudioCmd::Toggle);
    }

    pub fn stop(&self) {
        self.send(AudioCmd::Stop);
    }

    pub fn seek_to(&self, position_ms: u64) {
        self.send(AudioCmd::SeekTo(position_ms));
    }

    pub fn seek_by(&self, delta_ms: i64) {
        self.send(AudioCmd::SeekBy(delta_ms));
    }

    pub fn set_volume(&self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        self.shared.set_volume(clamped);
        self.send(AudioCmd::SetVolume(clamped));
    }

    pub fn state(&self) -> PlaybackState {
        self.shared.state()
    }

    pub fn position_ms(&self) -> u64 {
        self.shared.position_ms()
    }

    pub fn duration_ms(&self) -> u64 {
        self.shared.duration_ms()
    }

    pub fn volume(&self) -> f32 {
        self.shared.volume()
    }

    /// 停止播放并回收音频线程。
    pub fn shutdown(&mut self) {
        // 线程已经退出时 send 会失败，这是预期情况，忽略即可
        let _ = self.command_tx.send(AudioCmd::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    fn send(&self, command: AudioCmd) {
        if self.command_tx.send(command).is_err() {
            tlog!(crate::logger::LEVEL_WARN, "音频线程已退出，命令被丢弃");
        }
    }
}

impl Drop for AudioHandle {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.shutdown();
        }
    }
}

// ============================================================================
// 音频线程
// ============================================================================

fn run(rx: Receiver<AudioCmd>, bus: EventBus, shared: Arc<Shared>, levels: AudioLevels) {
    // 设备必须在音频线程里创建：cpal 的 Stream 不是 Send
    let mut stream = match rodio::DeviceSinkBuilder::open_default_sink() {
        Ok(stream) => stream,
        Err(error) => {
            let message = format!(
                "无法打开音频输出设备：{error}。请确认系统音频服务正常（Linux 下检查 PipeWire/ALSA）。"
            );
            tlog!(crate::logger::LEVEL_ERROR, "{message}");
            shared.set_state(PlaybackState::Stopped);
            bus.send(Event::Audio(AudioEvent::Failed(message)));
            // 设备不可用时仍要消费命令，否则发送端会积压
            drain_until_shutdown(rx);
            return;
        }
    };

    // rodio 默认会在 DeviceSink 析构时往 stdout 打一行 "Dropping DeviceSink..."，
    // 那行字会直接糊在 TUI 界面上，必须关掉。
    stream.log_on_drop(false);

    let player = rodio::Player::connect_new(stream.mixer());
    player.set_volume(shared.volume());

    let mut runtime = Runtime {
        player,
        shared,
        levels,
        bus,
        loaded: false,
        finished_reported: true,
    };
    runtime.run_loop(rx);

    // `stream` 在这里才 drop —— 必须活到播放结束，否则声音会立刻中断
    drop(stream);
}

/// 设备不可用时，把命令读干净直到收到 Shutdown，避免通道无界增长。
fn drain_until_shutdown(rx: Receiver<AudioCmd>) {
    for command in rx.iter() {
        if matches!(command, AudioCmd::Shutdown) {
            break;
        }
    }
}

struct Runtime {
    player: rodio::Player,
    shared: Arc<Shared>,
    /// 电平采集。包在解码器外面，采样透传的同时记下峰值。
    levels: AudioLevels,
    bus: EventBus,
    /// 是否已经装载了音源。
    loaded: bool,
    /// 本曲是否已上报过结束，防止同一首歌反复触发切歌。
    finished_reported: bool,
}

impl Runtime {
    fn run_loop(&mut self, rx: Receiver<AudioCmd>) {
        loop {
            match rx.recv_timeout(POLL_INTERVAL) {
                Ok(AudioCmd::Shutdown) => break,
                Ok(command) => self.handle(command),
                // 没有命令时也走一遍同步，让进度条持续前进
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.sync();
        }

        self.player.stop();
        self.shared.set_state(PlaybackState::Stopped);
    }

    fn handle(&mut self, command: AudioCmd) {
        match command {
            AudioCmd::Load {
                path,
                start_at_ms,
                expected_duration_ms,
            } => self.load(&path, start_at_ms, expected_duration_ms),
            AudioCmd::Toggle => {
                if self.player.is_paused() {
                    self.resume();
                } else {
                    self.pause();
                }
            }
            AudioCmd::Stop => self.stop(),
            AudioCmd::SeekTo(position_ms) => self.seek_to(position_ms),
            AudioCmd::SeekBy(delta_ms) => {
                let current = self.shared.position_ms() as i64;
                let target = current.saturating_add(delta_ms).max(0) as u64;
                self.seek_to(target);
            }
            AudioCmd::SetVolume(volume) => {
                self.player.set_volume(volume);
                self.shared.set_volume(volume);
            }
            // 在 run_loop 里已处理
            AudioCmd::Shutdown => {}
        }
    }

    fn load(&mut self, path: &Path, start_at_ms: u64, expected_duration_ms: u64) {
        self.player.stop();
        self.player.clear();
        self.loaded = false;
        // 装载期间先屏蔽「结束」上报，避免旧的 empty 状态误触发切歌
        self.finished_reported = true;

        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) => {
                self.report_failure(format!("打开音频文件 {} 失败：{error}", path.display()));
                return;
            }
        };

        let decoder = match rodio::Decoder::try_from(file) {
            Ok(decoder) => decoder,
            Err(error) => {
                self.report_failure(format!(
                    "解码 {} 失败：{error}。该文件可能不是有效音频，或格式不受支持。",
                    path.display()
                ));
                return;
            }
        };

        // 解码器报出的时长最准；拿不到就用列表里的时长兜底，保证进度条可用
        let duration_ms = decoder
            .total_duration()
            .map(|duration| duration.as_millis() as u64)
            .filter(|millis| *millis > 0)
            .unwrap_or(expected_duration_ms);

        self.shared
            .duration_ms
            .store(duration_ms, Ordering::Relaxed);
        self.shared
            .position_ms
            .store(start_at_ms, Ordering::Relaxed);

        // 包一层电平采集：采样原样透传给播放器，顺带把峰值记进环形缓冲
        self.player
            .append(LevelMeter::new(decoder, self.levels.clone()));
        self.player.play();

        if start_at_ms > 0 {
            if let Err(error) = self.player.try_seek(Duration::from_millis(start_at_ms)) {
                tlog!(
                    crate::logger::LEVEL_WARN,
                    "跳转到 {start_at_ms}ms 失败：{error}"
                );
            }
        }

        self.loaded = true;
        self.finished_reported = false;
        self.shared.set_state(PlaybackState::Playing);
        self.bus
            .send(Event::Audio(AudioEvent::Ready { duration_ms }));
    }

    fn pause(&mut self) {
        if !self.loaded {
            return;
        }
        self.player.pause();
        self.shared.set_state(PlaybackState::Paused);
    }

    fn resume(&mut self) {
        if !self.loaded {
            return;
        }
        self.player.play();
        self.shared.set_state(PlaybackState::Playing);
        // 恢复播放后允许再次上报结束
        self.finished_reported = false;
    }

    fn stop(&mut self) {
        self.player.stop();
        self.player.clear();
        // 清掉残留的柱子，否则会定格在最后一帧，看着像卡住了
        self.levels.clear();
        self.loaded = false;
        self.finished_reported = true;
        self.shared.position_ms.store(0, Ordering::Relaxed);
        self.shared.duration_ms.store(0, Ordering::Relaxed);
        self.shared.set_state(PlaybackState::Stopped);
    }

    fn seek_to(&mut self, position_ms: u64) {
        if !self.loaded {
            return;
        }

        let duration_ms = self.shared.duration_ms();
        let target = if duration_ms > 0 {
            position_ms.min(duration_ms.saturating_sub(1))
        } else {
            position_ms
        };

        match self.player.try_seek(Duration::from_millis(target)) {
            Ok(()) => {
                self.shared.position_ms.store(target, Ordering::Relaxed);
                // 跳转后重新允许上报结束
                self.finished_reported = false;
            }
            Err(error) => tlog!(crate::logger::LEVEL_WARN, "跳转到 {target}ms 失败：{error}"),
        }
    }

    /// 把播放器的真实状态同步到共享快照，并检测曲目结束。
    fn sync(&mut self) {
        if !self.loaded {
            return;
        }

        self.shared
            .position_ms
            .store(self.player.get_pos().as_millis() as u64, Ordering::Relaxed);

        let paused = self.player.is_paused();
        let drained = self.player.empty();

        if drained && !paused {
            if !self.finished_reported {
                self.finished_reported = true;
                self.loaded = false;
                self.shared.set_state(PlaybackState::Stopped);
                self.bus.send(Event::Audio(AudioEvent::TrackFinished));
            }
            return;
        }

        let state = if paused {
            PlaybackState::Paused
        } else {
            PlaybackState::Playing
        };
        if self.shared.state() != state {
            self.shared.set_state(state);
        }
    }

    /// 上报失败。
    ///
    /// 刻意**不**触发「播放结束」流程：否则一首坏文件会连锁触发自动切歌，
    /// 坏歌连成片时会瞬间刷掉整个队列。让用户看到错误后自己决定下一步。
    fn report_failure(&mut self, message: String) {
        tlog!(crate::logger::LEVEL_ERROR, "{message}");
        self.loaded = false;
        self.finished_reported = true;
        self.shared.set_state(PlaybackState::Stopped);
        self.bus.send(Event::Audio(AudioEvent::Failed(message)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_state_code_round_trips() {
        for state in [
            PlaybackState::Stopped,
            PlaybackState::Loading,
            PlaybackState::Playing,
            PlaybackState::Paused,
        ] {
            assert_eq!(PlaybackState::from_code(state.code()), state);
        }
    }

    #[test]
    fn unknown_code_falls_back_to_stopped() {
        assert_eq!(PlaybackState::from_code(200), PlaybackState::Stopped);
    }

    #[test]
    fn shared_clamps_volume() {
        let shared = Shared::new(2.0);
        assert!((shared.volume() - 1.0).abs() < f32::EPSILON);

        shared.set_volume(-1.0);
        assert!(shared.volume().abs() < f32::EPSILON);
    }
}
