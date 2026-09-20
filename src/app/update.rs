//! 事件 → 状态变更。
//!
//! 这是整个程序的「大脑」：所有输入（按键、异步结果、音频事件、定时心跳）都在
//! 这里被翻译成对 [`AppState`] 的修改。它**不**渲染、不阻塞、不直接做 IO——
//! 需要 IO 时一律 `runtime.spawn` 一个任务，结果通过事件总线回来。
//!
//! 这条纪律带来的直接好处：`handle_*` 全是纯同步函数，可以逐个单元测试，
//! 也不会因为某个网络请求卡住而冻结界面。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use ratatui::crossterm::event::MouseEvent;

use crate::api::catalog::PAGE_LIMIT;
use crate::api::cloud::QrStatus;
use crate::api::model::{Artist, Playlist, RankBoard, Song};
use crate::app::App;
use crate::app::state::{
    ConfirmAction, Focus, HitTarget, HitZone, LoginState, PromptAction, PromptState, Tab,
    move_selection, select_first, select_last,
};
use crate::config::Config;

use crate::audio::download::Downloader;
use crate::audio::engine::{AudioEvent, PlaybackState, SEEK_STEP_MS, VOLUME_STEP};
use crate::event::{Event, Loaded, PlaylistSource};
use crate::keymap::{Action, KeyMode};
use crate::logger::tlog;

/// 翻页时跳过的行数。
const PAGE_STEP: isize = 10;

/// 下载进度上报的字节间隔。太小会让事件通道被高频消息淹没。
const PROGRESS_STEP_BYTES: u64 = 256 * 1024;

/// 「上一首」在播放超过这个时长后，先回到本曲开头而不是切歌。
const RESTART_THRESHOLD_MS: u64 = 3_000;

/// 每帧最多处理的事件数，防止事件洪水导致画面完全停止刷新。
const MAX_EVENTS_PER_FRAME: usize = 64;

/// 每隔多少拍测量一次缓存占用。
///
/// 默认 200ms 一拍，50 拍约 10 秒。目录扫描放在阻塞线程池里，
/// 因此这个频率不会影响界面流畅度。
const CACHE_MEASURE_TICKS: u64 = 50;

