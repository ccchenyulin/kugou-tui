//! 视图渲染。
//!
//! 每个函数只负责一个矩形区域的绘制，通过参数接收它需要的那部分状态，
//! 而不是整个 [`AppState`]——这样借用关系是显式的，编译器能帮我们发现
//! 「同一帧里既读又写同一个字段」这类问题。
//!
//! 视图函数**不修改**状态，除了把 [`ListState`] 交给 `render_stateful_widget`
//! 以便 ratatui 更新滚动偏移。

pub mod lists;
pub mod player;
pub mod settings;
pub mod sources;
pub mod visualizer;

pub use lists::SongView;
pub use lists::{
    render_artist_entries, render_cloud_entries, render_playlist_entries, render_rank_entries,
    render_search_input, render_song_list,
};
pub use player::{
    QueueView, prepare_cover, render_home, render_lyric_panel, render_player, render_queue,
};
pub use settings::render_settings;
pub use sources::{render_login_picker, render_sources};
pub use visualizer::render_visualizer;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Clear, Paragraph, Row, Table, Wrap};

use crate::app::state::{AppState, ConfirmAction, Focus, LoginState, PromptState, Tab};
use crate::keymap::CHEATSHEET;
use crate::ui::theme::Theme;
use crate::ui::widgets::{display_width, human_bytes, panel, placeholder, truncate_to_width};

/// 侧边栏宽度：随终端宽度伸缩，但保持在一个可读区间内。
pub fn sidebar_width(total_width: u16) -> u16 {
    // 下限 24 而不是 20：panel 的左右内边距各占 1 列，加上边框 2 列，
    // 20 列只剩 16 列可用——「API 127.0.0.1:3001」这类行会被挤断成两行。
    // 24 列刚好放得下最长的那一行（20 字符）。
    (total_width / 5).clamp(24, 32)
}

/// 左侧导航栏：标签页切换 + 连接/播放/缓存概览。
/// 把播放电平画成一排竖条。
///
/// 用的是 `▁▂▃▄▅▆▇█` 这组八级块字符：不依赖真彩，在没有 256 色的终端上也能看；
/// 而且只占一行，比用 Gauge 省地方。数据是音频线程算出的真实峰值，静音会掉到底、
/// 鼓点会顶到头，不是随机动画。
fn level_line(levels: &[f32], width: u16, theme: &Theme) -> Line<'static> {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    let width = width as usize;
    if width == 0 {
        return Line::default();
    }

    if levels.is_empty() {
        return Line::from(Span::styled("  未播放", theme.dim()));
    }

    // 电平格子数（LEVEL_BUCKETS）和侧边栏能显示的列数不一定相等。若直接按格子数铺，
    // 超宽就会折行，把下面整个「连接」区顶下去——所以这里按可用宽度重采样。
    let text: String = (0..width)
        .map(|column| {
            let index = (column * levels.len() / width).min(levels.len() - 1);
            let level = levels[index].clamp(0.0, 1.0);
            let bar = (level * (BARS.len() - 1) as f32).round() as usize;
            BARS[bar.min(BARS.len() - 1)]
        })
        .collect();

    Line::from(Span::styled(text, theme.now_playing()))
}

