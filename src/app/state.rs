//! 应用状态。
//!
//! 状态是**纯数据**：不含网络句柄、不含渲染代码。`update` 负责改它，`ui` 负责读它。
//! 这条分界线让「按键 → 状态变化」和「状态 → 屏幕」都能单独推理。
//!
//! 列表选中态直接用 ratatui 的 [`ListState`]，因为滚动偏移需要跨帧保持——
//! 每帧重建状态会让长列表的滚动位置反复归零。

use ratatui::layout::Rect;
use ratatui::widgets::ListState;

use crate::api::model::{Artist, Lyric, Playlist, RankBoard, Song};
use crate::audio::engine::PlaybackState;
use crate::config::Config;

// ============================================================================
// 导航
// ============================================================================

/// 顶层标签页。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Search,
    Playlists,
    Artists,
    Ranks,
    Cloud,
    /// 音频可视化。不承载列表，整块主区都用来画实时频谱。
    Visualizer,
    /// 音源管理：启用/禁用、设默认、调优先级、查看可用状态。
    ///
    /// 刻意排在最后：前面 6 个的顺序是从第一版就定下来的，老用户已经形成
    /// 肌肉记忆，插队会让所有数字键错位。
    Sources,
}

impl Tab {
    pub const ALL: [Tab; 7] = [
        Tab::Search,
        Tab::Playlists,
        Tab::Artists,
        Tab::Ranks,
        Tab::Cloud,
        Tab::Visualizer,
        Tab::Sources,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Search => "搜索",
            Self::Playlists => "歌单",
            Self::Artists => "歌手",
            Self::Ranks => "排行榜",
            Self::Cloud => "云端",
            Self::Visualizer => "可视化",
            Self::Sources => "音源",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    /// 侧边栏与状态栏共用的「第 N 个标签」文本。
    pub fn position_text(self) -> String {
        format!("{}/{}", self.index() + 1, Self::ALL.len())
    }

    /// `1`..`6` 数字键（与 `Tab::ALL` 长度保持一致）。
    pub fn from_number(number: u8) -> Option<Self> {
        Self::ALL.get(number.checked_sub(1)? as usize).copied()
    }
}

/// 当前获得按键的面板。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    /// 左侧标签/状态栏。
    Sidebar,
    /// 主区域：搜索输入框，或歌单/歌手/榜单的条目列表。
    #[default]
    Primary,
    /// 次区域：歌曲列表。
    Secondary,
    /// 播放队列。
    Queue,
}

/// 鼠标可命中的区域。
///
/// 渲染时由 ui 层回填。主循环拿它把「屏幕坐标」翻译成「第几行」，从而支持
/// 点击选中、双击激活、滚轮翻页。之所以放在状态里而不是 ui 层内部，是因为
/// 事件处理在 app 层，必须能读到本帧的布局结果。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    /// 侧边栏标签，值为标签下标（0-4）。
    Tab(usize),
    /// 主区的「条目列表」（歌单/歌手/榜单）。
    Entries,
    /// 主区的「歌曲列表」。
    Songs,
    /// 播放队列。
    Queue,
    /// 播放条的进度条区域：点击可跳转进度。
    Progress,
}

#[derive(Debug, Clone, Copy)]
pub struct HitZone {
    pub rect: Rect,
    pub target: HitTarget,
    /// 该列表当前显示的第一行数据下标（来自 ListState 的 offset）。
    pub first_index: usize,
    /// 列表总长度，防止点击到列表末尾之外的空白行时越界。
    pub len: usize,
}

impl HitZone {
    pub fn contains(&self, column: u16, row: u16) -> bool {
        self.rect
            .contains(ratatui::layout::Position::new(column, row))
    }

    /// 屏幕行号 → 数据下标。超出列表范围返回 None。
    pub fn index_at(&self, row: u16) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        let offset = row.saturating_sub(self.rect.top()) as usize;
        Some((self.first_index + offset).min(self.len - 1))
    }
}

/// 状态栏消息级别，决定配色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusLevel {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

// ============================================================================
// 通用控件状态
// ============================================================================

/// 单行文本输入。
///
/// `cursor` 是**字符**下标而不是字节下标——中文歌名很常见，用字节下标会在
/// 中间截断 UTF-8 导致 panic。
#[derive(Debug, Default, Clone)]
pub struct TextInput {
    buffer: String,
    cursor: usize,
}