impl App {
    // ==================================================================
    // 事件分发
    // ==================================================================

    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) => {
                let mode = if self.state.is_editing() {
                    KeyMode::TextInput
                } else {
                    KeyMode::Normal
                };
                let action = crate::keymap::resolve(key, mode);
                // 排查「按键没反应」时这条日志是决定性的：它能区分
                // 「按键根本没到程序」和「到了但映射成了 None」。
                // 默认不输出，用 KUGOU_TUI_DEBUG=1 开启。
                tlog!(
                    crate::logger::LEVEL_DEBUG,
                    "按键 {:?}（模式 {mode:?}）→ {action:?}",
                    key
                );
                self.handle_action(action);
            }
            // 布局每帧按终端实际尺寸重算，无需额外处理
            Event::Resize => {}
            Event::Audio(audio) => self.handle_audio_event(audio),
            Event::Loaded(loaded) => self.handle_loaded(*loaded),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::Tick => self.tick(),
            // 来自 MPRIS（桌面媒体控件）的语义动作，已经是明确意图，直接执行
            Event::Action(action) => self.handle_action(action),
        }
    }

    // ==================================================================
    // 鼠标
    // ==================================================================

    /// 鼠标事件：点击选中、双击激活、滚轮移动选中、点击进度条跳转。
    ///
    /// 坐标→行的换算依赖 ui 层每帧回填的命中区（见 `AppState::hit_test`）。
    /// 没有命中任何区域时直接忽略，不做「就近猜测」。
    fn handle_mouse(&mut self, mouse: MouseEvent) {
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};

        match mouse.kind {
            // 悬停反馈：这里只记位置，真正的命中判断交给渲染层（只有它知道行几何）
            MouseEventKind::Moved => {
                self.state.hover = Some((mouse.column, mouse.row));
            }
            MouseEventKind::ScrollDown => self.scroll_list_at(&mouse, 1),
            MouseEventKind::ScrollUp => self.scroll_list_at(&mouse, -1),
            MouseEventKind::Down(MouseButton::Left) => self.click_at(&mouse),
            _ => {}
        }
    }

    /// 滚轮：在光标所在的列表上移动选中。
    fn scroll_list_at(&mut self, mouse: &MouseEvent, delta: isize) {
        let Some(zone) = self.state.hit_test(mouse.column, mouse.row) else {
            return;
        };
        // 进度条和侧边栏不参与列表滚动
        if !matches!(
            zone.target,
            HitTarget::Entries | HitTarget::Songs | HitTarget::Queue
        ) {
            return;
        }
        self.focus_hit_target(zone.target);
        self.move_selection(delta * 3);
    }

    /// 左键点击。
    fn click_at(&mut self, mouse: &MouseEvent) {
        let Some(zone) = self.state.hit_test(mouse.column, mouse.row) else {
            return;
        };

        match zone.target {
            HitTarget::Tab(index) => {
                if let Some(tab) = Tab::from_number(index as u8 + 1) {
                    self.switch_tab(tab);
                }
            }
            HitTarget::Entries | HitTarget::Songs | HitTarget::Queue => {
                self.click_list_row(zone, mouse)
            }
            HitTarget::Progress => self.click_progress(zone, mouse),
        }
    }

    /// 点击列表行：选中，双击则激活（等同 Enter）。
    fn click_list_row(&mut self, zone: HitZone, mouse: &MouseEvent) {
        let Some(index) = zone.index_at(mouse.row) else {
            return;
        };

        // 双击判定要在覆盖 last_click 之前做
        let double = self.state.is_double_click(zone.target, Some(index));

        match zone.target {
            HitTarget::Entries => self.state.select_entry_index(index),
            HitTarget::Songs => self.state.select_song_index(index),
            HitTarget::Queue => {
                self.focus_hit_target(zone.target);
                self.state.queue_cursor.select(Some(index));
            }
            HitTarget::Tab(_) | HitTarget::Progress => return,
        }

        self.focus_hit_target(zone.target);
        self.state.set_last_click(zone.target, Some(index));

        if double {
            self.activate();
        }
    }

    /// 点击进度条：按 x 比例跳转到对应时间。
    fn click_progress(&mut self, zone: HitZone, mouse: &MouseEvent) {
        let duration_ms = self.state.duration_ms;
        if duration_ms == 0 || zone.rect.width == 0 {
            return;
        }

        let ratio =
            f64::from(mouse.column.saturating_sub(zone.rect.left())) / f64::from(zone.rect.width);
        let target = (ratio * duration_ms as f64) as u64;
        let target = target.min(duration_ms.saturating_sub(1));
        self.audio.seek_to(target);
        // 乐观更新，让进度条立刻响应
        self.state.position_ms = target;
    }

    /// 把焦点切到命中区对应的面板。离开搜索框时退出输入态，否则字母键会被吞掉。
    fn focus_hit_target(&mut self, target: HitTarget) {
        let focus = match target {
            HitTarget::Entries => Focus::Primary,
            HitTarget::Songs => Focus::Secondary,
            HitTarget::Queue => Focus::Queue,
            HitTarget::Tab(_) | HitTarget::Progress => return,
        };

        if self.state.focus == Focus::Primary && focus != Focus::Primary {
            self.state.search.editing = false;
        }
        self.state.focus = focus;
    }

    /// 处理一个按键动作。
    pub fn handle_action(&mut self, action: Action) {
        // 帮助面板是模态的：只允许关闭它
        if self.state.show_help {
            if matches!(
                action,
                Action::Help | Action::Cancel | Action::Quit | Action::ForceQuit
            ) {
                if !matches!(action, Action::Quit | Action::ForceQuit) {
                    self.state.show_help = false;
                } else {
                    self.state.should_quit = true;
                    self.state.force_quit = matches!(action, Action::ForceQuit);
                }
            }
            return;
        }

        // 确认对话框优先于一切：有未决确认时其它按键一律拦截。
        if let Some(confirm) = self.state.pending_confirm {
            match action {
                Action::Submit => {
                    self.state.pending_confirm = None;
                    match confirm {
                        ConfirmAction::ClearQueue => self.clear_queue(),
                        ConfirmAction::DeleteCloudPlaylist => self.delete_cloud_playlist(),
                        ConfirmAction::ClearCache => self.clear_cache(),
                    }
                }
                Action::Cancel | Action::Char('n') | Action::Char('N') => {
                    self.state.pending_confirm = None;
                    self.state.info("已取消");
                }
                _ => {}
            }
            return;
        }

        // 文本输入弹窗（新建歌单）：拦截所有按键，只处理文本编辑、提交与取消。
        if let Some(prompt) = self.state.prompt.as_ref() {
            let action_kind = prompt.action;
            match action {
                Action::Char(character) => {
                    // 上面已确认 prompt 存在，但这里不用 expect：契约一旦变化
                    // 就是整个程序 panic，不划算。取不到就当作没按这个键。
                    if let Some(prompt) = self.state.prompt.as_mut() {
                        prompt.buffer.push(character);
                    }
                }
                Action::Backspace => {
                    if let Some(prompt) = self.state.prompt.as_mut() {
                        prompt.buffer.pop();
                    }
                }
                Action::Submit => {
                    let Some(prompt) = self.state.prompt.take() else {
                        return;
                    };
                    let name = prompt.buffer.trim().to_string();
                    match action_kind {
                        PromptAction::CreateCloudPlaylist => self.create_cloud_playlist(name),
                    }
                }
                Action::Cancel => {
                    self.state.prompt = None;
                    self.state.info("已取消");
                }
                Action::Quit => self.state.should_quit = true,
                Action::ForceQuit => {
                    self.state.should_quit = true;
                    self.state.force_quit = true;
                }
                _ => {}
            }
            return;
        }

        // 登录弹窗是模态的：只放行取消与退出，避免二维码显示时误触其它功能。
        if self.state.login.is_some() {
            match action {
                Action::Cancel => {
                    self.state.login = None;
                    self.state.info("已取消登录");
                }
                Action::Quit => self.state.should_quit = true,
                Action::ForceQuit => {
                    self.state.should_quit = true;
                    self.state.force_quit = true;
                }
                _ => {}
            }
            return;
        }

        if self.state.is_editing() && self.handle_editing_action(action) {
            return;
        }
        self.handle_normal_action(action);
    }

    /// 输入态按键。返回 `true` 表示已消费。
    fn handle_editing_action(&mut self, action: Action) -> bool {
        let input = &mut self.state.search.input;
        match action {
            Action::Char(character) => input.insert(character),
            Action::Backspace => input.backspace(),
            Action::Delete => input.delete(),
            Action::CursorLeft => input.move_left(),
            Action::CursorRight => input.move_right(),
            Action::CursorHome => input.move_home(),
            Action::CursorEnd => input.move_end(),
            Action::Submit => self.run_search(),
            // Esc 退出输入态但保留已输入内容，再按一次 Enter 可继续编辑
            Action::Cancel => self.state.search.editing = false,
            _ => return false,
        }
        true
    }

    fn handle_normal_action(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Quit => self.state.should_quit = true,
            Action::ForceQuit => {
                self.state.should_quit = true;
                self.state.force_quit = true;
            }
            Action::Help => self.state.show_help = true,
            Action::SwitchSource => self.switch_source(),
            Action::ToggleSidebar => {
                self.state.sidebar_visible = !self.state.sidebar_visible;
            }
            Action::SwitchTab(number) => {
                if let Some(tab) = Tab::from_number(number) {
                    self.switch_tab(tab);
                }
            }
            Action::FocusNext => self.state.cycle_focus(true),
            Action::FocusPrev => self.state.cycle_focus(false),

            Action::MoveDown => self.move_selection(1),
            Action::MoveUp => self.move_selection(-1),
            Action::MoveTop => self.move_selection_edge(true),
            Action::MoveBottom => self.move_selection_edge(false),
            Action::PageDown => self.move_selection(PAGE_STEP),
            Action::PageUp => self.move_selection(-PAGE_STEP),

            Action::Submit => self.activate(),
            Action::Cancel => self.go_back(),

            Action::PlayPause => self.toggle_playback(),
            Action::Next => self.next_track(true),
            Action::Prev => self.previous_track(),
            Action::SeekForward => self.seek_by(SEEK_STEP_MS),
            Action::SeekTo(position_ms) => self.seek_to(position_ms),
            Action::LoadMoreSearch => self.load_more_search(),
            Action::SeekBackward => self.seek_by(-SEEK_STEP_MS),
            Action::VolumeUp => self.adjust_volume(VOLUME_STEP),
            Action::VolumeDown => self.adjust_volume(-VOLUME_STEP),
            Action::ToggleMute => self.toggle_mute(),
            Action::CyclePlaybackMode => {
                let mode = self.state.queue.cycle_mode();
                self.state.config.playback_mode = mode;
                self.state.info(format!("播放模式：{}", mode.label()));
            }
            Action::ToggleLyricPanel => {
                self.state.show_lyric_panel = !self.state.show_lyric_panel;
            }
            Action::LyricDelay => self.adjust_lyric_offset(-100),
            Action::LyricAdvance => self.adjust_lyric_offset(100),

            Action::OpenSearch => {
                self.switch_tab(Tab::Search);
                self.state.focus = Focus::Primary;
                self.state.search.editing = true;
            }
            Action::Reload => self.reload_current_tab(),
            Action::QueueAppend => self.queue_focused_song(false),
            Action::AddAllToQueue => self.queue_all_songs(),
            Action::QueuePlayNext => self.queue_focused_song(true),
            Action::RemoveFromQueue => self.remove_selected_from_queue(),
            // 清空队列是破坏性操作，先进确认流程，避免误按一下就把整个队列清掉。
            Action::ClearQueue => {
                self.state.pending_confirm = Some(ConfirmAction::ClearQueue);
            }
            // 删文件不可恢复，同样先确认
            Action::ClearCache => {
                self.state.pending_confirm = Some(ConfirmAction::ClearCache);
            }
            Action::ToggleSortOrder => self.toggle_sort_order(),
            Action::OpenRanks => self.switch_tab(Tab::Ranks),
            Action::OpenCloud => self.switch_tab(Tab::Cloud),
            Action::Login => self.start_login(),
            Action::CycleArtistFilter => self.cycle_artist_filter(),
            Action::SyncToCloud => self.sync_queue_to_cloud(),
            Action::AddToCloud => self.add_focused_song_to_cloud(),
            Action::RemoveFromCloud => self.remove_focused_song_from_cloud(),
            // 删除歌单是破坏性操作，先进确认流程
            Action::DeleteCloudPlaylist => {
                if self.state.sync_target.is_none() {
                    self.state.warn("请先在「云端」标签页选中一个歌单");
                    return;
                }
                self.state.pending_confirm = Some(ConfirmAction::DeleteCloudPlaylist);
            }
            Action::NewCloudPlaylist => {
                self.state.prompt = Some(PromptState::new(
                    "新建云端歌单",
                    PromptAction::CreateCloudPlaylist,
                ));
                self.state.info("输入歌单名称，Enter 创建 · Esc 取消");
            }

            // 这些动作只在输入态有意义，浏览态下忽略
            Action::Char(_)
            | Action::Backspace
            | Action::Delete
            | Action::CursorLeft
            | Action::CursorRight
            | Action::CursorHome
            | Action::CursorEnd => {}
        }
    }

    // ==================================================================
    // 导航
    // ==================================================================

    pub fn switch_tab(&mut self, tab: Tab) {
        self.switch_tab_inner(tab, true);
    }

    /// 切到指定标签页。
    ///
    /// `move_focus` 为 `false` 时**保留当前焦点**。用方向键在侧边栏里连续上下走时
    /// 必须这样：若每次移动都把焦点甩回主区，用户按第二下就没反应了。
    fn switch_tab_inner(&mut self, tab: Tab, move_focus: bool) {
        self.state.tab = tab;
        self.state.search.editing = false;
        if move_focus {
            self.state.focus = Focus::Primary;
        }
        self.ensure_tab_loaded(tab);
    }

    /// 侧边栏里上下移动：切换标签，焦点留在侧边栏。
    fn move_sidebar(&mut self, delta: isize) {
        let current = Tab::ALL
            .iter()
            .position(|tab| *tab == self.state.tab)
            .unwrap_or(0);
        let next = (current as isize + delta).clamp(0, Tab::ALL.len() as isize - 1) as usize;
        let tab = Tab::ALL[next];

        if tab != self.state.tab {
            self.switch_tab_inner(tab, false);
        }
    }

    /// 首次进入某个标签页时惰性加载数据。
    ///
    /// 加载失败时列表保持为空、`loading` 复位，于是下次再切回来会重试——
    /// 对「服务刚启动完还没就绪」这类瞬时故障很友好。
    pub fn ensure_tab_loaded(&mut self, tab: Tab) {
        match tab {
            Tab::Playlists
                if self.state.playlists.list.is_empty() && !self.state.playlists.list.loading =>
            {
                self.load_plaza_playlists();
            }
            Tab::Artists
                if self.state.artists.list.is_empty() && !self.state.artists.list.loading =>
            {
                self.load_artists();
            }
            Tab::Ranks if self.state.ranks.list.is_empty() && !self.state.ranks.list.loading => {
                self.load_ranks();
            }
            Tab::Cloud if self.state.cloud.list.is_empty() && !self.state.cloud.list.loading => {
                self.load_cloud_playlists();
            }
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        match (self.state.tab, self.state.focus) {
            // 侧边栏里上下移动 = 切换标签页（侧边栏的高亮就是当前标签）
            (_, Focus::Sidebar) => self.move_sidebar(delta),
            (_, Focus::Queue) => {
                move_selection(&mut self.state.queue_cursor, self.state.queue.len(), delta);
            }
            (Tab::Search, Focus::Primary | Focus::Secondary) => {
                self.state.search.results.move_by(delta);
            }
            (Tab::Playlists, Focus::Primary) => self.state.playlists.list.move_by(delta),
            (Tab::Playlists, Focus::Secondary) => self.state.playlists.songs.move_by(delta),
            (Tab::Artists, Focus::Primary) => self.state.artists.list.move_by(delta),
            (Tab::Artists, Focus::Secondary) => self.state.artists.songs.move_by(delta),
            (Tab::Ranks, Focus::Primary) => self.state.ranks.list.move_by(delta),
            (Tab::Ranks, Focus::Secondary) => self.state.ranks.songs.move_by(delta),
            (Tab::Cloud, Focus::Primary) => self.state.cloud.list.move_by(delta),
            (Tab::Cloud, Focus::Secondary) => self.state.cloud.songs.move_by(delta),
            // 可视化页没有列表。必须放在最后：否则会抢在 Sidebar / Queue 之前，
            // 导致这一页连侧边栏切换标签都不响应（实测踩过）。放在最后时，焦点在
            // 侧边栏或队列仍走上面的分支；焦点在主区则退化为切换标签页，避免
            // 上下键完全无响应。
            (Tab::Visualizer, _) => self.move_sidebar(delta),
        }
    }
    fn move_selection_edge(&mut self, to_first: bool) {
        match (self.state.tab, self.state.focus) {
            // 侧边栏：跳到第一个 / 最后一个标签
            (_, Focus::Sidebar) => {
                let target = if to_first {
                    Tab::ALL[0]
                } else {
                    Tab::ALL[Tab::ALL.len() - 1]
                };
                if target != self.state.tab {
                    self.switch_tab_inner(target, false);
                }
            }
            (_, Focus::Queue) => {
                if to_first {
                    select_first(&mut self.state.queue_cursor, self.state.queue.len());
                } else {
                    select_last(&mut self.state.queue_cursor, self.state.queue.len());
                }
            }
            (Tab::Search, _) => {
                if to_first {
                    self.state.search.results.select_first();
                } else {
                    self.state.search.results.select_last();
                }
            }
            (Tab::Playlists, Focus::Primary) => {
                if to_first {
                    self.state.playlists.list.select_first();
                } else {
                    self.state.playlists.list.select_last();
                }
            }
            (Tab::Playlists, Focus::Secondary) => {
                if to_first {
                    self.state.playlists.songs.select_first();
                } else {
                    self.state.playlists.songs.select_last();
                }
            }
            (Tab::Artists, Focus::Primary) => {
                if to_first {
                    self.state.artists.list.select_first();
                } else {
                    self.state.artists.list.select_last();
                }
            }
            (Tab::Artists, Focus::Secondary) => {
                if to_first {
                    self.state.artists.songs.select_first();
                } else {
                    self.state.artists.songs.select_last();
                }
            }
            (Tab::Ranks, Focus::Primary) => {
                if to_first {
                    self.state.ranks.list.select_first();
                } else {
                    self.state.ranks.list.select_last();
                }
            }
            (Tab::Ranks, Focus::Secondary) => {
                if to_first {
                    self.state.ranks.songs.select_first();
                } else {
                    self.state.ranks.songs.select_last();
                }
            }
            (Tab::Cloud, Focus::Primary) => {
                if to_first {
                    self.state.cloud.list.select_first();
                } else {
                    self.state.cloud.list.select_last();
                }
            }
            (Tab::Cloud, Focus::Secondary) => {
                if to_first {
                    self.state.cloud.songs.select_first();
                } else {
                    self.state.cloud.songs.select_last();
                }
            }
            // 同 move_selection：必须放在最后，否则会抢在 Sidebar / Queue 之前。
            (Tab::Visualizer, _) => {
                let target = if to_first {
                    Tab::ALL[0]
                } else {
                    Tab::ALL[Tab::ALL.len() - 1]
                };
                if target != self.state.tab {
                    self.switch_tab_inner(target, false);
                }
            }
        }
    }

    /// Enter：进入下一层，或播放选中歌曲。
    fn activate(&mut self) {
        match (self.state.tab, self.state.focus) {
            // 可视化页没有列表，方向键与 Enter 在它上面没有意义
            (Tab::Visualizer, _) => {}
            // 搜索框还没进入编辑态时，Enter 先聚焦输入框
            (Tab::Search, Focus::Primary) => {
                if self.state.search.editing {
                    self.run_search();
                } else if self.state.search.input.is_empty() {
                    self.state.search.editing = true;
                } else {
                    self.run_search();
                }
            }
            (Tab::Search, Focus::Secondary) => self.play_from_focused_songs(),
            (Tab::Playlists, Focus::Primary) => self.open_selected_playlist(),
            (Tab::Playlists, Focus::Secondary) => self.play_from_focused_songs(),
            (Tab::Artists, Focus::Primary) => self.open_selected_artist(),
            (Tab::Artists, Focus::Secondary) => self.play_from_focused_songs(),
            (Tab::Ranks, Focus::Primary) => self.open_selected_rank(),
            (Tab::Ranks, Focus::Secondary) => self.play_from_focused_songs(),
            (Tab::Cloud, Focus::Primary) => self.open_selected_cloud_playlist(),
            (Tab::Cloud, Focus::Secondary) => self.play_from_focused_songs(),
            (_, Focus::Queue) => self.play_from_queue(),
            (_, Focus::Sidebar) => self.state.focus = Focus::Primary,
        }
    }

    /// Esc：回到上一层。
    fn go_back(&mut self) {
        match self.state.focus {
            Focus::Queue => self.state.focus = Focus::Secondary,
            // 歌曲列表是两层浏览的第二层，退回条目列表（搜索页也一样：
            // Secondary 是结果列表，Primary 是输入框）
            Focus::Secondary | Focus::Primary | Focus::Sidebar => {
                self.state.focus = Focus::Primary;
                self.state.sidebar_visible = true;
            }
        }
    }

    // ==================================================================
    // 播放控制
    // ==================================================================

    /// 用一批歌曲替换播放队列并起播第 `index` 首。
    pub fn play_from(&mut self, songs: Vec<Song>, index: usize) {
        let Some(song) = self.state.queue.replace_with(songs, index).cloned() else {
            self.state.warn("列表为空，无法播放");
            return;
        };
        self.sync_queue_cursor(index);
        self.start_playback(song, 0);
    }

    fn play_from_focused_songs(&mut self) {
        let selection = self.state.focused_songs().and_then(|songs| {
            songs
                .selected_index()
                .map(|index| (songs.songs.clone(), index))
        });

        match selection {
            Some((songs, index)) => self.play_from(songs, index),
            None => self.state.warn("当前没有可播放的歌曲"),
        }
    }

    fn play_from_queue(&mut self) {
        let Some(index) = self.state.queue_cursor.selected() else {
            self.state.warn("播放队列为空");
            return;
        };
        let Some(song) = self.state.queue.jump_to(index).cloned() else {
            self.state.warn("播放队列为空");
            return;
        };
        self.start_playback(song, 0);
    }

    fn toggle_playback(&mut self) {
        match self.state.playback {
            // 正在缓冲时忽略，避免连按导致状态错乱
            PlaybackState::Loading => {}
            PlaybackState::Playing | PlaybackState::Paused => self.audio.toggle(),
            PlaybackState::Stopped => {
                if let Some(song) = self.state.queue.current().cloned() {
                    self.start_playback(song, 0);
                } else {
                    self.play_from_focused_songs();
                }
            }
        }
    }

    fn next_track(&mut self, triggered_by_user: bool) {
        let Some(index) = self.state.queue.advance(triggered_by_user) else {
            // 顺序播放到底：停止而不是循环
            self.audio.stop();
            self.state.playback = PlaybackState::Stopped;
            self.state.position_ms = 0;
            self.state.info("播放队列已到末尾");
            return;
        };
        self.sync_queue_cursor(index);
        if let Some(song) = self.state.queue.current().cloned() {
            self.start_playback(song, 0);
        }
    }

    fn previous_track(&mut self) {
        // 播放超过 3 秒时先回到本曲开头，这是主流播放器的通用行为
        if self.state.position_ms > RESTART_THRESHOLD_MS {
            self.audio.seek_to(0);
            self.state.position_ms = 0;
            return;
        }

        let Some(index) = self.state.queue.retreat() else {
            self.state.warn("播放队列为空");
            return;
        };
        self.sync_queue_cursor(index);
        if let Some(song) = self.state.queue.current().cloned() {
            self.start_playback(song, 0);
        }
    }

    /// 把当前播放信息推给 MPRIS，供桌面组件显示。
    ///
    /// 每次 tick 调一次。开销就是一次互斥锁写入，可忽略；桌面组件的轮询
    /// 频率远低于此，没必要更高频。
    fn sync_mpris(&mut self) {
        let Some(handle) = self.mpris.as_ref() else {
            return;
        };

        let info = match self.state.current.as_ref() {
            Some(song) => crate::mpris::TrackInfo {
                title: song.name.clone(),
                artists: song
                    .singers
                    .iter()
                    .map(|singer| singer.name.clone())
                    .collect(),
                album: song.album_name.clone(),
                art_url: song.cover.clone(),
                position_us: (self.state.position_ms as i64) * 1_000,
                duration_us: (self.state.duration_ms as i64) * 1_000,
                status: self.state.playback,
            },
            None => crate::mpris::TrackInfo {
                status: self.state.playback,
                ..Default::default()
            },
        };

        handle.update(info);
    }

    /// 绝对定位到 `position_ms`。
    ///
    /// 与 [`Self::seek_by`] 的区别：那个是相对步进，这个是"跳到某处"。
    /// MPRIS 的 SetPosition（桌面组件拖进度条）需要后者。
    fn seek_to(&mut self, position_ms: u64) {
        if self.state.current.is_none() {
            return;
        }
        // 夹在时长范围内，避免拖到尽头后位置越界
        let target = if self.state.duration_ms > 0 {
            position_ms.min(self.state.duration_ms.saturating_sub(1))
        } else {
            position_ms
        };

        self.audio.seek_to(target);
        // 乐观更新，让进度条立刻响应；音频线程随后给出真实位置
        self.state.position_ms = target;
    }

    /// 搜索结果「加载更多」：追加下一页。
    ///
    /// 之所以分页而不是一次取全：酷狗搜索只有第 1 页是精确匹配，
    /// 深页塞的是兜底内容（实测搜「黑色幽默」第 2 页起变成有声书）。
    /// 全量合并会把相关结果淹没，所以让用户主动一页页要看。
    fn load_more_search(&mut self) {
        let keyword = self.state.search.submitted.clone();
        if keyword.is_empty() {
            self.state.warn("请先搜索");
            return;
        }
        if self.state.search.results.songs.is_empty() {
            self.state.warn("当前没有搜索结果");
            return;
        }

        let next_page = self.state.search.page + 1;
        // 实测上限 16 页，再往后会返回 code=149 Out Page Range
        if next_page > 16 {
            self.state.info("已经到最后一页了");
            return;
        }

        let api = self.api.clone();
        let bus = self.bus.clone();
        let page_size = self.state.config.page_size;
        self.state.busy = Some(format!("加载「{keyword}」第 {next_page} 页"));

        self.state.search.page = next_page;

        self.runtime.spawn(async move {
            match api.search_songs(&keyword, next_page, page_size).await {
                Ok(songs) => bus.emit(Loaded::Search {
                    keyword,
                    songs,
                    append: true,
                }),
                Err(error) => {
                    // 越界就是没有更多了，不是故障
                    if error.is_page_out_of_range() {
                        bus.emit(Loaded::Search {
                            keyword,
                            songs: Vec::new(),
                            append: true,
                        });
                    } else {
                        bus.fail(format!("加载「{keyword}」更多结果失败"), error);
                    }
                }
            }
        });
    }

    fn seek_by(&mut self, delta_ms: i64) {
        if self.state.current.is_none() {
            return;
        }
        self.audio.seek_by(delta_ms);
        // 乐观更新，让进度条立刻响应；音频线程随后会给出真实位置
        let next = (self.state.position_ms as i64 + delta_ms).max(0) as u64;
        self.state.position_ms = match self.state.duration_ms {
            0 => next,
            duration => next.min(duration),
        };
    }

    fn adjust_volume(&mut self, delta: f32) {
        // 静音状态下按音量键，先解除静音再调整
        let base = self
            .state
            .volume_before_mute
            .take()
            .unwrap_or(self.state.volume);
        let next = (base + delta).clamp(0.0, 1.0);

        self.state.volume = next;
        self.audio.set_volume(next);
        self.state.info(format!("音量 {:.0}%", next * 100.0));
    }

    fn toggle_mute(&mut self) {
        match self.state.volume_before_mute.take() {
            Some(previous) => {
                self.state.volume = previous;
                self.audio.set_volume(previous);
                self.state
                    .info(format!("取消静音，音量 {:.0}%", previous * 100.0));
            }
            None => {
                self.state.volume_before_mute = Some(self.state.volume);
                self.state.volume = 0.0;
                self.audio.set_volume(0.0);
                self.state.info("已静音");
            }
        }
    }

    fn adjust_lyric_offset(&mut self, delta_ms: i64) {
        let offset = (self.state.config.lyric_offset_ms + delta_ms).clamp(-10_000, 10_000);
        self.state.config.lyric_offset_ms = offset;
        self.state.info(format!("歌词偏移 {offset:+} ms"));
    }

    /// 起播一首歌：先查缓存，未命中则解析直链 → 下载 → 播放。
    fn start_playback(&mut self, song: Song, start_at_ms: u64) {
        self.state.current = Some(song.clone());
        self.state.duration_ms = song.duration_ms;
        self.state.position_ms = start_at_ms;
        self.state.download_progress = None;
        self.state.lyric = crate::app::state::LyricPane {
            hash: Some(song.hash.clone()),
            ..Default::default()
        };

        self.audio.mark_loading();
        self.state.playback = PlaybackState::Loading;

        // privilege 不含可播标记时提前提示，省得用户对着「缓冲中」干等
        if song.looks_playable() {
            self.state
                .info(format!("正在解析播放地址：{}", describe_song(&song)));
        } else {
            self.state.warn(format!(
                "《{}》可能受版权限制（VIP/已下架），若取链失败请换一首",
                song.name
            ));
        }

        self.request_lyric(song.clone());
        self.request_stream(song, start_at_ms);
    }

    fn request_stream(&mut self, song: Song, start_at_ms: u64) {
        let key = song.cache_key(&self.state.config.quality);

        // 缓存命中直接播，零网络开销
        if let Some(path) = self.cache.find(&key) {
            tlog!(crate::logger::LEVEL_DEBUG, "缓存命中：{}", path.display());
            self.audio.load(path, start_at_ms, song.duration_ms);
            return;
        }

        let api = self.api.clone();
        let bus = self.bus.clone();
        let quality = self.state.config.quality.clone();

        self.runtime.spawn(async move {
            match api.song_stream_url(&song, &quality).await {
                Ok(stream) => bus.emit(Loaded::StreamReady {
                    song: Box::new(song),
                    url: stream.url,
                    start_at_ms,
                    is_trial: stream.is_trial,
                    reason: stream.reason,
                }),
                Err(error) => bus.fail(format!("获取《{}》的播放地址失败", song.name), error),
            }
        });
    }

    fn request_lyric(&mut self, song: Song) {
        let api = self.api.clone();
        let bus = self.bus.clone();

        self.state.lyric.loading = true;
        self.runtime.spawn(async move {
            match api.fetch_lyric(&song).await {
                Ok(lyric) => bus.emit(Loaded::Lyric {
                    hash: song.hash.clone(),
                    lyric,
                }),
                Err(error) => {
                    // 歌词失败不影响播放，只在日志留痕
                    tlog!(
                        crate::logger::LEVEL_WARN,
                        "获取《{}》的歌词失败：{error}",
                        song.name
                    );
                    bus.emit(Loaded::Lyric {
                        hash: song.hash.clone(),
                        lyric: crate::api::model::Lyric::default(),
                    });
                }
            }
        });
    }

    // ==================================================================
    // 数据加载
    // ==================================================================

    /// 启动时带关键词直接进入搜索结果页（对应 `--search`）。
    pub fn startup_search(&mut self, keyword: &str) {
        self.switch_tab(Tab::Search);
        self.state.search.input.set(keyword);
        self.run_search();
    }

    pub fn run_search(&mut self) {
        let keyword = self.state.search.input.text().trim().to_string();
        if keyword.is_empty() {
            self.state.warn("请输入搜索关键词");
            return;
        }

        self.state.search.submitted = keyword.clone();
        self.state.search.editing = false;
        self.state.search.results.loading = true;
        self.state.search.results.title = format!("搜索「{keyword}」");
        self.state.busy = Some(format!("搜索 {keyword}"));

        let api = self.api.clone();
        let bus = self.bus.clone();
        let page_size = self.state.config.page_size;

        self.runtime.spawn(async move {
            // 刻意**只取第一页**，不做全量翻页。
            //
            // 实测：酷狗搜索只有第 1 页是精确匹配，深页塞的是兜底内容——
            // 搜「黑色幽默」翻到第 2 页往后全是「卖花的惹不起」这类有声书，
            // 全量合并会把相关结果淹没在垃圾里。MoeKoeMusic 也是分页浏览
            // （`searchResults.value = response.data.lists` 只放当前页）。
            // 想看更多按 `M` 一页页追加，顺序保持服务端的相关性。
            match api.search_songs(&keyword, 1, page_size).await {
                Ok(songs) => bus.emit(Loaded::Search {
                    keyword,
                    songs,
                    append: false,
                }),
                Err(error) => bus.fail(format!("搜索「{keyword}」失败"), error),
            }
        });
    }

    pub fn load_plaza_playlists(&mut self) {
        let api = self.api.clone();
        let bus = self.bus.clone();
        let (category, page_size) = (self.state.playlists.category, self.state.config.page_size);

        self.state.playlists.list.loading = true;
        self.state.busy = Some("载入歌单广场".to_string());

        self.runtime.spawn(async move {
            match api.plaza_playlists(category, 1, page_size).await {
                Ok(items) => bus.emit(Loaded::Playlists {
                    title: "歌单广场".to_string(),
                    items,
                }),
                Err(error) => bus.fail("载入歌单广场失败", error),
            }
        });
    }

    pub fn load_artists(&mut self) {
        let api = self.api.clone();
        let bus = self.bus.clone();
        let kind = self.state.artists.kind;

        self.state.artists.list.loading = true;
        self.state.busy = Some("载入歌手列表".to_string());

        self.runtime.spawn(async move {
            match api.artist_list(kind, 60).await {
                Ok(artists) => bus.emit(Loaded::Artists(artists)),
                Err(error) => bus.fail("载入歌手列表失败", error),
            }
        });
    }

    /// `f`：在歌手列表里轮换地区筛选。
    ///
    /// 取值含义见 KuGouMusicApi 文档的 `/artist/lists`：0 全部 / 1 华语 / 2 欧美 /
    /// 3 日韩 / 4 其他。切换后立刻重新拉取，并把上一次的选中项一并清掉——否则
    /// 筛选完还停在旧歌手上，用户会以为没生效。
    fn cycle_artist_filter(&mut self) {
        if self.state.tab != Tab::Artists {
            self.state
                .info("按 f 可筛选歌手地区，请先切到「歌手」标签页");
            return;
        }

        const REGIONS: [(i64, &str); 5] = [
            (0, "全部"),
            (1, "华语"),
            (2, "欧美"),
            (3, "日韩"),
            (4, "其他"),
        ];

        let current = REGIONS
            .iter()
            .position(|(kind, _)| *kind == self.state.artists.kind)
            .unwrap_or(0);
        let (kind, label) = REGIONS[(current + 1) % REGIONS.len()];

        self.state.artists.kind = kind;
        self.state.artists.list.replace(Vec::new());
        self.state.artists.songs.replace(String::new(), Vec::new());
        self.state.info(format!("歌手地区筛选：{label}"));
        self.load_artists();
    }

    pub fn load_ranks(&mut self) {
        let api = self.api.clone();
        let bus = self.bus.clone();

        self.state.ranks.list.loading = true;
        self.state.busy = Some("载入排行榜".to_string());

        self.runtime.spawn(async move {
            match api.rank_boards().await {
                Ok(boards) => bus.emit(Loaded::RankBoards(boards)),
                Err(error) => bus.fail("载入排行榜失败", error),
            }
        });
    }

    pub fn load_cloud_playlists(&mut self) {
        if !self.state.logged_in {
            self.state
                .warn("云端歌单需要登录，请配置 cookie（--cookie 或配置文件）");
            return;
        }

        let api = self.api.clone();
        let bus = self.bus.clone();

        self.state.cloud.list.loading = true;
        self.state.busy = Some("载入云端歌单".to_string());

        self.runtime.spawn(async move {
            match api.user_playlists().await {
                Ok(items) => bus.emit(Loaded::CloudPlaylists(items)),
                Err(error) => bus.fail("载入云端歌单失败", error),
            }
        });
    }

    fn open_selected_playlist(&mut self) {
        let Some(playlist) = self.state.playlists.list.selected().cloned() else {
            self.state.warn("请先选择一个歌单");
            return;
        };
        self.load_playlist_songs(playlist, PlaylistSource::Plaza);
    }

    fn open_selected_cloud_playlist(&mut self) {
        let Some(playlist) = self.state.cloud.list.selected().cloned() else {
            self.state.warn("请先选择一个歌单");
            return;
        };
        // 选中的歌单同时作为云端同步目标
        if playlist.is_writable() {
            self.state.sync_target = Some(playlist.clone());
        }
        self.load_playlist_songs(playlist, PlaylistSource::Cloud);
    }

    /// 载入歌单歌曲。
    ///
    /// `source` 决定结果落到哪个歌曲面板：歌单广场和云端歌单共用一个请求路径，
    /// 但它们是两个独立的界面区域。
    fn load_playlist_songs(&mut self, playlist: Playlist, source: PlaylistSource) {
        let api = self.api.clone();
        let bus = self.bus.clone();

        let pane = match source {
            PlaylistSource::Plaza => &mut self.state.playlists.songs,
            PlaylistSource::Cloud => &mut self.state.cloud.songs,
        };
        pane.loading = true;
        pane.title = format!("{}（载入中…）", playlist.name);
        self.state.busy = Some(format!("载入歌单《{}》", playlist.name));

        self.runtime.spawn(async move {
            // 自建/收藏歌单走新版接口（按数字 listid），公开歌单走 global_collection_id
            let is_own = playlist.list_id.filter(|_| playlist.is_own);

            // ---- 首屏：先只取第一页，让界面立刻有内容 ----
            //
            // 大歌单（几百首）即使并发翻页也要好几秒，这段时间界面只有一个"载入中"，
            // 体验很差。学 MoeKoeMusic 的做法：先给首屏，剩下的后台继续取。
            // 它那边是滚动到底再加载；我们一次性取完，但**先让用户看到东西**。
            let first = match is_own {
                Some(list_id) => api.user_playlist_tracks(list_id, 1, PAGE_LIMIT).await,
                None => api.playlist_tracks(&playlist.id, 1, PAGE_LIMIT).await,
            };

            match first {
                Ok(songs) => {
                    let has_more = songs.len() >= PAGE_LIMIT as usize;
                    bus.emit(Loaded::PlaylistTracks {
                        playlist: playlist.clone(),
                        songs,
                        source,
                    });
                    // 不足一页说明这首页就是全部，没必要再取
                    if !has_more {
                        return;
                    }
                }
                Err(error) => {
                    bus.fail(format!("载入歌单《{}》失败", playlist.name), error);
                    return;
                }
            }

            // ---- 后台继续取全部，取到后覆盖为完整列表 ----
            //
            // 界面上会看到「30 首」变成「400 首」，是个不错的进度反馈，
            // 比干等一个"载入中"强。
            let result = match is_own {
                Some(list_id) => api.user_playlist_tracks_all(list_id).await,
                None => api.playlist_tracks_all(&playlist.id).await,
            };

            match result {
                Ok(songs) => bus.emit(Loaded::PlaylistTracks {
                    playlist,
                    songs,
                    source,
                }),
                Err(error) => bus.fail(format!("载入歌单《{}》失败", playlist.name), error),
            }
        });
    }

    fn open_selected_artist(&mut self) {
        let Some(artist) = self.state.artists.list.selected().cloned() else {
            self.state.warn("请先选择一位歌手");
            return;
        };

        let api = self.api.clone();
        let bus = self.bus.clone();

        self.state.artists.songs.loading = true;
        self.state.artists.songs.title = format!("{}（载入中…）", artist.name);
        self.state.busy = Some(format!("载入歌手 {}", artist.name));

        self.runtime.spawn(async move {
            match api.artist_tracks_all(artist.id, "hot").await {
                Ok(songs) => bus.emit(Loaded::ArtistSongs { artist, songs }),
                Err(error) => bus.fail(format!("载入歌手 {} 的歌曲失败", artist.name), error),
            }
        });
    }

    fn open_selected_rank(&mut self) {
        let Some(board) = self.state.ranks.list.selected().cloned() else {
            self.state.warn("请先选择一个榜单");
            return;
        };

        let api = self.api.clone();
        let bus = self.bus.clone();

        self.state.ranks.songs.loading = true;
        self.state.ranks.songs.title = format!("{}（载入中…）", board.name);
        self.state.busy = Some(format!("载入榜单 {}", board.name));

        self.runtime.spawn(async move {
            match api.rank_tracks_all(board.id).await {
                Ok(songs) => bus.emit(Loaded::RankTracks { board, songs }),
                Err(error) => bus.fail(format!("载入榜单 {} 失败", board.name), error),
            }
        });
    }

    fn reload_current_tab(&mut self) {
        match self.state.tab {
            Tab::Search => {
                if self.state.search.submitted.is_empty() {
                    self.state.info("还没有搜索过，按 / 开始搜索");
                } else {
                    self.run_search();
                }
            }
            Tab::Playlists => self.load_plaza_playlists(),
            Tab::Artists => self.load_artists(),
            Tab::Ranks => self.load_ranks(),
            Tab::Cloud => self.load_cloud_playlists(),
            Tab::Visualizer => self.state.info("可视化页面没有需要刷新的数据"),
        }
    }

    // ==================================================================
    // 队列与云端
    // ==================================================================

    fn queue_focused_song(&mut self, play_next: bool) {
        let Some(song) = self
            .state
            .focused_songs()
            .and_then(|songs| songs.selected().cloned())
        else {
            self.state.warn("当前没有选中的歌曲");
            return;
        };

        let label = describe_song(&song);
        if play_next {
            self.state.queue.insert_next(song);
            self.state.success(format!("已插入到下一首：{label}"));
        } else {
            self.state.queue.append(song);
            self.state.success(format!("已加入队列：{label}"));
        }
        self.clamp_queue_cursor();
    }

    /// `x`：把播放队列里选中的歌曲移出队列。
    fn remove_selected_from_queue(&mut self) {
        if self.state.focus != Focus::Queue {
            self.state.warn("请先按 Tab 把焦点切到播放队列");
            return;
        }
        let Some(index) = self.state.queue_cursor.selected() else {
            self.state.warn("播放队列为空");
            return;
        };
        let Some(removed) = self.state.queue.remove(index) else {
            return;
        };

        self.clamp_queue_cursor();
        // 被移除的正好是当前曲目时停止播放，避免出现「在放一首已不在队列里的歌」
        let removed_current = self
            .state
            .current
            .as_ref()
            .map(|song| song.hash == removed.hash)
            .unwrap_or(false);
        if removed_current {
            self.audio.stop();
            self.state.current = None;
            self.state.playback = PlaybackState::Stopped;
            self.state.position_ms = 0;
            self.state.duration_ms = 0;
        }

        self.state
            .info(format!("已从队列移除：{}", describe_song(&removed)));
    }

    // ==================================================================
    // 扫码登录
    // ==================================================================

    /// `L`：在应用内开始扫码登录。
    ///
    /// 流程是 `/login/qr/key` → `/login/qr/create` → 轮询 `/login/qr/check`。
    /// 二维码直接渲染在界面上，不用切终端，也不用额外依赖 `qrencode` 之类的命令行工具。
    fn start_login(&mut self) {
        let config_path = Config::path();

        if self.state.logged_in {
            // 只提示保存位置，不回显凭据
            self.state
                .info(format!("已登录，凭据保存在 {}", config_path.display()));
            return;
        }

        if let Some(login) = self.state.login.as_ref() {
            if !login.finished {
                self.state.info("登录已在进行中，扫码或按 Esc 取消");
                return;
            }
        }

        self.state.login = Some(LoginState {
            message: "正在获取二维码…".to_string(),
            ..Default::default()
        });

        let api = self.api.clone();
        let bus = self.bus.clone();

        self.runtime.spawn(async move {
            let key = match api.login_qr_key().await {
                Ok(key) => key,
                Err(error) => {
                    bus.fail("获取登录二维码失败", error);
                    return;
                }
            };

            match api.login_qr_create(&key).await {
                Ok(content) => bus.emit(Loaded::LoginQr { key, content }),
                Err(error) => bus.fail("生成登录二维码失败", error),
            }
        });
    }

    /// 轮询扫码状态。由 tick 每约 2 秒调用一次。
    fn poll_login(&mut self) {
        let Some(login) = self.state.login.as_ref() else {
            return;
        };
        if login.finished || login.key.is_empty() {
            return;
        }

        let key = login.key.clone();
        let api = self.api.clone();
        let bus = self.bus.clone();

        self.runtime.spawn(async move {
            let check = match api.login_qr_check(&key).await {
                Ok(check) => check,
                Err(error) => {
                    bus.fail("查询扫码状态失败", error);
                    return;
                }
            };

            match check.status {
                QrStatus::Expired => bus.emit(Loaded::LoginFailed {
                    message: "二维码已过期，请按 Esc 后重新按 L".to_string(),
                }),
                QrStatus::Waiting => bus.emit(Loaded::LoginStatus {
                    message: "等待扫码…".to_string(),
                }),
                QrStatus::Pending => bus.emit(Loaded::LoginStatus {
                    message: "已扫码，请在手机上确认".to_string(),
                }),
                QrStatus::Success => match (check.token, check.userid) {
                    (Some(token), Some(userid)) => {
                        bus.emit(Loaded::LoginSucceeded { token, userid })
                    }
                    _ => bus.emit(Loaded::LoginFailed {
                        message: "扫码已授权，但未拿到 token".to_string(),
                    }),
                },
            }
        });
    }

    /// 结束登录（成功或失败）。
    fn finish_login(&mut self, succeeded: bool, message: String) {
        self.state.login = Some(LoginState {
            finished: true,
            succeeded,
            message,
            ..Default::default()
        });
    }

    /// 写入登录凭据并热更新 ApiClient 的 cookie。
    fn apply_login(&mut self, token: String, userid: String) {
        let cookie = format!("token={token}; userid={userid}");

        self.state.config.cookie = Some(cookie);
        // 必须走 `cookie_header()`：它会把 dfid 拼进去。直接用裸 cookie 会把
        // 已有的 dfid 冲掉，本次会话取播放直链就会报「本次请求需要验证」。
        self.api.set_cookie(self.state.config.cookie_header());

        let config_path = Config::path();
        match self.state.config.save() {
            Ok(()) => {
                self.state.logged_in = true;
                // 刻意**不回显 cookie**：它等价于账号密码，显示在界面上会被旁观者看到。
                // 只告诉用户写到了哪里。
                self.finish_login(
                    true,
                    format!(
                        "登录成功（userid={userid}），凭据已写入 {}",
                        config_path.display()
                    ),
                );
                self.state.success("登录成功，按 Esc 关闭");
                // 顺带取一次会员信息，界面上能直接看到服务端认定的会员形态
                self.fetch_vip_status();
            }
            Err(error) => {
                self.finish_login(false, format!("保存登录凭据失败：{error}"));
            }
        }
    }

    /// `v`：切换到下一个音源（酷狗 ↔ 酷狗概念版）。
    ///
    /// 只换「去哪儿请求 + 带什么身份」，**不碰播放队列、不打断当前曲目**——
    /// 正在放的音频已经在本地缓存里，换音源没有理由把它停掉。
    pub fn switch_source(&mut self) {
        let next = self.state.config.active_source_kind().next();

        // 先把当前身份存回档案，否则切走再切回来时登录态和 dfid 就丢了
        self.state.config.sync_active_source();
        self.state.config.switch_source(next);

        if let Err(error) = self.state.config.save() {
            self.state
                .warn(format!("音源已切换，但保存配置失败：{error}"));
        }

        // 用新音源的地址与身份重建 HTTP 客户端。
        // cookie 必须走 `cookie_header()`（而不是 `config.cookie`）：前者会带上 dfid，
        // 缺了它 `/song/url` 会返回 errcode 20028「本次请求需要验证」。
        match crate::api::ApiClient::new(
            &self.state.config.api_base,
            self.state.config.cookie_header(),
            self.state.config.proxy.as_deref(),
        ) {
            Ok(client) => {
                self.api = client;
                // 身份可能变了，会员信息要重新取
                self.state.vip_label = None;
                self.fetch_vip_status();
                // dfid 是平台相关的：新音源的档案里可能还没有，不补一个的话
                // 该音源在本会话内取链会一直失败（启动时的探测只跑一次）。
                self.ensure_device_fingerprint();
                // 两个平台的登录态不通用：切过去若是空的，得明确告诉用户重新扫码，
                // 否则他会以为「切了概念版还是只能试听」——其实只是没登录。
                if self.state.config.cookie.is_none() {
                    self.state.warn(format!(
                        "已切换到「{}」，但该音源还没登录——按 L 重新扫码（两个平台账号不通用）",
                        next.label()
                    ));
                } else {
                    self.state.success(format!(
                        "已切换到「{}」音源，当前播放不受影响",
                        next.label()
                    ));
                }
            }
            Err(error) => {
                self.state
                    .error(format!("切换到「{}」失败：{error}", next.label()));
            }
        }
    }

    /// 拉一次会员信息，把摘要显示在侧边栏。
    ///
    /// 这一步是排查「明明有会员却只能试听」的关键：界面上能直接看到服务端认定的
    /// 会员形态与到期时间，不用去翻日志或 curl。
    pub fn fetch_vip_status(&mut self) {
        if !self.state.logged_in {
            self.state.vip_label = None;
            return;
        }

        let api = self.api.clone();
        let bus = self.bus.clone();

        self.runtime.spawn(async move {
            match api.user_vip_detail().await {
                Ok(info) => bus.emit(Loaded::VipStatus {
                    label: info.label(),
                }),
                // 取不到会员信息不影响听歌，静默降级即可
                Err(error) => tlog!(crate::logger::LEVEL_WARN, "获取会员信息失败：{error}"),
            }
        });
    }

    /// `A`：把当前列表**全部**加入播放队列。
    ///
    /// 一次 `append_all`，避免逐首调用（数百首时逐首添加既慢又容易在扩容时卡顿。
    fn queue_all_songs(&mut self) {
        let descending = self.state.sort_descending;
        let Some(songs) = self.state.focused_songs().map(|list| list.songs.clone()) else {
            self.state.warn("当前没有可加入队列的歌曲列表");
            return;
        };

        if songs.is_empty() {
            self.state.warn("列表为空，先按 Enter 载入歌曲");
            return;
        }

        let count = songs.len();
        let _ = self.state.queue.append_all(songs);
        self.clamp_queue_cursor();

        let order = if descending { "倒序" } else { "正序" };
        self.state
            .success(format!("已把 {count} 首歌加入队列（{order}）"));
    }

    /// `o`：切换歌曲列表的排列方向，并同步反转所有已载入的列表。
    ///
    /// 反转的是存储，所以界面顺序与播放队列顺序始终一致——这也是它比「渲染时反转」更
    /// 可靠的理由。
    fn toggle_sort_order(&mut self) {
        self.state.sort_descending = !self.state.sort_descending;
        let descending = self.state.sort_descending;

        self.state.search.results.toggle_sort();
        self.state.playlists.songs.toggle_sort();
        self.state.artists.songs.toggle_sort();
        self.state.ranks.songs.toggle_sort();
        self.state.cloud.songs.toggle_sort();

        // 排列方向变了，排序提示也跟着更新，免得用户按 o 之后界面没有任何反馈。
        let order = if descending {
            "倒序（最后一首在最上）"
        } else {
            "正序"
        };
        self.state.info(format!("列表排列：{order}"));
    }

    /// 执行「清空播放队列」。**停止当前播放**——队列都没了，继续播一首不在队列里的歌既没意义
    /// （下一首无从查找）。若只想删掉其中一首，用 `x`，它不会打断当前播放。
    /// 清空音频缓存目录。
    ///
    /// 由确认弹窗触发——删文件不可恢复，不能让一次误按就清掉全部缓存。
    /// 清空后立刻重新统计占用，界面上的「已用」才会跟着变。
    fn clear_cache(&mut self) {
        match self.cache.clear() {
            Ok(report) => {
                self.refresh_cache_usage();

                let freed = crate::ui::widgets::human_bytes(report.freed_bytes);
                if report.failed > 0 {
                    self.state.warn(format!(
                        "已清理 {} 个文件（{}），{} 个删除失败（检查目录权限）",
                        report.removed_files, freed, report.failed
                    ));
                } else if report.removed_files == 0 {
                    self.state.info("缓存已经是空的");
                } else {
                    self.state.success(format!(
                        "已清理缓存：{} 个文件，释放 {}",
                        report.removed_files, freed
                    ));
                }
            }
            Err(error) => self.state.error(format!("清空缓存失败：{error}")),
        }
    }

    fn clear_queue(&mut self) {
        if self.state.queue.is_empty() {
            self.state.warn("播放队列已经为空");
            return;
        }

        let count = self.state.queue.len();
        self.audio.stop();
        self.state.queue.clear();
        self.state.queue_cursor.select(None);
        self.state.current = None;
        self.state.playback = PlaybackState::Stopped;
        self.state.position_ms = 0;
        self.state.duration_ms = 0;
        self.state
            .info(format!("已清空播放队列（{count} 首），已停止播放"));
    }

    /// `d`：把选中歌曲从当前云端歌单移除。
    ///
    /// 用的是歌单条目的 `fileid` 而不是 hash —— 歌单里同一首歌的 fileid 才是它在歌单里的位置标识。
    pub fn remove_focused_song_from_cloud(&mut self) {
        let Some(target) = self.state.sync_target.as_ref() else {
            self.state.warn("请先在「云端」标签页选中一个歌单");
            return;
        };

        let Some(list_id) = target.list_id else {
            self.state.error("该歌单不可写（缺少 listid）");
            return;
        };

        if !self.state.logged_in {
            self.state.warn("需要登录才能修改云端歌单，按 L 扫码登录");
            return;
        }

        let Some(song) = self
            .state
            .focused_songs()
            .and_then(|list| list.selected().cloned())
        else {
            self.state.warn("当前没有选中的歌曲");
            return;
        };

        let Some(file_id) = song.file_id else {
            // 只有歌单接口才给 fileid；搜索结果没有，无法定位歌单内的条目。
            self.state
                .warn("这首歌不在歌单里（缺少 fileid），无法从歌单移除");
            return;
        };

        let label = describe_song(&song);
        let name = target.name.clone();
        let api = self.api.clone();
        let bus = self.bus.clone();

        self.state.busy = Some(format!("从《{name}》移除歌曲"));

        self.runtime.spawn(async move {
            match api.remove_tracks_from_playlist(list_id, &[file_id]).await {
                Ok(count) => bus.emit(Loaded::CloudNotice(format!(
                    "已从《{name}》移除 {count} 首歌"
                ))),
                Err(error) => bus.fail(format!("从《{}》移除《{}》失败", name, label), error),
            }
        });
    }

    /// 执行「删除云端歌单」。
    fn delete_cloud_playlist(&mut self) {
        let Some(target) = self.state.sync_target.take() else {
            self.state.warn("没有选中的云端歌单");
            return;
        };

        let Some(list_id) = target.list_id else {
            self.state.error("该歌单不可写（缺少 listid）");
            return;
        };

        if !self.state.logged_in {
            self.state.warn("需要登录才能删除云端歌单，按 L 扫码登录");
            return;
        }

        let name = target.name.clone();
        let api = self.api.clone();
        let bus = self.bus.clone();

        self.state.busy = Some(format!("删除歌单《{name}》"));

        self.runtime.spawn(async move {
            match api.delete_playlist(list_id).await {
                Ok(()) => bus.emit(Loaded::CloudNotice(format!("已删除歌单《{}》", name))),
                Err(error) => bus.fail(format!("删除歌单《{}》失败", name), error),
            }
        });
    }

    /// 用输入的名字新建云端歌单。
    fn create_cloud_playlist(&mut self, name: String) {
        if name.trim().is_empty() {
            self.state.warn("歌单名称不能为空");
            return;
        }

        if !self.state.logged_in {
            self.state.warn("需要登录才能新建云端歌单，按 L 扫码登录");
            return;
        }

        let api = self.api.clone();
        let bus = self.bus.clone();
        self.state.busy = Some("新建云端歌单".to_string());

        self.runtime.spawn(async move {
            match api.create_playlist(&name).await {
                Ok(list_id) => bus.emit(Loaded::CloudNotice(match list_id {
                    Some(id) => format!("已新建歌单《{name}》（listid={id}）"),
                    None => format!("已新建歌单《{name}》"),
                })),
                Err(error) => bus.fail(format!("新建歌单《{}》失败", name), error),
            }
        });
    }

    fn add_focused_song_to_cloud(&mut self) {
        if !self.state.logged_in {
            self.state.warn("云端歌单需要登录，请配置 cookie");
            return;
        }
        let Some(target) = self.state.sync_target.clone() else {
            self.state.warn("请先在「云端」标签页选中一个歌单作为目标");
            return;
        };
        let Some(list_id) = target.list_id else {
            self.state.error("该歌单不可写（缺少 listid）");
            return;
        };
        let Some(song) = self
            .state
            .focused_songs()
            .and_then(|songs| songs.selected().cloned())
        else {
            self.state.warn("当前没有选中的歌曲");
            return;
        };

        let label = describe_song(&song);
        let playlist_name = target.name.clone();
        let api = self.api.clone();
        let bus = self.bus.clone();

        self.state.busy = Some(format!("收藏《{label}》"));

        self.runtime.spawn(async move {
            match api
                .add_tracks_to_playlist(list_id, std::slice::from_ref(&song))
                .await
            {
                Ok(_) => bus.emit(Loaded::CloudNotice(format!(
                    "已把《{label}》收藏到《{playlist_name}》"
                ))),
                Err(error) => bus.fail(format!("收藏《{label}》失败"), error),
            }
        });
    }

    fn sync_queue_to_cloud(&mut self) {
        if !self.state.logged_in {
            self.state.warn("云端歌单需要登录，请配置 cookie");
            return;
        }
        let Some(target) = self.state.sync_target.clone() else {
            self.state
                .warn("请先在「云端」标签页选中一个歌单作为同步目标");
            return;
        };
        let Some(list_id) = target.list_id else {
            self.state.error("该歌单不可写（缺少 listid）");
            return;
        };
        if self.state.queue.is_empty() {
            self.state.warn("播放队列为空，没有可同步的内容");
            return;
        }

        let songs = self.state.queue.items().to_vec();
        let count = songs.len();
        let playlist_name = target.name.clone();
        let api = self.api.clone();
        let bus = self.bus.clone();

        self.state.busy = Some(format!("同步 {count} 首到《{playlist_name}》"));

        self.runtime.spawn(async move {
            match api.add_tracks_to_playlist(list_id, &songs).await {
                Ok(written) => bus.emit(Loaded::CloudNotice(format!(
                    "已把 {written} 首歌同步到《{playlist_name}》"
                ))),
                Err(error) => bus.fail(format!("同步到《{playlist_name}》失败"), error),
            }
        });
    }

    fn sync_queue_cursor(&mut self, index: usize) {
        if self.state.queue.is_empty() {
            self.state.queue_cursor.select(None);
        } else {
            self.state
                .queue_cursor
                .select(Some(index.min(self.state.queue.len() - 1)));
        }
    }

    fn clamp_queue_cursor(&mut self) {
        let len = self.state.queue.len();
        if len == 0 {
            self.state.queue_cursor.select(None);
            return;
        }
        let current = self.state.queue_cursor.selected().unwrap_or(0).min(len - 1);
        self.state.queue_cursor.select(Some(current));
    }

    // ==================================================================
    // 异步结果
    // ==================================================================

    fn handle_loaded(&mut self, loaded: Loaded) {
        match loaded {
            Loaded::Search {
                keyword,
                songs,
                append,
            } => {
                self.state.busy = None;

                if append {
                    // 追加：保持服务端给的相关性顺序，**不重排**。
                    // 重排会把新一页的相关结果搅进旧结果里，破坏"越靠前越相关"。
                    let pane = &mut self.state.search.results;
                    let mut all = std::mem::take(&mut pane.songs);
                    let added = songs.len();
                    all.extend(songs);
                    let total = all.len();
                    pane.set_songs_sorted(format!("搜索「{keyword}」· {total} 首"), all, false);
                    if added == 0 {
                        self.state.info("没有更多结果了");
                    } else {
                        self.state
                            .success(format!("又加载了 {added} 首（共 {total} 首）"));
                    }
                } else {
                    let count = songs.len();
                    // 标题带上条数：只写「搜索「XX」」会让人以结果就列表里这几条，
                    // 实际接口 total 常有好几百，按 M 可以继续加载。
                    self.state.search.page = 1;
                    //
                    // 这里**刻意不**跟随 `sort_descending`。
                    // 那个开关是给歌单用的（新歌在最上），但搜索结果的价值全在
                    // 服务端给的相关性排序上：默认倒序会把最相关的翻到最后，
                    // 实测搜「黑色幽默」原本周杰伦排第一，倒序后前排变成
                    // 「潜水不会游」「科野」这类，等于搜不到想要的东西。
                    self.state.search.results.set_songs_sorted(
                        format!("搜索「{keyword}」· {count} 首"),
                        songs,
                        false,
                    );
                    if count == 0 {
                        self.state.warn(format!("「{keyword}」没有找到结果"));
                    } else {
                        self.state.success(format!("「{keyword}」找到 {count} 首"));
                        // 结果到手后把焦点交给列表，方便直接按 Enter 播放
                        self.state.focus = Focus::Secondary;
                    }
                }
            }

            Loaded::Playlists { title, items } => {
                self.state.busy = None;
                let count = items.len();
                self.state.playlists.list.replace(items);
                if count == 0 {
                    self.state.warn(format!("{title}暂无内容"));
                } else {
                    self.state.info(format!("{title}：{count} 个歌单"));
                }
            }

            Loaded::PlaylistTracks {
                playlist,
                songs,
                source,
            } => {
                self.state.busy = None;
                let descending = self.state.sort_descending;
                let count = songs.len();
                let title = format!("{} · {count} 首", playlist.name);

                match source {
                    PlaylistSource::Plaza => self
                        .state
                        .playlists
                        .songs
                        .set_songs_sorted(title, songs, descending),
                    PlaylistSource::Cloud => self
                        .state
                        .cloud
                        .songs
                        .set_songs_sorted(title, songs, descending),
                }

                self.state.focus = Focus::Secondary;
                self.state
                    .info(format!("《{}》共 {count} 首", playlist.name));
            }

            Loaded::Artists(artists) => {
                self.state.busy = None;
                let count = artists.len();
                self.state.artists.list.replace(artists);
                if count == 0 {
                    self.state.warn("歌手列表为空");
                } else {
                    self.state.info(format!("共 {count} 位歌手"));
                }
            }

            Loaded::ArtistSongs { artist, songs } => {
                self.state.busy = None;
                let descending = self.state.sort_descending;
                let count = songs.len();
                self.state.artists.songs.set_songs_sorted(
                    format!("{} · {count} 首", artist.name),
                    songs,
                    descending,
                );
                self.state.focus = Focus::Secondary;
            }

            Loaded::RankBoards(boards) => {
                self.state.busy = None;
                let count = boards.len();
                self.state.ranks.list.replace(boards);
                if count == 0 {
                    self.state.warn("排行榜列表为空");
                } else {
                    self.state.info(format!("共 {count} 个榜单"));
                }
            }

            Loaded::RankTracks { board, songs } => {
                self.state.busy = None;
                let descending = self.state.sort_descending;
                let count = songs.len();
                self.state.ranks.songs.set_songs_sorted(
                    format!("{} · {count} 首", board.name),
                    songs,
                    descending,
                );
                self.state.focus = Focus::Secondary;
            }

            Loaded::CloudPlaylists(items) => {
                self.state.busy = None;
                // 刻意不写状态栏：这个方法也会在「同步完成」之后被调用，而那句
                // 「已同步 N 首」比「云端歌单：N 个」重要得多，不该被它覆盖掉。
                // 列表本身的变化已经是足够的反馈。
                self.state.cloud.list.replace(items);
            }

            Loaded::Lyric { hash, lyric } => {
                // 用户可能已经切歌：`lyric.hash` 在 start_playback 时被设为当前曲目，
                // 对不上说明这是上一首的迟到结果，直接丢弃
                if self.state.lyric.hash.as_deref() != Some(hash.as_str()) {
                    return;
                }
                let empty = lyric.is_empty();
                self.state.lyric.lyric = lyric;
                self.state.lyric.loading = false;
                self.state.lyric.active_line = None;
                if empty {
                    tlog!(crate::logger::LEVEL_DEBUG, "歌曲 {hash} 没有可用歌词");
                }
            }

            Loaded::StreamReady {
                song,
                url,
                start_at_ms,
                is_trial,
                reason,
            } => {
                if !self.is_current(&song) {
                    return;
                }
                // 记下来：片段播完时要提示，而不是当成正常结束直接切下一首
                self.state.current_is_trial = is_trial;
                if is_trial {
                    // 带上服务端给的原因，否则用户只能猜「是不是会员没生效」
                    let why = reason
                        .map(|why| format!("：{why}"))
                        .unwrap_or_else(|| "（完整版需要对应会员）".to_string());
                    self.state
                        .warn(format!("《{}》只有试听片段{why}", song.name));
                }
                // 片段落到独立的缓存键，避免把完整版的位置占住
                self.start_download(*song, url, start_at_ms, is_trial);
            }

            Loaded::DownloadProgress { received, total } => {
                self.state.download_progress = Some((received, total));
            }

            Loaded::StreamCached {
                song,
                path,
                start_at_ms,
            } => {
                if !self.is_current(&song) {
                    // 用户已切歌，删掉刚下载的孤儿文件，避免缓存被无用数据占满
                    if let Err(error) = std::fs::remove_file(&path) {
                        tlog!(
                            crate::logger::LEVEL_WARN,
                            "清理过期缓存 {} 失败：{error}",
                            path.display()
                        );
                    }
                    return;
                }

                self.state.download_progress = None;
                self.state.busy = None;
                self.audio.load(path, start_at_ms, song.duration_ms);

                // 缓存回收是同步目录扫描，扔到阻塞线程池，别卡住 UI
                let cache = self.cache.clone();
                self.runtime
                    .spawn_blocking(move || match cache.enforce_limit() {
                        Ok(report) if report.removed_files > 0 => tlog!(
                            crate::logger::LEVEL_INFO,
                            "缓存回收：删除 {} 个文件，释放 {} 字节",
                            report.removed_files,
                            report.freed_bytes
                        ),
                        Ok(_) => {}
                        Err(error) => {
                            tlog!(crate::logger::LEVEL_WARN, "缓存回收失败：{error}")
                        }
                    });
            }

            Loaded::DeviceFingerprint(dfid) => {
                self.state.config.dfid = Some(dfid.clone());
                let cookie = self.state.config.cookie_header();
                self.api.set_cookie(cookie);
                tlog!(crate::logger::LEVEL_INFO, "已获取设备指纹 dfid={dfid}");
                // 立刻落盘，下次启动就不用再请求
                if let Err(error) = self.state.config.save() {
                    tlog!(crate::logger::LEVEL_WARN, "保存 dfid 到配置失败：{error}");
                }
            }

            Loaded::CacheUsage(bytes) => {
                self.state.cache_bytes = bytes;
            }

            Loaded::CloudNotice(message) => {
                self.state.busy = None;
                self.state.success(message);
                // 刷新云端歌单，让新加入的歌曲数量反映出来
                self.load_cloud_playlists();
            }

            Loaded::LoginQr { key, content } => {
                let qr = crate::ui::widgets::qr_lines(&content).unwrap_or_default();
                let login = self.state.login.get_or_insert_with(LoginState::default);
                login.qr = qr;
                login.key = key;
                login.message = "用酷狗 App 扫码登录".to_string();
                login.finished = false;
            }

            Loaded::LoginStatus { message } => {
                if let Some(login) = self.state.login.as_mut() {
                    if !login.finished {
                        login.message = message;
                    }
                }
            }

            Loaded::LoginSucceeded { token, userid } => {
                self.apply_login(token, userid);
            }

            Loaded::LoginFailed { message } => {
                self.finish_login(false, message);
            }

            Loaded::VipStatus { label } => {
                self.state.vip_label = Some(label);
            }

            Loaded::Failed { context, error } => {
                self.state.busy = None;
                self.state.download_progress = None;
                tlog!(crate::logger::LEVEL_ERROR, "{context}：{error}");
                let hint = error.user_hint();
                self.state.error(format!("{context}：{hint}"));
            }
        }
    }

    fn start_download(&mut self, song: Song, url: String, start_at_ms: u64, is_trial: bool) {
        let key = if is_trial {
            song.trial_cache_key(&self.state.config.quality)
        } else {
            song.cache_key(&self.state.config.quality)
        };
        let extension = Downloader::extension_from_url(&url);
        let target = self.cache.path_for(&key, extension);

        let downloader = self.downloader.clone();
        let bus = self.bus.clone();
        let label = describe_song(&song);

        self.state.download_progress = Some((0, None));
        self.state.busy = Some(format!("缓冲《{label}》"));

        self.runtime.spawn(async move {
            // 节流用原子量而不是 Cell：闭包需要 Send + Sync
            let last_reported = AtomicU64::new(0);
            let progress = |received: u64, total: Option<u64>| {
                let finished = total.is_some_and(|total| received >= total);
                if !finished
                    && received.saturating_sub(last_reported.load(Ordering::Relaxed))
                        < PROGRESS_STEP_BYTES
                {
                    return;
                }
                last_reported.store(received, Ordering::Relaxed);
                bus.emit(Loaded::DownloadProgress { received, total });
            };

            match downloader.fetch_to(&url, &target, &progress).await {
                Ok(_) => bus.emit(Loaded::StreamCached {
                    song: Box::new(song),
                    path: target,
                    start_at_ms,
                }),
                Err(error) => bus.fail(format!("下载《{label}》失败"), error),
            }
        });
    }

    /// 事件里的歌曲是否仍是当前播放的那首。
    fn is_current(&self, song: &Song) -> bool {
        self.state
            .current
            .as_ref()
            .map(|current| current.hash == song.hash)
            .unwrap_or(false)
    }

    // ==================================================================
    // 音频事件与心跳
    // ==================================================================

    fn handle_audio_event(&mut self, event: AudioEvent) {
        match event {
            AudioEvent::Ready { duration_ms } => {
                if duration_ms > 0 {
                    self.state.duration_ms = duration_ms;
                }
                self.state.busy = None;
                self.state.download_progress = None;
                self.state.playback = PlaybackState::Playing;
                let title = self
                    .state
                    .current
                    .as_ref()
                    .map(describe_song)
                    .unwrap_or_default();
                self.state.success(format!("正在播放：{title}"));
            }
            AudioEvent::TrackFinished => {
                self.state.position_ms = 0;

                // 试听片段播完 ≠ 整首播完。这里不能走自动切歌：用户会以为是
                // 「会员没生效、听了几十秒就跳歌」，而实际是没拿到完整版。
                // 停在当前曲目并说清楚原因，把选择权交还给用户。
                if self.state.current_is_trial {
                    self.state.current_is_trial = false;
                    self.state.playback = PlaybackState::Stopped;
                    self.state
                        .warn("试听片段已播完（完整版需要对应会员），按 n 跳下一首");
                    return;
                }

                // 自然播完：顺序模式到底就停，单曲循环原地重播
                self.next_track(false);
            }
            AudioEvent::Failed(message) => {
                self.state.busy = None;
                self.state.download_progress = None;
                self.state.playback = PlaybackState::Stopped;
                self.state.error(message);
            }
        }
    }

    /// 定时心跳：同步播放状态、推进歌词、低频测量缓存占用。
    fn tick(&mut self) {
        self.state.ticks = self.state.ticks.wrapping_add(1);
        self.state.playback = self.audio.state();
        self.state.position_ms = self.audio.position_ms();

        let duration = self.audio.duration_ms();
        if duration > 0 {
            self.state.duration_ms = duration;
        }
        // 算出真实经过时长：动画要按时间缓动，不能按帧数，否则帧率一变观感就变
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame_at);
        self.last_frame_at = now;

        // 电平每帧刷新（audio 那一侧只是读原子量，开销可忽略）
        self.state.levels = self.audio.levels();
        self.state.advance_visualizer(elapsed);

        self.sync_mpris();

        if self.state.playback == PlaybackState::Playing {
            self.update_active_lyric();
        }

        // 登录中：每约 2 秒轮询一次扫码状态（tick 默认 200ms，10 拍 = 2s）
        if self.state.login.is_some() && self.state.ticks % 10 == 0 {
            self.poll_login();
        }

        if self.state.ticks % CACHE_MEASURE_TICKS == 0 {
            self.refresh_cache_usage();
        }
    }

    /// 测量缓存占用。目录扫描是同步 IO，扔到阻塞线程池执行。
    pub fn refresh_cache_usage(&mut self) {
        let cache = self.cache.clone();
        let bus = self.bus.clone();
        self.runtime.spawn_blocking(move || {
            bus.emit(Loaded::CacheUsage(cache.total_bytes()));
        });
    }

    fn update_active_lyric(&mut self) {
        // 正值表示歌词提前，所以要从播放位置里减掉偏移
        let position = (self.state.position_ms as i64 - self.state.config.lyric_offset_ms).max(0);
        let index = self.state.lyric.lyric.index_at(position as u64);
        if index != self.state.lyric.active_line {
            self.state.lyric.active_line = index;
        }
    }

    /// 取一条事件，最多处理 [`MAX_EVENTS_PER_FRAME`] 条，防止事件洪水饿死渲染。
    pub fn drain_events(&mut self) {
        for _ in 0..MAX_EVENTS_PER_FRAME {
            let Ok(event) = self.receiver.try_recv() else {
                return;
            };
            self.handle_event(event);
            if self.state.should_quit {
                return;
            }
        }
    }
}

