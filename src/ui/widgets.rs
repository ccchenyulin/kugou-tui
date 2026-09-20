//! 可复用的渲染原语。
//!
//! 这些函数只依赖 [`AppState`] 的片段或纯数据，不感知「当前在哪个标签页」，
//! 因此各个视图可以自由组合它们。

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Padding;
use ratatui::widgets::{Block, BorderType, Borders, HighlightSpacing, List, ListItem, Paragraph};

use crate::api::model::Song;
use crate::audio::engine::PlaybackState;
use crate::ui::theme::Theme;

/// 把一段文本编码成二维码，渲染成可直接显示的行。
///
/// # 为什么用半块字符
///
/// 终端字符的高度约为宽度的两倍。若一个模块占一个字符，二维码会被纵向拉成两倍；
/// 登录用的二维码通常是 33×33 模块以上，那样会占满整个屏幕。
///
/// 这里用 `▀` `▄` `█` 把**上下两个模块压进同一个字符**：前景色画上半格、背景色画
/// 下半格。这样每个模块占「1 列宽 × 半行高」，近似正方形，整体体积也只有原来的
/// 四分之一。
///
/// 返回 `None` 表示内容过长无法编码（登录场景不会发生）。
pub fn qr_lines(content: &str) -> Option<Vec<String>> {
    /// 静默区（二维码外围留白）的模块数。
    ///
    /// 规范要求 4 个模块，但终端里每多一圈就多占半行。这里取 2——配合固定的
    /// 「白底」配色，留白本身就是静默区，2 个模块足以被识别。
    const QUIET: usize = 2;

    let code = qrcode::QrCode::new(content.as_bytes()).ok()?;
    let image = code.render::<char>().quiet_zone(false).build();

    let core: Vec<Vec<bool>> = image
        .lines()
        .map(|line| line.chars().map(|character| character != ' ').collect())
        .collect();
    let core_width = core.first().map(Vec::len).unwrap_or(0);
    if core_width == 0 {
        return None;
    }

    // 四周补一圈留白
    let width = core_width + QUIET * 2;
    let mut modules: Vec<Vec<bool>> = Vec::with_capacity(core.len() + QUIET * 2);
    for _ in 0..QUIET {
        modules.push(vec![false; width]);
    }
    for row in &core {
        let mut padded = vec![false; width];
        padded[QUIET..QUIET + core_width].copy_from_slice(row);
        modules.push(padded);
    }
    for _ in 0..QUIET {
        modules.push(vec![false; width]);
    }

    let mut lines = Vec::with_capacity(modules.len().div_ceil(2));
    let mut row = 0;
    while row < modules.len() {
        let top = &modules[row];
        let bottom = modules.get(row + 1);

        let mut line = String::with_capacity(top.len());
        for (column, top_dark) in top.iter().enumerate() {
            let bottom_dark = bottom.is_some_and(|row| row[column]);
            line.push(match (*top_dark, bottom_dark) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        lines.push(line);
        row += 2;
    }

    Some(lines)
}

/// 构造带**统一选中表现**的列表。
///
/// # 为什么不能只靠背景色
///
/// 早期版本只用 `highlight_style` 的背景色表示选中。在深色终端上
/// `#264260` 这种暗蓝与背景对比极低，用户移动光标时**看不出任何变化**，
/// 会误以为程序卡死或按键无效。所以这里固定加一个 `> ` 文字标记：
/// 文字在任何终端、任何配色下都看得见。
///
/// `HighlightSpacing::Always` 给所有行预留同样宽度的标记位，选中行与其它行
/// 才不会错位。
pub fn selection_list<'a>(items: Vec<ListItem<'a>>, theme: &Theme) -> List<'a> {
    List::new(items)
        .highlight_symbol("> ")
        .highlight_style(theme.selection())
        .highlight_spacing(HighlightSpacing::Always)
        // 上下各留 2 行上下文，光标移动时视线不用重新找位置
        .scroll_padding(2)
}

