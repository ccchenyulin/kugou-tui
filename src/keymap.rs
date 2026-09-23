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

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

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
    /// 打开当前歌曲的右键菜单（等同鼠标右键）。
    ContextMenu,

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
    /// 相对跳转指定毫秒（正负皆可）。
    ///
    /// MPRIS 的 `Seek` 给的是任意大小的偏移量，不能用「N 次 ±5 秒」去凑——
    /// 拖一次 1 小时的进度条会往事件队列里灌 720 条消息，主循环单帧只消费
    /// 64 条，界面会明显卡住。
    SeekBy(i64),
    /// 搜索结果「加载更多」：追加下一页。
    LoadMoreSearch,
    /// 绝对定位到指定毫秒。MPRIS 的 SetPosition 需要它——桌面组件拖进度条是"跳到某处"，
    /// 不是"前进/后退几秒"，只有 SeekForward/Backward 是做不到的。
    SeekTo(u64),
    VolumeUp,
    VolumeDown,
    ToggleMute,
    /// 顺序 → 单曲循环 → 随机 → 列表循环
    CyclePlaybackMode,
    /// 在支持的音质档位之间循环（下一首生效）。
    CycleQuality,
    /// 打开/关闭歌词面板。
    ToggleLyricPanel,
    /// 歌词整体延后 100ms。
    LyricDelay,
    /// 歌词整体提前 100ms。
    LyricAdvance,

    // ---- 业务 ----
    /// 打开搜索输入框。
    OpenSearch,
    /// 打开设置页（直达键；它同时也够得到数字键 9）。
    OpenSettings,
    /// 下载当前播放歌曲到设置页里选的目录。
    DownloadCurrent,
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
    /// 清空音频缓存目录。
    ClearCache,
    /// 切换歌曲列表的排列方向：倒序（最后一首在最上）↔ 正序。
    ToggleSortOrder,
    /// 打开排行榜视图。
    OpenRanks,
    /// 打开云端歌单视图。
    OpenCloud,
    /// 扫码登录（应用内渲染二维码）。
    Login,
    /// 领取「概念版」当天 VIP（领一天 → 升级成畅听 VIP）。
    ClaimVip,
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
    /// 打开音源管理页。
    SwitchSource,
    /// 音源管理页：把选中音源设为默认。
    SetDefaultSource,
    /// 音源管理页：把选中音源的优先级往上 / 往下调。
    RaiseSourcePriority,
    LowerSourcePriority,
    /// 按下了数字 1-9。语义由 App 按当前焦点决定：
    /// 侧边栏 → 切标签页。数字键只做这一件事，不会去选中列表项。
    Digit(u8),

    /// 未绑定。
    None,
}

/// 把一个按键事件翻译成语义动作。
/// 自定义键位表。启动时一次性装好，之后只读。
///
/// 用 `OnceLock` 而不是把它塞进 `AppState`：`resolve` 是个纯函数式的入口，
/// 被输入处理链路直接调用，为它单独传递上下文要改动所有调用点，不划算。
static CUSTOM: OnceLock<HashMap<(KeyCode, KeyModifiers), Action>> = OnceLock::new();

/// 安装配置文件里的自定义键位。
///
/// # 冲突与非法
///
/// * **按键冲突**（两个动作绑到同一个键）：后配置的覆盖先配置的，并记 WARN——
///   静默丢一个会让用户以为是程序坏了。
/// * **非法**：动作名不存在、按键名无法解析、或动作带参数（`SwitchTab` 等
///   需要数字的动作）→ 跳过该条并记 WARN，**不中断启动**。键位错了只是不顺手，
///   不该让程序起不来。
///
/// 返回实际生效的条数，便于启动时在日志里核对。
pub fn install_custom(bindings: &BTreeMap<String, String>) -> usize {
    let mut table: HashMap<(KeyCode, KeyModifiers), Action> = HashMap::new();

    for (action_name, key_name) in bindings {
        let action = match action_from_name(action_name) {
            Some(action) => action,
            None => {
                crate::logger::tlog!(
                    crate::logger::LEVEL_WARN,
                    "键位配置：未知动作 {action_name:?}（键 {key_name:?}），已忽略"
                );
                continue;
            }
        };
        let (code, modifiers) = match parse_key(key_name) {
            Some(parsed) => parsed,
            None => {
                crate::logger::tlog!(
                    crate::logger::LEVEL_WARN,
                    "键位配置：无法解析按键 {key_name:?}（动作 {action_name}），已忽略"
                );
                continue;
            }
        };

        if let Some(previous) = table.insert((code, modifiers), action) {
            crate::logger::tlog!(
                crate::logger::LEVEL_WARN,
                "键位配置：{key_name:?} 同时绑定了 {action_name} 与之前的 {previous:?}，以 {action_name} 为准"
            );
        }
    }

    let installed = table.len();
    if installed > 0 {
        let _ = CUSTOM.set(table);
        crate::logger::tlog!(crate::logger::LEVEL_INFO, "已加载 {installed} 条自定义键位");
    }
    installed
}