pub fn render_sidebar(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let focused = state.focus == Focus::Sidebar;
    let block = panel("kugou-tui", focused, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!("kugou-tui {}", state.tab.position_text()),
        theme.dim(),
    )));

    // 按分组渲染：10 个标签平铺会像一堵文字墙，分组后才扫得动。
    // 每个标签前标出数字键——显示顺序与 `Tab::ALL`（数字键落点）不同，
    // 不标出来的话用户按 1 却跳到别的页。
    let mut current_group: Option<&'static str> = None;
    for tab in Tab::SIDEBAR_ORDER {
        if Some(tab.group()) != current_group {
            current_group = Some(tab.group());
            lines.push(Line::from(Span::styled(
                format!("── {} ──", tab.group()),
                theme.dim(),
            )));
        }

        let selected = tab == state.tab;
        let pointer = if selected { ">" } else { " " };
        let style = if selected {
            theme.now_playing()
        } else {
            theme.body()
        };
        let key = match tab.number_key() {
            Some(key) => key.to_string(),
            None => " ".to_string(),
        };
        lines.push(Line::from(Span::styled(
            format!("{pointer} {key} {} {}", tab.icon(), tab.title()),
            style,
        )));
    }

    // 播放电平放在导航下方：它是「现在在放什么」最直观的反馈。
    // 传入可用宽度，避免条数超出后折行、把下面的区块顶下去。
    lines.push(level_line(&state.levels, inner.width, theme));

    lines.push(Line::default());
    lines.push(section("连接", theme));
    lines.push(kv("API", api_host(&state.config.api_base), theme));
    lines.push(kv("音源", state.config.active_source_kind().label(), theme));
    lines.push(kv("登录", if state.logged_in { "是" } else { "否" }, theme));
    lines.push(kv(
        "指纹",
        if state.config.dfid.is_some() {
            "已获取"
        } else {
            "—"
        },
        theme,
    ));
    // 会员形态直接显示出来：排查「有会员却只能试听」时，一眼就能看出
    // 服务端到底认成了哪种会员、什么时候到期
    if let Some(label) = state.vip_label.as_deref() {
        lines.push(kv("会员", label, theme));
    }

    lines.push(Line::default());
    lines.push(section("播放", theme));
    lines.push(kv("模式", state.queue.mode().label(), theme));
    lines.push(kv("队列", &format!("{} 首", state.queue.len()), theme));
    lines.push(kv(
        "音量",
        &if state.is_muted() {
            "静音".to_string()
        } else {
            format!("{:.0}%", state.volume * 100.0)
        },
        theme,
    ));

    lines.push(Line::default());
    lines.push(section("缓存", theme));
    let limit = if state.config.cache_limit_mib == 0 {
        "不限".to_string()
    } else {
        format!("{} MiB", state.config.cache_limit_mib)
    };
    lines.push(kv("已用", &human_bytes(state.cache_bytes), theme));
    lines.push(kv("上限", &limit, theme));
    // 缓存目录与清理按键挤在一行：侧边栏只有 19 列，而且高度已经占满
    // （实测再单独加一行会被裁掉），完整路径用 `--print-config` 看。
    // 这里只显示目录最后一段，指个方向就够。
    let dir = dir_basename(&state.config.cache_dir);
    lines.push(Line::from(vec![
        Span::styled("  C清理 ", theme.dim()),
        Span::styled(dir, theme.body()),
    ]));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// 取路径的最后一段（目录名），用于在窄侧边栏里指代缓存目录。
///
/// 取不到（比如路径以 `..` 结尾）就退回完整路径——宁可让它被侧边栏裁掉，
/// 也不要显示一个认不出来的空值。
fn dir_basename(path: &std::path::Path) -> String {
    // 用 to_string_lossy 而非 OsStr::display()：后者要 Rust 1.87，
    // 项目 MSRV 是 1.86，用了 clippy 会报 MSRV 错误
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

fn section(title: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(format!("── {title}"), theme.dim()))
}

fn kv(key: &str, value: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key} "), theme.dim()),
        Span::styled(value.to_string(), theme.body()),
    ])
}

/// 从 API 地址里剥掉协议前缀，侧边栏窄，省几个字符。
fn api_host(api_base: &str) -> &str {
    api_base
        .trim_start_matches("https://")
        .trim_start_matches("http://")
}

