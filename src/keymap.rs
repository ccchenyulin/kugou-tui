//! 按键 → 语义动作的映射。
//!
//! 分成两套键表，由 [`KeyMode`] 选择：
//!
//! * [`KeyMode::Normal`] —— 列表浏览态，字母键是快捷键（`q` 退出、`n` 下一首……）。
//! * [`KeyMode::TextInput`] —— 输入框获得焦点，字母键必须原样插入文本，
//!   否则用户就没法搜索带 `q`、`j` 的歌名了。
//!
//! 之所以把「模式判断」放在这里而不是让 UI 层各自处理，是为了让快捷键表只有一份，
//! 帮助面板与真实行为不会漂移。

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMode {
    /// 浏览态：字母键触发快捷键。
    Normal,
    /// 输入态：字母键插入文本。
    TextInput,
}

/// 与具体按键解耦的语义动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // ---- 全局 ----
    /// 退出（会先保存配置与播放进度）。
    Quit,
    /// 强制退出，不保存。
    ForceQuit,
    /// 打开帮助面板。
    Help,

    // ---- 列表导航 ----
    MoveUp,
    MoveDown,
    MoveTop,
    MoveBottom,
    PageUp,
    PageDown,
    /// 在左侧导航栏与右侧内容区之间切换焦点。
    FocusNext,
    FocusPrev,
    /// 激活当前项 / 提交输入。
    Submit,
    /// 返回上一层 / 取消输入。
    Cancel,

    // ---- 文本编辑 ----
    Char(char),
    Backspace,
    Delete,
    CursorLeft,
    CursorRight,
    CursorHome,
    CursorEnd,

    // ---- 播放控制 ----
    PlayPause,
    Next,
    Prev,
    SeekForward,
    SeekBackward,
    VolumeUp,
    VolumeDown,
    ToggleMute,
    /// 顺序 → 单曲循环 → 随机 → 列表循环
    CyclePlaybackMode,
    /// 打开/关闭歌词面板。
    ToggleLyricPanel,
    /// 歌词整体延后 100ms。
    LyricDelay,
    /// 歌词整体提前 100ms。
    LyricAdvance,

    // ---- 业务 ----
    /// 打开搜索输入框。
    OpenSearch,
    /// 重新拉取当前视图数据。
    Reload,
    /// 把当前选中歌曲追加到播放队列。
    QueueAppend,
    /// 把当前列表**全部**追加到播放队列（数百首时用它，避免逐首添加。
    AddAllToQueue,
    /// 把当前选中歌曲插到下一首播放。
    QueuePlayNext,
    /// 把播放队列里选中的歌曲移出队列。
    RemoveFromQueue,
    /// 清空整个播放队列（需二次确认）。
    ClearQueue,
    /// 切换歌曲列表的排列方向：倒序（最后一首在最上）↔ 正序。
    ToggleSortOrder,
    /// 打开排行榜视图。
    OpenRanks,
    /// 打开云端歌单视图。
    OpenCloud,
    /// 扫码登录（应用内渲染二维码）。
    Login,
    /// 在歌手列表里轮换地区筛选：全部 → 华语 → 欧美 → 日韩 → 其他。
    CycleArtistFilter,
    /// 把当前播放队列同步到指定的云端歌单。
    SyncToCloud,
    /// 把选中歌曲加入云端歌单。
    AddToCloud,
    /// 把选中歌曲**从**云端歌单移除（按歌单条目的 fileid）。
    RemoveFromCloud,
    /// 删除（取消收藏）当前选中的云端歌单。
    DeleteCloudPlaylist,
    /// 弹出输入框，用输入的名字新建云端歌单。
    NewCloudPlaylist,
    /// 切换左侧导航栏的可见性（小窗口下腾出空间）。
    ToggleSidebar,
    /// 切换到下一个音源（酷狗 ↔ 网易云）。
    SwitchSource,
    /// 数字键切换顶层标签页。
    SwitchTab(u8),

    /// 未绑定。
    None,
}

/// 把一个按键事件翻译成语义动作。
pub fn resolve(key: KeyEvent, mode: KeyMode) -> Action {
    // Ctrl+C 在任何模式下都表示「立刻退出」。
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::ForceQuit;
    }

    match mode {
        KeyMode::TextInput => resolve_text_input(key),
        KeyMode::Normal => resolve_normal(key),
    }
}

/// 输入态键表：文本编辑 + 提交/取消 + 上下移动列表。
fn resolve_text_input(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => Action::Char(c),
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Delete => Action::Delete,
        KeyCode::Left => Action::CursorLeft,
        KeyCode::Right => Action::CursorRight,
        KeyCode::Home => Action::CursorHome,
        KeyCode::End => Action::CursorEnd,
        KeyCode::Enter => Action::Submit,
        KeyCode::Esc => Action::Cancel,
        // 单行输入框里上下键没有编辑含义。早期把它们映射成 `None`，结果是
        // 用户在输入态按 ↑/↓「完全没有反应」，误以为程序卡死。改为移动列表选中：
        // 边输入边用方向键挑结果，本来就该是顺手的操作。
        KeyCode::Up => Action::MoveUp,
        KeyCode::Down => Action::MoveDown,
        _ => Action::None,
    }
}