/// 动作名（snake_case）→ [`Action`]。
///
/// 带参数的动作（`SwitchTab` / `SeekTo` / `SeekBy` / `Char`）不在其中：
/// 它们的值来自运行时，配置文件里写不出完整语义。
pub fn action_from_name(name: &str) -> Option<Action> {
    Some(match name {
        "quit" => Action::Quit,
        "force_quit" => Action::ForceQuit,
        "help" => Action::Help,
        "move_up" => Action::MoveUp,
        "move_down" => Action::MoveDown,
        "move_top" => Action::MoveTop,
        "move_bottom" => Action::MoveBottom,
        "page_up" => Action::PageUp,
        "page_down" => Action::PageDown,
        "focus_next" => Action::FocusNext,
        "focus_prev" => Action::FocusPrev,
        "submit" => Action::Submit,
        "cancel" => Action::Cancel,
        "backspace" => Action::Backspace,
        "delete" => Action::Delete,
        "cursor_left" => Action::CursorLeft,
        "cursor_right" => Action::CursorRight,
        "cursor_home" => Action::CursorHome,
        "cursor_end" => Action::CursorEnd,
        "play_pause" => Action::PlayPause,
        "next" => Action::Next,
        "prev" => Action::Prev,
        "seek_forward" => Action::SeekForward,
        "seek_backward" => Action::SeekBackward,
        "load_more_search" => Action::LoadMoreSearch,
        "volume_up" => Action::VolumeUp,
        "volume_down" => Action::VolumeDown,
        "toggle_mute" => Action::ToggleMute,
        "cycle_playback_mode" => Action::CyclePlaybackMode,
        "cycle_quality" => Action::CycleQuality,
        "toggle_lyric_panel" => Action::ToggleLyricPanel,
        "lyric_delay" => Action::LyricDelay,
        "lyric_advance" => Action::LyricAdvance,
        "open_search" => Action::OpenSearch,
        "open_settings" => Action::OpenSettings,
        "download_current" => Action::DownloadCurrent,
        "reload" => Action::Reload,
        "queue_append" => Action::QueueAppend,
        "add_all_to_queue" => Action::AddAllToQueue,
        "queue_play_next" => Action::QueuePlayNext,
        "remove_from_queue" => Action::RemoveFromQueue,
        "clear_queue" => Action::ClearQueue,
        "clear_cache" => Action::ClearCache,
        "toggle_sort_order" => Action::ToggleSortOrder,
        "open_ranks" => Action::OpenRanks,
        "open_cloud" => Action::OpenCloud,
        "login" => Action::Login,
        "claim_vip" => Action::ClaimVip,
        "cycle_artist_filter" => Action::CycleArtistFilter,
        "sync_to_cloud" => Action::SyncToCloud,
        "add_to_cloud" => Action::AddToCloud,
        "remove_from_cloud" => Action::RemoveFromCloud,
        "delete_cloud_playlist" => Action::DeleteCloudPlaylist,
        "new_cloud_playlist" => Action::NewCloudPlaylist,
        "toggle_sidebar" => Action::ToggleSidebar,
        "switch_source" => Action::SwitchSource,
        "set_default_source" => Action::SetDefaultSource,
        "raise_source_priority" => Action::RaiseSourcePriority,
        "lower_source_priority" => Action::LowerSourcePriority,
        _ => return None,
    })
}