/// 底部状态栏：左侧消息 + 右侧忙碌指示与快捷键提示。
pub fn render_status(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    busy: Option<String>,
    theme: &Theme,
) {
    if area.height == 0 {
        return;
    }

    let message = truncate_to_width(&state.status, area.width.saturating_sub(24) as usize);
    let mut spans = vec![Span::styled(
        format!(" {message}"),
        theme.status(state.status_level),
    )];

    if let Some(busy) = busy {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(format!("[{busy}]"), theme.title()));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);

    // 右侧快捷键提示靠右对齐，单独画一层，避免和长消息互相挤压
    let hint = Line::from(vec![
        Span::styled("[?]", theme.key_hint()),
        Span::styled(" 帮助 ", theme.dim()),
        Span::styled("[q]", theme.key_hint()),
        Span::styled(" 退出 ", theme.dim()),
    ]);
    let hint_width = display_width("[?] 帮助 [q] 退出 ");
    if area.width as usize > hint_width + 20 {
        let hint_area = Rect {
            x: area.x + area.width - hint_width as u16,
            y: area.y,
            width: hint_width as u16,
            height: 1,
        };
        frame.render_widget(Paragraph::new(hint), hint_area);
    }
}

/// 文本输入弹窗（新建歌单等）。
pub fn render_prompt(frame: &mut Frame, prompt: &PromptState, theme: &Theme) {
    let popup = crate::ui::widgets::centered_rect(frame.area(), 52, 5);
    frame.render_widget(Clear, popup);

    let block = panel(prompt.title.clone(), true, theme);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let line = Line::from(vec![
        Span::styled("> ", theme.title()),
        Span::styled(prompt.buffer.clone(), theme.body()),
    ]);
    frame.render_widget(
        Paragraph::new(vec![
            line,
            Line::from(""),
            Line::from(Span::styled("Enter 创建 · Esc 取消", theme.dim())),
        ]),
        inner,
    );
}

/// 扫码登录弹窗。
///
/// 二维码用 `█` 与空格渲染——终端显示不了接口返回的 PNG，只能自己编码。每个模块横向
/// 重复一次以修正终端字符的高宽比。
pub fn render_login(
    frame: &mut Frame,
    login: &LoginState,
    source: crate::source::SourceKind,
    theme: &Theme,
) {
    let popup = crate::ui::widgets::centered_rect(
        frame.area(),
        login.dialog_width(),
        login.dialog_height(),
    );
    frame.render_widget(Clear, popup);

    let title = if login.succeeded {
        "登录成功"
    } else {
        "扫码登录"
    };
    let block = panel(title, true, theme);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let qr_style = theme.qr();
    let mut lines: Vec<Line> = Vec::new();
    for row in &login.qr {
        lines.push(Line::from(Span::styled(row.clone(), qr_style)));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        login.message.clone(),
        theme.title(),
    )));

    // 未结束 + 不是网易云：上面那段说明对酷狗够用了；网易云用户得明确知道用哪个 App。
    if !login.finished && matches!(source, crate::source::SourceKind::Netease) {
        lines.push(Line::from(Span::styled(
            "用网易云 App 扫描（手机端登录入口）",
            theme.dim(),
        )));
    }

    // 已结束的弹窗里已经有结论了，「取消」字样跟原消息打架，改成「关闭」。
    lines.push(Line::from(Span::styled(
        if login.finished {
            "Esc 关闭"
        } else {
            "Esc 取消"
        },
        theme.dim(),
    )));

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
        inner,
    );
}

/// 二次确认对话框。
///
/// 用于清空队列这类破坏性操作。故意做成模态——任何按键都先被它接走，只有确认键才
/// 生效，这样误按一下不会把整个队列清掉。
pub fn render_confirm(frame: &mut Frame, action: ConfirmAction, theme: &Theme) {
    let popup = crate::ui::widgets::centered_rect(frame.area(), 52, 5);
    frame.render_widget(Clear, popup);

    let block = panel("请确认", true, theme);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let text = vec![
        Line::from(action.question()),
        Line::from(""),
        Line::from(Span::styled(action.hint(), theme.dim())),
    ];
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        inner,
    );
}