impl TextInput {
    pub fn text(&self) -> &str {
        &self.buffer
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// 光标处的字节偏移，供渲染时定位终端光标。
    pub fn cursor_byte_index(&self) -> usize {
        self.byte_index()
    }

    pub fn set(&mut self, text: impl Into<String>) {
        self.buffer = text.into();
        self.cursor = self.buffer.chars().count();
    }

    pub fn insert(&mut self, character: char) {
        // 换行与控制字符会破坏单行输入框的假设
        if character.is_control() {
            return;
        }
        let index = self.byte_index();
        self.buffer.insert(index, character);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let start = self.byte_index();
        let width = self.buffer[start..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or_default();
        self.buffer.replace_range(start..start + width, "");
    }

    pub fn delete(&mut self) {
        let start = self.byte_index();
        let Some(character) = self.buffer[start..].chars().next() else {
            return;
        };
        self.buffer
            .replace_range(start..start + character.len_utf8(), "");
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.buffer.chars().count());
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.buffer.chars().count();
    }

    fn byte_index(&self) -> usize {
        self.buffer
            .char_indices()
            .nth(self.cursor)
            .map(|(index, _)| index)
            .unwrap_or(self.buffer.len())
    }
}

/// 条目列表（歌单 / 歌手 / 排行榜），带持久化的滚动偏移。
#[derive(Debug)]
pub struct EntryList<T> {
    pub entries: Vec<T>,
    pub cursor: ListState,
    pub loading: bool,
}

impl<T> Default for EntryList<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            cursor: ListState::default(),
            loading: false,
        }
    }
}

impl<T> EntryList<T> {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 直接选中第 `index` 项（鼠标点击用）。下标越界时钳到末尾。
    pub fn select(&mut self, index: usize) {
        if self.entries.is_empty() {
            self.cursor.select(None);
            return;
        }
        self.cursor.select(Some(index.min(self.entries.len() - 1)));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.cursor
            .selected()
            .filter(|index| *index < self.entries.len())
    }

    pub fn selected(&self) -> Option<&T> {
        self.selected_index()
            .and_then(|index| self.entries.get(index))
    }

    /// 用新数据替换列表，并把选中项复位到第一行。
    pub fn replace(&mut self, entries: Vec<T>) {
        self.entries = entries;
        self.loading = false;
        if self.entries.is_empty() {
            self.cursor.select(None);
        } else {
            self.cursor.select(Some(0));
        }
    }

    pub fn move_by(&mut self, delta: isize) {
        move_selection(&mut self.cursor, self.entries.len(), delta);
    }

    pub fn select_first(&mut self) {
        select_first(&mut self.cursor, self.entries.len());
    }

    pub fn select_last(&mut self) {
        select_last(&mut self.cursor, self.entries.len());
    }
}

/// 歌曲列表。
#[derive(Debug, Default)]
pub struct SongList {
    /// 列表标题，形如「歌单名 · 128 首」。
    pub title: String,
    pub songs: Vec<Song>,
    pub cursor: ListState,
    pub loading: bool,
    /// 空列表时的提示语。
    ///
    /// 各标签页空态的原因不同（没搜过 / 还没载入 / 筛选无结果），一律显示
    /// 「暂无数据」会让用户不知道下一步该按什么。
    pub empty_hint: String,
}

impl SongList {
    /// 设置空态提示语。载入数据不会覆盖它。
    pub fn set_empty_hint(&mut self, hint: impl Into<String>) {
        self.empty_hint = hint.into();
    }

    /// 当前应显示的空态文案。
    pub fn empty_text(&self) -> &str {
        if self.empty_hint.is_empty() {
            "暂无数据"
        } else {
            &self.empty_hint
        }
    }

    pub fn is_empty(&self) -> bool {
        self.songs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.songs.len()
    }

    /// 反转顺序，并让光标仍停在同一首歌上（下标取镜像）。
    ///
    /// 这里反转的是**存储**，因此播放队列与界面顺序永远一致。
    pub fn toggle_sort(&mut self) {
        self.songs.reverse();
        let len = self.songs.len();
        if len == 0 {
            self.cursor.select(None);
            return;
        }
        if let Some(index) = self.cursor.selected() {
            self.cursor.select(Some((len - 1 - index).min(len - 1)));
        }
    }

    /// 按当前排序方向整理新载入的歌曲。
    pub fn set_songs_sorted(
        &mut self,
        title: impl Into<String>,
        mut songs: Vec<Song>,
        sort_descending: bool,
    ) {
        if sort_descending {
            songs.reverse();
        }
        self.replace(title, songs);
    }

    pub fn select(&mut self, index: usize) {
        if self.songs.is_empty() {
            self.cursor.select(None);
            return;
        }
        self.cursor.select(Some(index.min(self.songs.len() - 1)));
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.cursor
            .selected()
            .filter(|index| *index < self.songs.len())
    }