/// 解析按键名 → (键码, 修饰键)。
///
/// 支持单字符（`q` / `Q` / `/`）、命名键（`space` / `enter` / `up` / `f1`…）
/// 以及 `ctrl+` / `alt+` / `shift+` 前缀。
fn parse_key(text: &str) -> Option<(KeyCode, KeyModifiers)> {
    let text = text.trim();
    let (modifiers, key) = if let Some(rest) = text.strip_prefix("ctrl+") {
        (KeyModifiers::CONTROL, rest)
    } else if let Some(rest) = text.strip_prefix("alt+") {
        (KeyModifiers::ALT, rest)
    } else if let Some(rest) = text.strip_prefix("shift+") {
        (KeyModifiers::SHIFT, rest)
    } else {
        (KeyModifiers::NONE, text)
    };

    let code = match key {
        "space" | " " => KeyCode::Char(' '),
        "enter" | "return" | "cr" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        "backspace" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        // 单个字符**必须先判**，否则会被下面的 F1~F12 分支吃掉：
        // `f` 也满足「长度 ≤ 3 且以 f 开头」，于是 `f[1..]` 是空串、parse 失败、
        // 整条返回 None —— 结果是「歌手地区筛选」的 `f` 唯独没法通过配置重绑，
        // 而其余单字母都行。这是写测试时才发现的。
        single if single.chars().count() == 1 => KeyCode::Char(single.chars().next()?),
        // f1 ~ f12
        function if function.len() <= 3 && function.starts_with('f') => {
            let number: u8 = function[1..].parse().ok()?;
            if (1..=12).contains(&number) {
                KeyCode::F(number)
            } else {
                return None;
            }
        }
        _ => return None,
    };

    Some((code, modifiers))
}

/// 查自定义键位。未安装或没命中返回 `None`。
fn custom_action(key: KeyEvent) -> Option<Action> {
    let table = CUSTOM.get()?;
    table
        .get(&(key.code, key.modifiers))
        // 终端对 Shift+字母 通常报「大写 Char + SHIFT」，而配置里写的可能是
        // 不带修饰的 "Q"。回退一次，两种写法都能命中。
        .or_else(|| table.get(&(key.code, KeyModifiers::NONE)))
        .copied()
}