/// `歌手 - 歌名`，用于状态栏与提示。
fn describe_song(song: &Song) -> String {
    let singers = song.singer_text();
    if singers == "未知歌手" {
        song.name.clone()
    } else {
        format!("{singers} - {}", song.name)
    }
}

/// 歌手的副标题。
///
/// 酷狗的歌手列表接口不填 `songcount`（实测恒为 0 或缺失），所以只在真的有数字时
/// 才显示「N 首」——否则整页都是误导性的「0 首」。
pub fn artist_subtitle(artist: &Artist) -> String {
    let mut parts = Vec::new();
    if let Some(songs) = artist.song_count.filter(|count| *count > 0) {
        parts.push(format!("{songs} 首"));
    }
    if let Some(fans) = artist.follower_count.filter(|count| *count > 0) {
        parts.push(format!("{fans} 粉丝"));
    }
    parts.join(" · ")
}

/// 歌单的副标题。
pub fn playlist_subtitle(playlist: &Playlist) -> String {
    let mut parts = Vec::new();
    if playlist.song_count > 0 {
        parts.push(format!("{} 首", playlist.song_count));
    }
    if let Some(creator) = playlist.creator.as_deref().filter(|name| !name.is_empty()) {
        parts.push(format!("by {creator}"));
    }
    if playlist.is_writable() {
        parts.push("可写".to_string());
    }
    parts.join(" · ")
}

/// 榜单的副标题。
pub fn rank_subtitle(board: &RankBoard) -> String {
    board
        .update_frequency
        .clone()
        .unwrap_or_else(|| format!("#{}", board.id))
}