    pub fn selected(&self) -> Option<&Song> {
        self.selected_index()
            .and_then(|index| self.songs.get(index))
    }

    pub fn replace(&mut self, title: impl Into<String>, songs: Vec<Song>) {
        self.title = title.into();
        self.songs = songs;
        self.loading = false;
        if self.songs.is_empty() {
            self.cursor.select(None);
        } else {
            self.cursor.select(Some(0));
        }
    }

    pub fn move_by(&mut self, delta: isize) {
        move_selection(&mut self.cursor, self.songs.len(), delta);
    }

    pub fn select_first(&mut self) {
        select_first(&mut self.cursor, self.songs.len());
    }

    pub fn select_last(&mut self) {
        select_last(&mut self.cursor, self.songs.len());
    }
}

/// 选中项移动，自动钳位到合法范围。
///
/// 还没有选中项时（列表刚加载完），向下移动落在首行、向上移动落在末行——
/// 这比「从 0 再位移」更符合直觉：按一下 `j` 就应该选中第一项。
pub fn move_selection(state: &mut ListState, len: usize, delta: isize) {
    if len == 0 {
        state.select(None);
        return;
    }

    let next = match state.selected() {
        None => {
            if delta >= 0 {
                0
            } else {
                len - 1
            }
        }
        Some(current) => (current as isize + delta).clamp(0, len as isize - 1) as usize,
    };

    state.select(Some(next));
}

pub fn select_first(state: &mut ListState, len: usize) {
    state.select((len > 0).then_some(0));
}

pub fn select_last(state: &mut ListState, len: usize) {
    state.select((len > 0).then(|| len - 1));
}

// ============================================================================
// 各标签页状态
// ============================================================================

/// 搜索页：输入框 + 结果列表。
#[derive(Debug, Default)]
pub struct SearchPane {
    pub input: TextInput,
    /// 输入框是否正在接收字符。
    pub editing: bool,
    /// 上次真正提交搜索的关键词，用于提示「结果对应的是哪个词」。
    pub submitted: String,
    pub results: SongList,
    /// 已加载到第几页。「加载更多」在此基础上 +1。
    ///
    /// 搜索刻意分页：酷狗只有第 1 页是精确匹配，深页是兜底内容，
    /// 一次取全会把相关结果淹没。想要更多就一页页追加。
    pub page: u32,
}

#[derive(Debug, Default)]
pub struct PlaylistPane {
    pub list: EntryList<Playlist>,
    pub songs: SongList,
    /// 歌单广场分类 id，`0` 为推荐。
    pub category: i64,
}

#[derive(Debug, Default)]
pub struct ArtistPane {
    pub list: EntryList<Artist>,
    pub songs: SongList,
    /// 歌手分类：0 全部 / 1 华语 / 2 欧美 / 3 日韩。
    pub kind: i64,
}

#[derive(Debug, Default)]
pub struct RankPane {
    pub list: EntryList<RankBoard>,
    pub songs: SongList,
}

#[derive(Debug, Default)]
pub struct CloudPane {
    pub list: EntryList<Playlist>,
    pub songs: SongList,
}

/// 当前封面的字符画。
#[derive(Debug, Default, Clone)]
pub struct CoverArt {
    /// 封面属于哪首歌（用 hash 标识）。`None` 表示还没有封面。
    pub hash: Option<String>,
    /// 半块字符画，每行等宽。**非 kitty 终端的兜底**。
    pub lines: Vec<String>,
    /// 原图的 PNG 字节。kitty 终端用它按原样显示（1:1 还原），
    /// 字符画只是画不出真图时的降级方案。
    pub png: Option<Vec<u8>>,
}

impl CoverArt {
    /// 是否属于这首歌。
    pub fn belongs_to(&self, hash: &str) -> bool {
        self.hash.as_deref() == Some(hash)
    }
}

/// 歌词面板状态。
#[derive(Debug, Default)]
pub struct LyricPane {
    /// 当前歌词属于哪首歌，用于丢弃过期的异步结果。
    pub hash: Option<String>,
    pub lyric: Lyric,
    pub loading: bool,
    /// 已渲染过的当前行下标，用于只在换行时重新计算居中偏移。
    pub active_line: Option<usize>,
}

// ============================================================================
// 根状态
// ============================================================================

#[derive(Debug)]
pub struct AppState {
    // ---- 配置与连接 ----
    pub config: Config,
    /// 是否已配置登录 cookie。未登录时云端功能不可用。
    pub logged_in: bool,

    // ---- 界面 ----
    pub tab: Tab,
    pub focus: Focus,
    pub sidebar_visible: bool,
    pub show_help: bool,
    pub show_lyric_panel: bool,
    pub should_quit: bool,
    /// 强制退出：跳过配置保存。
    pub force_quit: bool,