pub fn resolve(key: KeyEvent, mode: KeyMode) -> Action {
    // Ctrl+C 在任何模式下都表示「立刻退出」，且不可被自定义覆盖：
    // 它是唯一的强制退出通道，被绑走会让用户在异常时出不来。
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::ForceQuit;
    }

    // 自定义键位优先于默认键表
    if let Some(action) = custom_action(key) {
        return action;
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
        // y = quality：换个不冲突的键，r 已经被播放模式占了
        KeyCode::Char('y') => Action::CycleQuality,
        KeyCode::Char('l') => Action::ToggleLyricPanel,
        // 歌词微调：方括号在键盘上紧邻，符合直觉
        KeyCode::Char(']') => Action::LyricAdvance,
        KeyCode::Char('[') => Action::LyricDelay,

        // ---- 业务 ----
        KeyCode::Char('/') => Action::OpenSearch,
        // 设置页的直达键。它现在也够得到数字键 9，但逗号仍在多数键盘上挨着
        // m/n 那一排、不与任何现有键冲突，留着当第二条路径。
        KeyCode::Char(',') => Action::OpenSettings,
        KeyCode::Char('R') => Action::Reload,
        KeyCode::Char('a') => Action::QueueAppend,
        KeyCode::Char('A') => Action::AddAllToQueue,
        KeyCode::Char('i') => Action::QueuePlayNext,
        KeyCode::Char('x') => Action::RemoveFromQueue,
        KeyCode::Char('X') => Action::ClearQueue,
        KeyCode::Char('C') => Action::ClearCache,
        // 右键菜单的键盘入口。m 已被静音占用，用分号——它在多数程序里就是
        // 「命令/菜单」那个键
        KeyCode::Char(';') => Action::ContextMenu,
        KeyCode::Char('M') => Action::LoadMoreSearch,
        KeyCode::Char('o') => Action::ToggleSortOrder,
        KeyCode::Char('b') => Action::OpenRanks,
        KeyCode::Char('c') => Action::OpenCloud,
        KeyCode::Char('L') => Action::Login,
        // 大写 V：小写 v 已经是「切换音源」了，而领取 VIP 正是要配合音源用
        // （只有概念版能领），放同一个键上容易误触。
        KeyCode::Char('V') => Action::ClaimVip,
        KeyCode::Char('f') => Action::CycleArtistFilter,
        KeyCode::Char('S') => Action::SyncToCloud,
        KeyCode::Char('s') => Action::AddToCloud,
        KeyCode::Char('d') => Action::RemoveFromCloud,
        KeyCode::Char('D') => Action::DeleteCloudPlaylist,
        KeyCode::Char('N') => Action::NewCloudPlaylist,
        KeyCode::Char('\\') => Action::ToggleSidebar,
        // `v` 不再循环切换音源：改为打开音源管理页，在那里面挑更直观
        KeyCode::Char('v') => Action::SwitchSource,
        // 大写 W：下载当前播放歌曲到设置页里选的目录。
        // 用大写是为了避开键盘上更常用的小写字母，避免和未来可能加的快捷键冲突。
        KeyCode::Char('W') => Action::DownloadCurrent,
        // 音源管理页专用。用大写是为了避开已经占满的小写键位。
        KeyCode::Char('E') => Action::SetDefaultSource,
        KeyCode::Char('K') => Action::RaiseSourcePriority,
        KeyCode::Char('J') => Action::LowerSourcePriority,

        // 0-9 的语义按焦点决定：侧边栏里是切标签页，列表里是跳到对应项。
        // keymap 只产出「按了数字几」，具体含义交给 App 层判断。
        //
        // `0` 必须在内：它是第 10 个标签页（可视化）的键，`Tab::number_key()`
        // 会把它印在侧边栏上。原先这里写的是 `'1'..='9'`，于是侧边栏显示
        // 「0 可视化」、按下去却毫无反应——文档和界面都在骗人。
        KeyCode::Char(digit @ '0'..='9') => Action::Digit(digit as u8 - b'0'),
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
    // 数字键 1-9 加 0 覆盖全部 10 个标签页，落点见 `Tab::ALL`。
    // **只切标签页，不做「列表内跳到第 N 项」**——焦点通常在歌曲列表上，
    // 若数字键改成跳列表项，最常用的「按数字切页」就没了。
    // 改标签页数量时必须同步这里，否则帮助面板会指向一个不存在的键，
    // 用户照着按却没反应，很难自查。
    ("1..9 / 0", "切换标签页", "导航"),
    ("Tab / S-Tab", "切换焦点区域", "导航"),
    ("j / k", "上下移动", "导航"),
    ("g / G", "跳到首行 / 末行", "导航"),
    ("PgUp / PgDn", "翻页", "导航"),
    ("Enter", "播放选中歌曲 / 进入", "导航"),
    ("Esc", "返回上一层", "导航"),
    ("/", "搜索", "业务"),
    (",", "打开设置页", "业务"),
    ("R", "刷新当前列表（绕过服务端 2 分钟缓存）", "业务"),
    ("M", "搜索结果加载下一页", "业务"),
    (";", "打开歌曲右键菜单（也可用鼠标右键）", "业务"),
    ("a / i", "加入队列 / 插播到下一首", "业务"),
    ("A", "把当前列表全部加入队列", "业务"),
    ("x / X", "移出队列 / 清空队列（需确认）", "业务"),
    ("C", "清空音频缓存（需确认）", "业务"),
    ("o", "正序 ↔ 倒序（最后一首在最上）", "业务"),
    ("y", "切换音质（下一首生效）", "业务"),
    ("b / c", "排行榜 / 云端歌单", "业务"),
    ("L", "扫码登录（酷狗 / 网易云）", "业务"),
    ("V", "领取今日概念版 VIP（仅概念版音源）", "业务"),
    ("f", "歌手地区筛选（在歌手页）", "业务"),
    ("E / J / K", "音源设为默认 / 调优先级（在音源页）", "业务"),
    ("s / S", "收藏单曲到云端 / 把整个队列同步到云端", "业务"),
    ("d / D", "从云端歌单移除 / 删除歌单（需确认）", "业务"),
    ("N", "新建云端歌单", "业务"),
    ("Space", "播放 / 暂停", "播放"),
    ("n / p", "下一首 / 上一首", "播放"),
    (
        "← / →",
        "列表内快退/快进 5 秒；设置页中则是修改选中项",
        "播放",
    ),
    ("+ / -", "音量增减 5%", "播放"),
    ("m", "静音开关", "播放"),
    ("r", "循环播放模式", "播放"),
    ("l", "歌词面板开关", "播放"),
    ("[ / ]", "歌词延后 / 提前 100ms", "播放"),
    ("W", "下载当前播放歌曲到设置里的目录", "播放"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_chars_named_keys_and_modifiers() {
        assert_eq!(
            parse_key("q"),
            Some((KeyCode::Char('q'), KeyModifiers::NONE))
        );
        assert_eq!(
            parse_key("Q"),
            Some((KeyCode::Char('Q'), KeyModifiers::NONE))
        );
        assert_eq!(
            parse_key("/"),
            Some((KeyCode::Char('/'), KeyModifiers::NONE))
        );
        assert_eq!(
            parse_key("space"),
            Some((KeyCode::Char(' '), KeyModifiers::NONE))
        );
        assert_eq!(
            parse_key("enter"),
            Some((KeyCode::Enter, KeyModifiers::NONE))
        );
        assert_eq!(parse_key("up"), Some((KeyCode::Up, KeyModifiers::NONE)));
        assert_eq!(parse_key("f1"), Some((KeyCode::F(1), KeyModifiers::NONE)));
        assert_eq!(
            parse_key("ctrl+n"),
            Some((KeyCode::Char('n'), KeyModifiers::CONTROL))
        );
        // 越界的功能键要拒绝，不能悄悄变成别的键
        assert_eq!(parse_key("f99"), None);
        assert_eq!(parse_key("不存在的键"), None);
    }

    /// `f` 必须解析成字母 `f`，不能掉进 F1~F12 那条分支。
    ///
    /// 实测踩到过：F 键那条判的是「长度 ≤ 3 且以 `f` 开头」，`f` 也满足，于是
    /// `f[1..]` 是空串、parse 失败、整条返回 `None`——**只有 `f`（歌手地区筛选）
    /// 没法通过配置重绑**，其余单字母都行。写「面板里的键都真绑过」那条测试时才发现。
    #[test]
    fn single_letter_f_is_not_swallowed_by_the_function_key_branch() {
        assert_eq!(
            parse_key("f"),
            Some((KeyCode::Char('f'), KeyModifiers::NONE)),
            "`f` 是普通字母键，不是功能键"
        );
        // 功能键本身不受影响
        assert_eq!(parse_key("f1"), Some((KeyCode::F(1), KeyModifiers::NONE)));
        assert_eq!(parse_key("f12"), Some((KeyCode::F(12), KeyModifiers::NONE)));
    }

    #[test]
    fn action_names_round_trip() {
        assert_eq!(action_from_name("quit"), Some(Action::Quit));
        assert_eq!(action_from_name("play_pause"), Some(Action::PlayPause));
        assert_eq!(
            action_from_name("set_default_source"),
            Some(Action::SetDefaultSource)
        );
        // 带参数的动作不在映射里：配置文件写不出完整语义
        assert_eq!(action_from_name("switch_tab"), None);
        assert_eq!(action_from_name("不存在的动作"), None);
    }

    /// Ctrl+C 是唯一的强制退出通道，自定义键位不能把它抢走。
    #[test]
    fn ctrl_c_cannot_be_rebound() {
        let forced = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(resolve(forced, KeyMode::Normal), Action::ForceQuit);
        assert_eq!(resolve(forced, KeyMode::TextInput), Action::ForceQuit);
    }

    /// 10 个数字键必须**全部**能产出 `Action::Digit`，包括 `0`。
    ///
    /// 原先的分支写的是 `'1'..='9'`，把 `0` 漏掉了：侧边栏印着「0 可视化」、
    /// 文档写着「1–9、0」、`Tab::number_key()` 也照常返回 `'0'`，但按下去
    /// 什么都不会发生——界面和文档一起骗人，而且没有任何报错。
    /// 这条测试按 `Tab::ALL` 的落点逐个验，改标签页数量时也会一起被钉住。
    #[test]
    fn every_digit_key_reaches_a_tab() {
        for digit in '0'..='9' {
            let event = KeyEvent::new(KeyCode::Char(digit), KeyModifiers::NONE);
            assert_eq!(
                resolve(event, KeyMode::Normal),
                Action::Digit(digit as u8 - b'0'),
                "「{digit}」应当是切标签页"
            );
        }
    }

    /// 领取 VIP 是**大写** `V`，小写 `v` 仍然是切换音源。
    ///
    /// 两个键挨着、又都和音源相关，最容易在改键位时被合并成一个。
    #[test]
    fn capital_v_claims_vip_and_lowercase_v_switches_source() {
        let capital = KeyEvent::new(KeyCode::Char('V'), KeyModifiers::NONE);
        assert_eq!(resolve(capital, KeyMode::Normal), Action::ClaimVip);

        let lower = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE);
        assert_eq!(resolve(lower, KeyMode::Normal), Action::SwitchSource);
    }

    /// 帮助面板里写出来的每个键，都必须是**真的绑过的**。
    ///
    /// 面板是用户唯一的速查入口，写一个按下去没反应的键比不写更糟。这条测试把
    /// `CHEATSHEET` 逐条喂给 `resolve`，对不上就失败——加键位时忘了同步面板、
    /// 或面板里留了个早就删掉的键，都会在这里现形。
    ///
    /// **反方向测不了**：`resolve_normal` 是个大 match（还有区间与 guard 分支），
    /// 枚举不出它到底绑了哪些键，所以「绑了但面板没写」只能靠人查。真要做成
    /// 自动的，得先把那张表改成数据——代价大于收益，先不划算。
    #[test]
    fn every_shortcut_shown_in_help_is_actually_bound() {
        /// 面板里的写法 → `parse_key` 认的写法。
        ///
        /// 面板是给人看的，`←` 比 `left` 直观，`S-Tab` 比 `backtab` 眼熟。
        fn parse_as_keymap_knows(token: &str) -> Option<String> {
            // 别名要先判：`←` 也是「一个字符」，会被下面那条单字符捷径截走，
            // 于是变成 `Char('←')`——而 `resolve_normal` 绑的是 `KeyCode::Left`。
            if let Some(name) = match token {
                "←" => Some("left"),
                "→" => Some("right"),
                "↑" => Some("up"),
                "↓" => Some("down"),
                "S-Tab" => Some("backtab"),
                _ => None,
            } {
                return Some(name.to_string());
            }
            // `1..9` 是个范围，不是单个键，没法喂给 parse_key
            if token.contains("..") {
                return None;
            }
            // 单个字符必须**保留大小写**：`q` 与 `Q` 是两个不同的动作
            if token.chars().count() == 1 {
                return Some(token.to_string());
            }
            Some(token.to_lowercase())
        }

        let mut unbound = Vec::new();
        for (keys, description, _) in CHEATSHEET {
            // 用 `" / "` 分隔而不是 `"/"`：条目 12 的键就是 `/`（搜索），
            // 按单斜杠切会把它切成两个空串，等于跳过不查。
            for token in keys.split(" / ").map(str::trim).filter(|t| !t.is_empty()) {
                let Some(name) = parse_as_keymap_knows(token) else {
                    continue;
                };
                let Some((code, modifiers)) = parse_key(&name) else {
                    unbound.push(format!("「{token}」({description}) 连写法都不认识"));
                    continue;
                };
                if resolve(KeyEvent::new(code, modifiers), KeyMode::Normal) == Action::None {
                    unbound.push(format!("「{token}」({description})"));
                }
            }
        }

        assert!(
            unbound.is_empty(),
            "帮助面板里这些键其实没绑定：{}",
            unbound.join("、")
        );
    }
}