/// 统一的带边框面板。
///
/// `focused` 决定边框亮度——这是界面里唯一表示「键盘焦点在哪」的视觉线索，
/// 比给每个面板加标题后缀更省空间。
pub fn panel(title: impl Into<Line<'static>>, focused: bool, theme: &Theme) -> Block<'static> {
    let border_style = if focused {
        theme.focused_border()
    } else {
        theme.idle_border()
    };

    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(title.into())
        .title_style(if focused { theme.title() } else { theme.dim() })
        // 左右各留一列：内容贴着边框会显得很挤（rmpc 全项目都这么做）
        .padding(Padding::horizontal(1))
}

/// 在 `area` 内居中放置一个固定尺寸的矩形，用于弹窗。
///
/// 尺寸会被钳到 `area` 之内，避免小终端下算出越界矩形。
pub fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// 单行提示，居中显示。
pub fn placeholder(text: &str, theme: &Theme) -> Paragraph<'static> {
    Paragraph::new(Line::from(Span::styled(text.to_string(), theme.dim())))
}

/// 一行的渲染上下文。
#[derive(Debug, Clone, Copy)]
pub struct RowContext {
    /// 可用列宽，用于计算各列宽度。
    pub width: usize,
    /// 是否为正在播放的曲目。
    pub is_current: bool,
    /// 当前播放状态。
    pub playback: PlaybackState,
    /// 鼠标是否悬停在这一行。
    ///
    /// 终端不是网页，没有 CSS `:hover`；这里是靠 crossterm 的鼠标移动事件 +
    /// 命中测试算出来的。终端没开鼠标捕获时永远是 false，属于无害降级。
    pub hover: bool,
}

/// 把一首歌渲染成列表行。
///
/// 列宽按可用宽度自适应：歌名占大头，歌手与专辑依次让位，时长固定靠右。
/// 所有文本都按**显示宽度**截断（CJK 记 2 列），因此中英文混排不会串列。
pub fn song_row(
    number: usize,
    song: &Song,
    context: RowContext,
    theme: &Theme,
) -> ListItem<'static> {
    // 序号 4 列 + 时长 6 列 + 4 个空格分隔
    const NUMBER_WIDTH: usize = 4;
    const DURATION_WIDTH: usize = 6;
    const GAPS: usize = 4;

    let flexible = context
        .width
        .saturating_sub(NUMBER_WIDTH + DURATION_WIDTH + GAPS);
    let name_width = (flexible * 45 / 100).max(8);
    let singer_width = (flexible * 25 / 100).max(6);
    let album_width = flexible.saturating_sub(name_width + singer_width + 2);

    // ASCII 标记，避免字体缺字。
    //
    // 播放中刻意**不用** `>`：列表的选中标记已经占用了 `> `，两个 `>` 并排
    // （形如 `> >  1 歌名`）很容易被看成一个符号，分不清哪条是选中、哪条在播。
    let marker = if context.is_current {
        match context.playback {
            PlaybackState::Playing => "*",
            PlaybackState::Paused => "=",
            PlaybackState::Loading => "~",
            PlaybackState::Stopped => " ",
        }
    } else {
        " "
    };

    let number_style = if context.is_current {
        theme.now_playing()
    } else {
        theme.dim()
    };

    let name_style = if context.is_current {
        theme.now_playing()
    } else if context.hover {
        // 悬停：加粗一下就够，不用换背景——终端里大面积反色很刺眼
        theme.body().add_modifier(ratatui::style::Modifier::BOLD)
    } else {
        theme.body()
    };

    let line = Line::from(vec![
        // 序号从 1 开始：用户看的是「第几首」，不是数组下标
        Span::styled(format!("{marker}{:>3} ", number + 1), number_style),
        Span::styled(
            format!("{} ", truncate_to_width(&song.name, name_width)),
            name_style,
        ),
        Span::styled(
            format!("{} ", truncate_to_width(&song.singer_text(), singer_width)),
            theme.dim(),
        ),
        Span::styled(
            format!(
                "{} ",
                truncate_to_width(display_album(song), album_width.max(1))
            ),
            theme.dim(),
        ),
        Span::styled(song.duration_text(), theme.dim()),
    ]);

    ListItem::new(line)
}

/// 专辑名缺失时给个占位，避免列塌陷。
fn display_album(song: &Song) -> &str {
    if song.album_name.is_empty() {
        "—"
    } else {
        &song.album_name
    }
}