/// 歌曲右键菜单。
///
/// 居中的小弹窗，每项右侧标出对应键位——菜单是**键盘动作的索引**而不是
/// 另一套交互，标出来用户下次就能直接按。
pub fn render_context_menu(
    frame: &mut Frame,
    menu: &crate::app::state::ContextMenu,
    theme: &Theme,
) {
    use crate::app::state::MenuAction;

    let height = (menu.items.len() as u16 + 4).min(frame.area().height);
    let popup = crate::ui::widgets::centered_rect(frame.area(), 34, height);
    frame.render_widget(Clear, popup);

    // 标题带歌名：菜单是「对哪首歌操作」，不写清楚容易点错
    let title = format!("《{}》", menu.song.name);
    let block = panel(title, true, theme);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (index, action) in menu.items.iter().enumerate() {
        let selected = index == menu.cursor;
        let pointer = if selected { ">" } else { " " };
        let style = if selected {
            theme.selection().add_modifier(Modifier::BOLD)
        } else {
            theme.body()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{pointer} {}", action.label()), style),
            Span::styled(
                format!("{:>width$}", action.key_hint(), width = 10),
                if selected { style } else { theme.dim() },
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "↑↓ 选择 · Enter 执行 · Esc 关闭",
        theme.dim(),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);

    let _ = MenuAction::Play; // 类型已用于 items，这里仅为可读性
}

/// 下载音质选择框：列出可选音质，默认落在当前全局音质上。
///
/// 与右键菜单同样的浮层写法（Clear + panel + 居中），但内容换成音质列表。
pub fn render_quality_picker(
    frame: &mut Frame,
    picker: &crate::app::state::QualityPicker,
    theme: &Theme,
) {
    let height = (picker.candidates.len() as u16 + 6).min(frame.area().height);
    // 宽度要放得下最长的那一行（「蝰蛇全景声  viper_atmos」），太窄会截断
    let popup = crate::ui::widgets::centered_rect(frame.area(), 46, height);
    frame.render_widget(Clear, popup);

    let block = panel("下载音质", true, theme);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height == 0 {
        return;
    }

    let selected_index = picker.cursor.selected().unwrap_or(0);
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        truncate_to_width(&picker.song.name, inner.width as usize),
        theme.dim(),
    )));
    lines.push(Line::from(""));

    for (index, quality) in picker.candidates.iter().enumerate() {
        let selected = index == selected_index;
        let pointer = if selected { ">" } else { " " };
        let style = if selected {
            theme.selection().add_modifier(Modifier::BOLD)
        } else {
            theme.body()
        };
        let label = crate::app::settings::quality_label(quality);
        lines.push(Line::from(vec![
            Span::styled(
                truncate_to_width(
                    &format!("{pointer} {label}"),
                    inner.width.saturating_sub(8) as usize,
                ),
                style,
            ),
            Span::styled(
                format!("{:>width$}", quality, width = 7),
                if selected { style } else { theme.dim() },
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "↑↓ 选择 · Enter 下载 · Esc 取消",
        theme.dim(),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// 帮助弹窗。
pub fn render_help(frame: &mut Frame, area: Rect, theme: &Theme) {
    let width = (area.width.saturating_sub(8)).min(76);
    let height = (area.height.saturating_sub(4)).min(CHEATSHEET.len() as u16 + 6);
    let popup = crate::ui::widgets::centered_rect(area, width, height);

    frame.render_widget(Clear, popup);

    let block = panel(
        Line::from(Span::styled("快捷键 · 按 ? 或 Esc 关闭", theme.title())),
        true,
        theme,
    );
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height == 0 {
        return;
    }

    let rows = CHEATSHEET.iter().map(|(key, description, group)| {
        Row::new(vec![
            Cell::from((*key).to_string()),
            Cell::from((*description).to_string()),
            Cell::from((*group).to_string()),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Min(20),
            Constraint::Length(6),
        ],
    )
    .header(Row::new(vec!["按键", "功能", "分类"]).style(theme.title()))
    .column_spacing(1);

    frame.render_widget(table, inner);
}

/// 供视图复用的占位段落。
pub fn loading_placeholder(theme: &Theme) -> Paragraph<'static> {
    placeholder("载入中…", theme).alignment(Alignment::Center)
}

/// 供视图复用的空列表占位。
pub fn empty_placeholder(text: &str, theme: &Theme) -> Paragraph<'static> {
    placeholder(text, theme).alignment(Alignment::Center)
}