/// 浏览态键表。
fn resolve_normal(key: KeyEvent) -> Action {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    match key.code {
        // ---- 退出与帮助 ----
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('Q') => Action::ForceQuit,
        KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::F(1) => Action::Help,

        // ---- 导航：同时提供 vim 风格与方向键 ----
        KeyCode::Char('k') | KeyCode::Up => Action::MoveUp,
        KeyCode::Char('j') | KeyCode::Down => Action::MoveDown,
        KeyCode::Char('g') | KeyCode::Home => Action::MoveTop,
        KeyCode::Char('G') | KeyCode::End => Action::MoveBottom,
        KeyCode::PageUp => Action::PageUp,
        KeyCode::PageDown => Action::PageDown,
        KeyCode::Tab => Action::FocusNext,
        KeyCode::BackTab => Action::FocusPrev,
        KeyCode::Enter => Action::Submit,
        KeyCode::Esc => Action::Cancel,

        // ---- 播放控制 ----
        KeyCode::Char(' ') => Action::PlayPause,
        KeyCode::Char('n') => Action::Next,
        KeyCode::Char('p') => Action::Prev,
        // 左右方向键做 5 秒快进/快退，与大多数播放器一致
        KeyCode::Right => Action::SeekForward,
        KeyCode::Left => Action::SeekBackward,
        KeyCode::Char('+') | KeyCode::Char('=') => Action::VolumeUp,
        KeyCode::Char('-') => Action::VolumeDown,
        KeyCode::Char('m') => Action::ToggleMute,
        KeyCode::Char('r') => Action::CyclePlaybackMode,
        KeyCode::Char('l') => Action::ToggleLyricPanel,
        // 歌词微调：方括号在键盘上紧邻，符合直觉
        KeyCode::Char(']') => Action::LyricAdvance,
        KeyCode::Char('[') => Action::LyricDelay,

        // ---- 业务 ----
        KeyCode::Char('/') => Action::OpenSearch,
        KeyCode::Char('R') => Action::Reload,
        KeyCode::Char('a') => Action::QueueAppend,
        KeyCode::Char('A') => Action::AddAllToQueue,
        KeyCode::Char('i') => Action::QueuePlayNext,
        KeyCode::Char('x') => Action::RemoveFromQueue,
        KeyCode::Char('X') => Action::ClearQueue,
        KeyCode::Char('o') => Action::ToggleSortOrder,
        KeyCode::Char('b') => Action::OpenRanks,
        KeyCode::Char('c') => Action::OpenCloud,
        KeyCode::Char('L') => Action::Login,
        KeyCode::Char('f') => Action::CycleArtistFilter,
        KeyCode::Char('S') => Action::SyncToCloud,
        KeyCode::Char('s') => Action::AddToCloud,
        KeyCode::Char('d') => Action::RemoveFromCloud,
        KeyCode::Char('D') => Action::DeleteCloudPlaylist,
        KeyCode::Char('N') => Action::NewCloudPlaylist,
        KeyCode::Char('\\') => Action::ToggleSidebar,
        KeyCode::Char('v') => Action::SwitchSource,

        KeyCode::Char(digit @ '1'..='6') => Action::SwitchTab(digit as u8 - b'0'),
        // Shift 组合的字母键已被上面的显式分支吃掉，这里兜底避免误触发
        KeyCode::Char(_) if shift => Action::None,
        _ => Action::None,
    }
}

/// 供帮助面板展示的快捷键表：(按键, 说明, 分类)。
pub const CHEATSHEET: &[(&str, &str, &str)] = &[
    ("q / Q", "退出 / 强制退出", "全局"),
    ("? / F1", "打开本帮助", "全局"),
    ("\\", "显示/隐藏侧边栏", "全局"),
    ("v", "切换音源", "全局"),
    ("1..5", "切换顶层标签页", "全局"),
    ("Tab / S-Tab", "切换焦点区域", "导航"),
    ("j / k", "上下移动", "导航"),
    ("g / G", "跳到首行 / 末行", "导航"),
    ("PgUp / PgDn", "翻页", "导航"),
    ("Enter", "播放选中歌曲 / 进入", "导航"),
    ("Esc", "返回上一层", "导航"),
    ("/", "搜索", "业务"),
    ("R", "刷新当前列表", "业务"),
    ("a / i", "加入队列 / 下一首播放", "业务"),
    ("A", "把当前列表全部加入队列", "业务"),
    ("x / X", "移出队列 / 清空队列（需确认）", "业务"),
    ("o", "正序 ↔ 倒序（最后一首在最上）", "业务"),
    ("b / c", "排行榜 / 云端歌单", "业务"),
    ("L", "扫码登录", "业务"),
    ("f", "歌手地区筛选（在歌手页）", "业务"),
    ("s / S", "收藏到云端 / 同步整个队列", "业务"),
    ("d / D", "从云端歌单移除 / 删除歌单（需确认）", "业务"),
    ("N", "新建云端歌单", "业务"),
    ("Space", "播放 / 暂停", "播放"),
    ("n / p", "下一首 / 上一首", "播放"),
    ("← / →", "快退 / 快进 5 秒", "播放"),
    ("+ / -", "音量增减 5%", "播放"),
    ("m", "静音开关", "播放"),
    ("r", "循环播放模式", "播放"),
    ("l", "歌词面板开关", "播放"),
    ("[ / ]", "歌词延后 / 提前 100ms", "播放"),
];