/// 两段式列表行：左侧主标题 + 右侧副标题。
pub fn entry_row(
    index: usize,
    title: &str,
    subtitle: &str,
    marker: Option<&str>,
    width: usize,
    theme: &Theme,
) -> ListItem<'static> {
    let prefix = marker.unwrap_or(" ");
    let index_text = format!("{prefix}{:>3} ", index + 1);
    // 分隔空格并入副标题，这样它不会被后面的截断逻辑单独吃掉
    let subtitle_text = if subtitle.is_empty() {
        String::new()
    } else {
        format!(" {subtitle}")
    };

    // 一律按**显示宽度**计算。用 `str::len()`（字节数）会让 CJK 副标题
    // 把可用宽度估得过小，歌名被多截一截，末尾的分隔符还会被挤掉。
    let reserved = display_width(&index_text) + display_width(&subtitle_text);
    let available = width.saturating_sub(reserved).max(6);

    let line = Line::from(vec![
        Span::styled(index_text, theme.dim()),
        Span::styled(truncate_to_width(title, available), theme.body()),
        Span::styled(subtitle_text, theme.dim()),
    ]);

    ListItem::new(line)
}

/// 显示宽度：CJK 与全角字符按 2 列计算。
///
/// 没有引入 `unicode-width`：歌词和歌名里出现的字符绝大多数落在下面这些区间，
/// 误差只影响个别生僻符号的对齐，不值得多一条依赖。
pub fn char_width(character: char) -> usize {
    match character as u32 {
        0x1100..=0x115F      // 谚文字母
        | 0x2E80..=0x303E    // CJK 部首、假名标点
        | 0x3041..=0x33FF    // 假名、注音、CJK 兼容
        | 0x3400..=0x4DBF    // CJK 扩展 A
        | 0x4E00..=0x9FFF    // CJK 基本区
        | 0xA000..=0xA4CF    // 彝文
        | 0xAC00..=0xD7A3    // 谚文音节
        | 0xF900..=0xFAFF    // CJK 兼容表意
        | 0xFE30..=0xFE6F    // CJK 兼容形式
        | 0xFF00..=0xFF60    // 全角形式
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F  // 常用 emoji
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

pub fn display_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

/// 按终端显示宽度截断，超出部分用 `…` 收尾。
pub fn truncate_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if display_width(text) <= max_width {
        return text.to_string();
    }

    // 留一列给省略号
    let budget = max_width.saturating_sub(1);
    let mut output = String::new();
    let mut width = 0usize;

    for character in text.chars() {
        let character_width = char_width(character);
        if width + character_width > budget {
            break;
        }
        output.push(character);
        width += character_width;
    }

    output.push('…');
    output
}

/// 把字节数格式化成人类可读的形式。
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;

    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_cjk_as_double_width() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("海阔天空"), 8);
        assert_eq!(display_width("a海b"), 4);
    }

    #[test]
    fn truncates_within_budget() {
        assert_eq!(truncate_to_width("abcdef", 10), "abcdef");
        assert_eq!(truncate_to_width("abcdef", 4), "abc…");
        assert_eq!(truncate_to_width("海阔天空", 4), "海…");
        assert_eq!(truncate_to_width("海阔天空", 5), "海阔…");
    }

    #[test]
    fn truncation_of_zero_width_is_empty() {
        assert_eq!(truncate_to_width("abc", 0), "");
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KiB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MiB");
    }

    #[test]
    fn centered_rect_is_clamped_to_area() {
        let area = Rect::new(0, 0, 40, 10);
        let popup = centered_rect(area, 100, 100);
        assert_eq!(popup.width, 40);
        assert_eq!(popup.height, 10);
        assert_eq!(popup.x, 0);
        assert_eq!(popup.y, 0);
    }

    #[test]
    fn centered_rect_centers_smaller_popup() {
        let area = Rect::new(0, 0, 40, 20);
        let popup = centered_rect(area, 20, 10);
        assert_eq!(popup.x, 10);
        assert_eq!(popup.y, 5);
        assert_eq!(popup.width, 20);
        assert_eq!(popup.height, 10);
    }
}