    // ---- 各标签页 ----
    pub search: SearchPane,
    pub playlists: PlaylistPane,
    pub artists: ArtistPane,
    pub ranks: RankPane,
    pub cloud: CloudPane,

    // ---- 播放 ----
    pub queue: crate::app::queue::PlayQueue,
    pub queue_cursor: ListState,
    /// 音源管理页里选中的行。
    pub sources_cursor: ListState,
    pub current: Option<Song>,
    /// 当前播放的是否为试听片段。
    ///
    /// 片段播完和整首播完必须区别对待：片段结束是「没权限」，不该被当成正常结束
    /// 而自动跳下一首——否则用户只会看到「听几十秒就跳歌」，不知道是会员没生效。
    pub current_is_trial: bool,
    pub playback: PlaybackState,
    pub position_ms: u64,
    pub duration_ms: u64,
    /// 鼠标当前位置（列, 行）。终端支持鼠标移动上报时才有值，用于悬停反馈。
    pub hover: Option<(u16, u16)>,

    /// 播放电平（0.0~1.0），来自音频线程的真实采样峰值。会随播放逐帧刷新。
    pub levels: Vec<f32>,
    /// 平滑后的电平。原始电平每 ~15ms 跳一次，直接画会明显抖动；这里做
    /// 「快起慢落」的缓动后，柱子才跟手又不抖。
    pub smooth_levels: Vec<f32>,
    /// 峰值保持：柱顶那条刻度线的高度，比柱子本身落得慢，形成经典频谱的观感。
    pub peak_levels: Vec<f32>,
    /// 当前音量，`0.0 ~ 1.0`。
    pub volume: f32,
    /// 静音前的音量，用于 `m` 键还原。
    pub volume_before_mute: Option<f32>,
    pub lyric: LyricPane,

    // ---- 云端同步 ----
    /// 同步目标歌单。在云端标签页选中歌单时自动设置。
    pub sync_target: Option<Playlist>,

    // ---- 状态栏 ----
    pub status: String,
    pub status_level: StatusLevel,
    /// 正在进行的后台任务描述，非空时状态栏显示进度指示。
    pub busy: Option<String>,
    /// 下载进度 `(已下载, 总大小)`。
    pub download_progress: Option<(u64, Option<u64>)>,
    /// 音频缓存已占用字节数。定期测量，不在每帧做目录扫描。
    pub cache_bytes: u64,
    /// 心跳计数，用于把「每 N 拍做一次」的低频任务错开。
    pub ticks: u64,
    /// 本帧的鼠标命中区，由 ui 层每帧清空后回填。
    pub hit_zones: Vec<HitZone>,
    /// 歌曲列表的排列顺序。
    ///
    /// `true` = 倒序，即**最后一首排在最上面**（`o` 键切换）。之所以把它做成状态而不是
    /// 在渲染时反转：反转后的顺序必须与播放队列一致，就地反转存储，所有现有的下标逻辑
    /// （选中、播放、鼠标点击）都不用改，不会出现「看到的顺序和播放的顺序不一致」。
    pub sort_descending: bool,
    /// 待确认的危险操作。非空时按键先走确认流程，避免误按一下就清空整个队列。
    pub pending_confirm: Option<ConfirmAction>,
    /// 应用内扫码登录；`None` 表示未在进行登录。
    pub login: Option<LoginState>,
    /// 登录前的音源选择器。非空时它是模态的，会拦截所有按键。
    pub login_picker: Option<LoginPicker>,
    /// 文本输入弹窗；`None` 表示没有弹出的输入框。
    pub prompt: Option<PromptState>,
    /// 当前封面的字符画，以及它属于哪首歌（避免切歌后继续显示上一张）。
    pub cover: CoverArt,
    /// 本帧封面占用的字符区域。渲染时记录，渲染**之后**由 kitty 协议把真图
    /// 画上去——图片是终端浮层，必须在 ratatui 绘制完成后再放。
    pub cover_area: Option<ratatui::layout::Rect>,
    /// 当前账号的会员摘要（如「概念版 SVIP · 至 09-21」），未登录或未取到时为 None。
    pub vip_label: Option<String>,
    /// 上次鼠标点击命中的（区域, 数据下标）。
    last_click: Option<(HitTarget, Option<usize>)>,
    last_click_at: std::time::Instant,
}

/// 登录时的音源选择器。
///
/// 多个音源都能登录后，「按 L 登录哪个」就成了必须回答的问题——登录态是
/// 按音源分开存的，登录前必须选定目标，否则凭据会存错地方。
#[derive(Debug, Clone, Default)]
pub struct LoginPicker {
    /// 候选音源（只列支持登录的）。
    pub candidates: Vec<crate::source::SourceKind>,
    pub cursor: ListState,
}

impl LoginPicker {
    /// 当前选中的音源。
    pub fn selected(&self) -> Option<crate::source::SourceKind> {
        let index = self.cursor.selected().unwrap_or(0);
        self.candidates.get(index).copied()
    }

    /// 上下移动选中。
    pub fn move_by(&mut self, delta: isize) {
        if self.candidates.is_empty() {
            return;
        }
        let len = self.candidates.len();
        let current = self.cursor.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, len as isize - 1) as usize;
        self.cursor.select(Some(next));
    }
}

/// 应用内扫码登录的状态。
#[derive(Debug, Clone, Default)]
pub struct LoginState {
    /// 二维码的显示行（每模块两字符宽以修正终端字符的高宽比）。
    pub qr: Vec<String>,
    /// 二维码 key，轮询时用。
    pub key: String,
    /// 当前提示语。
    pub message: String,
    /// 是否已结束（成功或失败）。结束后不再轮询。
    pub finished: bool,
    pub succeeded: bool,
}

impl LoginState {
    /// 弹窗所需的高度：二维码高度 + 提示与内边距。
    pub fn dialog_height(&self) -> u16 {
        let qr_height = self.qr.len() as u16;
        if qr_height == 0 { 5 } else { qr_height + 4 }
    }

    /// 弹窗所需的宽度。
    pub fn dialog_width(&self) -> u16 {
        let qr_width = self
            .qr
            .first()
            .map(|line| line.chars().count())
            .unwrap_or(0) as u16;
        if qr_width == 0 {
            44
        } else {
            qr_width.max(40) + 4
        }
    }
}

/// 需要二次确认的操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    /// 清空播放队列。
    ClearQueue,
    /// 删除（取消收藏）一个云端歌单。
    DeleteCloudPlaylist,
    /// 清空音频缓存目录。
    ClearCache,
    /// 已有登录态时再按 `L`：重新扫码会覆盖现有凭据。
    Relogin,
}

impl ConfirmAction {
    pub fn question(self) -> &'static str {
        match self {
            Self::ClearQueue => "清空整个播放队列并停止播放？",
            Self::DeleteCloudPlaylist => "删除这个云端歌单？该操作会取消收藏它。",
            Self::ClearCache => "清空音频缓存？已缓存的歌曲需要重新下载。",
            Self::Relogin => "已登录。重新扫码会覆盖当前凭据，确定要重新登录？",
        }
    }

    pub fn hint(self) -> &'static str {
        "Enter 确认 · Esc 取消"
    }
}

/// 文本输入弹窗。用于「新建歌单」这类需要用户输入名字的操作。
#[derive(Debug, Clone)]
pub struct PromptState {
    pub title: String,
    pub buffer: String,
    pub action: PromptAction,
}

/// 文本输入弹窗提交后的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptAction {
    /// 用输入的名字新建云端歌单。
    CreateCloudPlaylist,
}

impl PromptState {
    pub fn new(title: impl Into<String>, action: PromptAction) -> Self {
        Self {
            title: title.into(),
            buffer: String::new(),
            action,
        }
    }
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let logged_in = config.is_logged_in();
        let volume = config.volume;
        let playback_mode = config.playback_mode;

        let mut state = Self {
            config,
            logged_in,
            tab: Tab::default(),
            focus: Focus::default(),
            sidebar_visible: true,
            show_help: false,
            show_lyric_panel: true,
            should_quit: false,
            force_quit: false,
            search: SearchPane::default(),
            playlists: PlaylistPane::default(),
            artists: ArtistPane::default(),
            ranks: RankPane::default(),
            cloud: CloudPane::default(),
            queue: crate::app::queue::PlayQueue::new(playback_mode),
            queue_cursor: ListState::default(),
            sources_cursor: ListState::default(),
            current: None,
            current_is_trial: false,
            playback: PlaybackState::Stopped,
            position_ms: 0,
            hover: None,
            levels: Vec::new(),
            smooth_levels: Vec::new(),
            peak_levels: Vec::new(),
            duration_ms: 0,
            volume,
            volume_before_mute: None,
            lyric: LyricPane::default(),
            cover: CoverArt::default(),
            cover_area: None,
            sync_target: None,
            status: "按 / 搜索，或按 2-5 浏览歌单/歌手/排行榜/云端".to_string(),
            status_level: StatusLevel::Info,
            busy: None,
            download_progress: None,
            cache_bytes: 0,
            ticks: 0,
            hit_zones: Vec::new(),
            last_click: None,
            last_click_at: std::time::Instant::now(),
            sort_descending: true,
            pending_confirm: None,
            login: None,
            login_picker: None,
            prompt: None,
            vip_label: None,
        };

        state.apply_empty_hints();
        state
    }

    /// 供 ui 层在每帧开始时调用：清掉上一帧的命中区。
    pub fn begin_frame(&mut self) {
        self.hit_zones.clear();
    }

    /// ui 层登记一个可命中区域。
    pub fn add_hit_zone(&mut self, rect: Rect, target: HitTarget, first_index: usize, len: usize) {
        self.hit_zones.push(HitZone {
            rect,
            target,
            first_index,
            len,
        });
    }

    /// 找出包含指定屏幕坐标的命中区（后登记的优先，这样小区域能覆盖大区域）。
    pub fn hit_test(&self, column: u16, row: u16) -> Option<HitZone> {
        self.hit_zones
            .iter()
            .rev()
            .find(|zone| zone.contains(column, row))
            .copied()
    }

    /// 上一次鼠标点击命中的（区域, 数据下标），用于识别双击。
    pub fn set_last_click(&mut self, target: HitTarget, index: Option<usize>) {
        self.last_click = Some((target, index));
        self.last_click_at = std::time::Instant::now();
    }

    /// 判断本次点击是否是双击（同一行、500ms 内）。
    pub fn is_double_click(&self, target: HitTarget, index: Option<usize>) -> bool {
        match self.last_click {
            Some((last_target, last_index)) => {
                last_target == target
                    && last_index == index
                    && self.last_click_at.elapsed() < std::time::Duration::from_millis(500)
            }
            None => false,
        }
    }

    /// 给各标签页的歌曲列表设置空态提示语。
    ///
    /// 启动时停在搜索页且结果为空——此时按上下键本来就无事可做，如果界面只写
    /// 「暂无数据」，用户既看不出「按键无效」还是「没有内容」，也不知道下一步
    /// 该按什么。把下一步动作直接写在空态里，是成本最低的可用性修复。
    fn apply_empty_hints(&mut self) {
        self.search
            .results
            .set_empty_hint("按 / 输入关键词，Enter 开始搜索");
        self.playlists
            .songs
            .set_empty_hint("选中歌单后按 Enter 载入歌曲");
        self.artists
            .songs
            .set_empty_hint("选中歌手后按 Enter 载入歌曲");
        self.ranks
            .songs
            .set_empty_hint("选中榜单后按 Enter 载入歌曲");
        self.cloud
            .songs
            .set_empty_hint("选中云端歌单后按 Enter 载入歌曲");
    }

    /// 选中「条目列表」（歌单/歌手/榜单）的第 `index` 项。
    pub fn select_entry_index(&mut self, index: usize) {
        match self.tab {
            Tab::Playlists => self.playlists.list.select(index),
            Tab::Artists => self.artists.list.select(index),
            Tab::Ranks => self.ranks.list.select(index),
            Tab::Cloud => self.cloud.list.select(index),
            // 搜索页、可视化页、音源页都没有条目列表
            Tab::Search | Tab::Visualizer | Tab::Sources => {}
        }
    }

    /// 当前标签页的歌曲列表。可视化页没有列表，返回 `None`。
    ///
    /// # 为什么要有这一层
    ///
    /// 「当前标签页该操作哪个歌曲列表」的分派原先散落在 6 处（`state.rs` 的
    /// `select_song_index` / `current_tab_songs`、`ui/mod.rs` 的 `songs_offset` /
    /// `songs_len`、`update.rs` 的 `move_selection` / `move_selection_edge`），
    /// 每处都是一份同样的 6 分支 match。新增标签页时漏改一处，表现就是「这个页里
    /// 方向键没反应」——可视化页就踩过一次。收敛到这里之后只剩这一份。
    pub fn songs(&self) -> Option<&SongList> {
        match self.tab {
            Tab::Search => Some(&self.search.results),
            Tab::Playlists => Some(&self.playlists.songs),
            Tab::Artists => Some(&self.artists.songs),
            Tab::Ranks => Some(&self.ranks.songs),
            Tab::Cloud => Some(&self.cloud.songs),
            Tab::Visualizer | Tab::Sources => None,
        }
    }

    /// 同上，可变版本。
    pub fn songs_mut(&mut self) -> Option<&mut SongList> {
        match self.tab {
            Tab::Search => Some(&mut self.search.results),
            Tab::Playlists => Some(&mut self.playlists.songs),
            Tab::Artists => Some(&mut self.artists.songs),
            Tab::Ranks => Some(&mut self.ranks.songs),
            Tab::Cloud => Some(&mut self.cloud.songs),
            Tab::Visualizer | Tab::Sources => None,
        }
    }

    /// 选中「歌曲列表」的第 `index` 项。
    pub fn select_song_index(&mut self, index: usize) {
        if let Some(list) = self.songs_mut() {
            list.select(index);
        }
    }

    /// 写入状态栏消息。
    pub fn notify(&mut self, level: StatusLevel, message: impl Into<String>) {
        self.status = message.into();
        self.status_level = level;
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.notify(StatusLevel::Info, message);
    }

    pub fn success(&mut self, message: impl Into<String>) {
        self.notify(StatusLevel::Success, message);
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.notify(StatusLevel::Warning, message);
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.notify(StatusLevel::Error, message);
    }

    /// 当前是否处于「输入框吃字符」的状态。
    pub fn is_editing(&self) -> bool {
        self.search.editing
    }

    /// 播放进度，`0.0 ~ 1.0`。时长为 0 时返回 0，避免除零。
    pub fn progress_ratio(&self) -> f64 {
        if self.duration_ms == 0 {
            return 0.0;
        }
        (self.position_ms as f64 / self.duration_ms as f64).clamp(0.0, 1.0)
    }

    pub fn is_muted(&self) -> bool {
        self.volume <= f32::EPSILON
    }

    /// 状态栏右侧的忙碌指示文本，附带下载百分比。
    pub fn busy_label(&self) -> Option<String> {
        let busy = self.busy.as_ref()?;
        match self.download_progress {
            Some((received, Some(total))) if total > 0 => {
                let percent = (received as f64 / total as f64 * 100.0).min(100.0);
                Some(format!("{busy} {percent:.0}%"))
            }
            // 服务端没给 Content-Length，或者长度为 0：退回按字节数显示
            Some((received, _)) => Some(format!("{busy} {:.1} MiB", received as f64 / 1_048_576.0)),
            None => Some(busy.clone()),
        }
    }

    /// 当前焦点面板对应的歌曲列表（如果该面板确实在展示歌曲）。
    pub fn focused_songs(&self) -> Option<&SongList> {
        match (self.tab, self.focus) {
            (Tab::Search, Focus::Primary | Focus::Secondary) => Some(&self.search.results),
            (Tab::Playlists, Focus::Secondary) => Some(&self.playlists.songs),
            (Tab::Artists, Focus::Secondary) => Some(&self.artists.songs),
            (Tab::Ranks, Focus::Secondary) => Some(&self.ranks.songs),
            (Tab::Cloud, Focus::Secondary) => Some(&self.cloud.songs),
            _ => None,
        }
    }

    /// 在当前标签页里按 `Tab` 轮转焦点。
    ///
    /// 只把「本页确实可见」的面板纳入轮转，避免焦点跑到看不见的地方。
    pub fn cycle_focus(&mut self, forward: bool) {
        let mut candidates = vec![Focus::Sidebar];
        candidates.push(Focus::Primary);
        if self
            .current_tab_songs()
            .is_some_and(|songs| !songs.is_empty())
        {
            candidates.push(Focus::Secondary);
        }
        if !self.queue.is_empty() {
            candidates.push(Focus::Queue);
        }

        let current = candidates
            .iter()
            .position(|focus| *focus == self.focus)
            .unwrap_or(0);

        let next = if forward {
            (current + 1) % candidates.len()
        } else {
            (current + candidates.len() - 1) % candidates.len()
        };

        self.focus = candidates[next];
        // 离开搜索框时必须退出编辑态，否则字母键会被继续吞掉
        if self.focus != Focus::Primary || self.tab != Tab::Search {
            self.search.editing = false;
        }
    }

    /// 当前标签页的歌曲列表。可视化页没有列表，返回 `None`。
    fn current_tab_songs(&self) -> Option<&SongList> {
        self.songs()
    }

    /// 推进可视化动画：把原始电平做「快起慢落」的缓动，并维护峰值刻度。
    ///
    /// 按**真实经过时间**做指数平滑，而不是按帧数。这样帧率变化（例如从省电的
    /// 5fps 切到 30fps）时，柱子的快慢观感保持一致，不会出现「帧率低就掉得慢」。
    pub fn advance_visualizer(&mut self, elapsed: std::time::Duration) {
        if self.levels.is_empty() {
            self.smooth_levels.clear();
            self.peak_levels.clear();
            return;
        }

        let count = self.levels.len();
        self.smooth_levels.resize(count, 0.0);
        self.peak_levels.resize(count, 0.0);

        // 夹一下：切标签页或卡顿后 elapsed 可能很大，避免一帧跳到底
        let seconds = elapsed.as_secs_f32().clamp(0.0, 0.5);
        // 起振 40ms、回落 300ms。起振要跟手（鼓点一响就上去了），回落落在
        // 200–400ms 这个区间内才显得从容——再快就抖，再慢就像卡住。
        let attack = 1.0 - (-seconds / 0.04).exp();
        let decay = 1.0 - (-seconds / 0.30).exp();
        // 峰值刻度每秒下落 50%（约 2 秒落到底），比柱子慢得多，才有频谱仪的余韵
        let peak_fall = seconds * 0.5;

        for index in 0..count {
            let target = self.levels[index].clamp(0.0, 1.0);
            let current = self.smooth_levels[index];
            let factor = if target > current { attack } else { decay };
            let next = current + (target - current) * factor;
            self.smooth_levels[index] = next;

            let peak = self.peak_levels[index];
            self.peak_levels[index] = (peak - peak_fall).max(next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> Song {
        Song {
            name: name.to_string(),
            hash: name.to_string(),
            ..Song::default()
        }
    }

    #[test]
    fn songs_are_descending_by_default() {
        // 进入歌单后默认「从最后一首开始」：载入时按排列方向反转存储
        let mut list = SongList::default();
        list.set_songs_sorted("t", vec![named("a"), named("b"), named("c")], true);
        let names: Vec<&str> = list.songs.iter().map(|song| song.name.as_str()).collect();
        assert_eq!(names, vec!["c", "b", "a"]);
    }

    #[test]
    fn toggle_sort_mirrors_order_and_cursor() {
        let mut list = SongList::default();
        list.replace("t", vec![named("a"), named("b"), named("c")]);
        list.select(0); // 选中最上面那首 a

        list.toggle_sort();

        assert_eq!(list.songs[0].name, "c", "反转后最后一首排在最上面");
        assert_eq!(
            list.selected_index(),
            Some(2),
            "光标应镜像到另一端，仍指向同一首歌"
        );
    }

    #[test]
    fn text_input_handles_multibyte_backspace() {
        let mut input = TextInput::default();
        input.set("海阔天空");
        assert_eq!(input.cursor_byte_index(), 12, "光标应停在末尾");

        // 一次退格只删一个汉字，不能把 UTF-8 字节切一半
        input.backspace();
        assert_eq!(input.text(), "海阔天");
        assert_eq!(input.cursor_byte_index(), 9);
    }

    #[test]
    fn text_input_inserts_at_cursor_position() {
        let mut input = TextInput::default();
        input.set("ac");
        input.move_home();
        input.move_right();
        input.insert('b');
        assert_eq!(input.text(), "abc");
        assert_eq!(input.cursor_byte_index(), 2);
    }

    #[test]
    fn text_input_delete_removes_character_under_cursor() {
        let mut input = TextInput::default();
        input.set("abc");
        input.move_home();
        input.delete();
        assert_eq!(input.text(), "bc");
    }

    #[test]
    fn text_input_ignores_control_characters() {
        let mut input = TextInput::default();
        input.insert('\n');
        input.insert('\t');
        assert!(input.is_empty());
    }

    #[test]
    fn text_input_cursor_never_exceeds_length() {
        let mut input = TextInput::default();
        input.set("ab");
        for _ in 0..10 {
            input.move_right();
        }
        assert_eq!(input.cursor_byte_index(), 2);
        input.insert('c');
        assert_eq!(input.text(), "abc");
    }

    #[test]
    fn move_selection_clamps_at_both_ends() {
        let mut state = ListState::default();
        move_selection(&mut state, 3, 1);
        assert_eq!(state.selected(), Some(0));
        move_selection(&mut state, 3, -5);
        assert_eq!(state.selected(), Some(0));
        move_selection(&mut state, 3, 99);
        assert_eq!(state.selected(), Some(2));
    }

    #[test]
    fn move_selection_on_empty_list_clears_selection() {
        let mut state = ListState::default();
        state.select(Some(5));
        move_selection(&mut state, 0, 1);
        assert_eq!(state.selected(), None);
    }

    #[test]
    fn tab_numbers_map_to_tabs() {
        assert_eq!(Tab::from_number(1), Some(Tab::Search));
        assert_eq!(Tab::from_number(5), Some(Tab::Cloud));
        assert_eq!(Tab::from_number(0), None);
        assert_eq!(Tab::from_number(9), None);
    }

    #[test]
    fn progress_ratio_avoids_division_by_zero() {
        let mut state = AppState::new(Config::default());
        assert_eq!(state.progress_ratio(), 0.0);
        state.duration_ms = 100;
        state.position_ms = 250;
        assert!((state.progress_ratio() - 1.0).abs() < f64::EPSILON);
    }
}
