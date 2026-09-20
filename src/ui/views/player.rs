//! 播放条、歌词面板、播放队列。
//!
//! 三者共同回答用户最关心的三个问题：**在放什么**（播放条）、**唱到哪了**
//! （歌词）、**接下来放什么**（队列）。

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, ListState, Paragraph};
use ratatui_image::StatefulImage;

use crate::api::model::format_duration_ms;
use crate::app::queue::PlayQueue;
use crate::app::state::{AppState, HitTarget};
use crate::audio::engine::PlaybackState;
use crate::ui::theme::Theme;
use crate::ui::views::{empty_placeholder, loading_placeholder};
use crate::ui::widgets::{RowContext, panel, selection_list, song_row, truncate_to_width};

/// 播放条高度：2 行内容 + 上下边框。
pub const PLAYER_HEIGHT: u16 = 4;

/// 播放条：曲目信息 + 进度条。
pub fn render_player(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
    if area.height < 3 || area.width < 20 {
        return;
    }

    let block = panel("播放", false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width < 10 {
        return;
    }

    let [header_area, gauge_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

    render_player_header(frame, header_area, state, theme);

    // 进度条自带居中标签，把时间放在条上，省下一整行
    let label = format!(
        "{} / {}",
        format_duration_ms(state.position_ms),
        format_duration_ms(state.duration_ms)
    );
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(theme.progress).bg(theme.progress_bg))
            .ratio(state.progress_ratio())
            .label(Span::styled(label, theme.body())),
        gauge_area,
    );

    // 点击进度条可跳转：登记命中区，由 app 层按 x 比例换算成目标时间。
    state.add_hit_zone(gauge_area, HitTarget::Progress, 0, 1);
}

/// 播放条第一行：左侧曲目，右侧状态。
fn render_player_header(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let [left, right] =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(34)]).areas(area);

    // 用 ASCII 标记表示播放状态，避免字体缺字
    let marker = match state.playback {
        PlaybackState::Playing => ">>",
        PlaybackState::Paused => "||",
        PlaybackState::Loading => "..",
        PlaybackState::Stopped => "--",
    };

    let title = match state.current.as_ref() {
        Some(song) => format!("{} - {}", song.singer_text(), song.name),
        None => "未在播放（在列表里按 Enter 播放）".to_string(),
    };

    let title_line = Line::from(vec![
        Span::styled(format!("{marker} "), theme.playback(state.playback)),
        Span::styled(
            truncate_to_width(&title, left.width.saturating_sub(4) as usize),
            if state.current.is_some() {
                theme.body()
            } else {
                theme.dim()
            },
        ),
    ]);
    frame.render_widget(Paragraph::new(title_line), left);

    let volume = if state.is_muted() {
        "静音".to_string()
    } else {
        format!("音量 {:.0}%", state.volume * 100.0)
    };
    let meta = format!(
        "{} · {} · {}",
        state.playback.label(),
        volume,
        state.queue.mode().label()
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate_to_width(&meta, right.width as usize),
            theme.dim(),
        ))),
        right,
    );
}

/// 歌词面板。
///
/// 当前行始终垂直居中——这是卡拉OK式滚动的关键：视线固定屏幕中央，
/// 而不是跟着文字往下跑。
pub fn render_lyric(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
    if area.height < 3 || area.width < 8 {
        return;
    }

    let title = match state.current.as_ref() {
        Some(song) => format!("歌词 · {}", song.name),
        None => "歌词".to_string(),
    };

    let block = panel(title, false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // 封面不在这里画：`render_lyric` 被首页/歌词页/封面页共用，封面由各自的
    // `draw_cover_block` 决定放不放、放多大。
    let lyric_area = inner;

    if state.lyric.loading && state.lyric.lyric.is_empty() {
        frame.render_widget(loading_placeholder(theme), lyric_area);
        return;
    }
    if state.lyric.lyric.is_empty() {
        let hint = if state.current.is_some() {
            "暂无歌词"
        } else {
            "播放歌曲后显示歌词"
        };
        frame.render_widget(empty_placeholder(hint, theme), lyric_area);
        return;
    }

    let total = state.lyric.lyric.lines.len();
    let viewport = lyric_area.height as usize;
    let active = state.lyric.active_line;

    // 让当前行居中，同时不允许滚出内容范围
    let focus_line = active.unwrap_or(0);

    // 把每一行歌词展平为「原文 + 译文（若有）」的若干个显示单元。
    // 滚动定位按歌词原文行号找显示位置，所以译文不会破坏对齐。
    let mut display: Vec<(usize, bool, String)> = Vec::with_capacity(total * 2);
    for (index, line) in state.lyric.lyric.lines.iter().enumerate() {
        display.push((index, false, line.text.clone()));
        // 译文与音译**都**显示（各占一行），不再二选一。
        // 日语歌尤其需要：原文看不懂，译文管理解，罗马音管跟唱，两者用途不同。
        if let Some(translation) = line.translation.as_deref() {
            if !translation.trim().is_empty() {
                display.push((index, true, translation.to_string()));
            }
        }
        if let Some(romanization) = line.romanization.as_deref() {
            if !romanization.trim().is_empty() {
                display.push((index, true, romanization.to_string()));
            }
        }
    }

    let focus_display = display
        .iter()
        .position(|(index, _, _)| *index == focus_line)
        .unwrap_or(0);
    let max_offset = display.len().saturating_sub(viewport);
    let offset = focus_display.saturating_sub(viewport / 2).min(max_offset);

    let lines: Vec<Line> = display
        .iter()
        .skip(offset)
        .take(viewport)
        .map(|(index, is_translation, text)| {
            // 译文始终暗一档（包括"当前行"的译文），只有原文才有活动态高亮，
            // 这样一眼能看出「上面那行亮的是原文，下面那行是它的译文」。
            let style = if *is_translation {
                theme.dim()
            } else if Some(*index) == active {
                theme.lyric_active()
            } else {
                theme.lyric_idle()
            };
            Line::from(Span::styled(text.clone(), style))
        })
        .collect();

    // 注意：上面已经用 skip(offset).take(viewport) 裁好了要显示的行，
    // 这里**不能**再调 .scroll((offset, 0))——那会形成双重偏移（实际滚 2×offset），
    // 滚得越来越快，很快就滚过内容末尾，表现就是「歌词播到一半后再也不出现」。
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        lyric_area,
    );
}

/// 播放队列面板需要的外部状态。
///
/// 打包成结构体而不是摊成三个参数：`render_queue` 本来就要接 frame / area /
/// queue / cursor / theme，再散开就超过 clippy 的 7 参数上限了。
#[derive(Debug, Clone, Copy)]
pub struct QueueView<'a> {
    /// 队列面板是否拥有键盘焦点。
    pub focused: bool,
    /// 当前播放曲目的 hash。
    pub current_hash: Option<&'a str>,
    pub playback: PlaybackState,
}

/// 播放队列面板。
pub fn render_queue(
    frame: &mut Frame,
    area: Rect,
    queue: &PlayQueue,
    cursor: &mut ListState,
    view: QueueView<'_>,
    theme: &Theme,
) {
    if area.height < 3 || area.width < 12 {
        return;
    }

    let title = format!("播放队列 · {} 首 · {}", queue.len(), queue.mode().label());
    let block = panel(title, view.focused, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if queue.is_empty() {
        frame.render_widget(
            empty_placeholder("队列为空 · 在列表里按 a 加入", theme),
            inner,
        );
        return;
    }

    let row_width = inner.width.saturating_sub(1) as usize;
    let items: Vec<_> = queue
        .items()
        .iter()
        .enumerate()
        .map(|(index, song)| {
            let context = RowContext {
                width: row_width,
                is_current: view.current_hash == Some(song.hash.as_str()),
                playback: view.playback,
                hover: false,
            };
            song_row(index, song, context, theme)
        })
        .collect();

    let widget = selection_list(items, theme);

    frame.render_stateful_widget(widget, inner, cursor);
}

/// 封面块的行数下限 / 上限。
///
/// 上限不只是审美：图片协议按区域尺寸编码，区域越大单次编码的数据越多，
/// 而封面页最宽也就 48 列左右，再大没有意义。
const COVER_MIN_ROWS: u16 = 6;
const COVER_MAX_ROWS: u16 = 24;

/// 在 `inner` 上方划出一块居中的方形封面区，返回（封面区, 剩余区）。
///
/// 抽成纯函数是为了能直接测：这块几何一变，图片协议就得重新编码（尺寸变了），
/// 值得有断言兜着。
///
/// 字符宽高比约 1:2，所以方形区域的列数取行数的两倍；最多占一半高，
/// 剩下的留给下方内容。区域太小时返回 `None`，调用方把整块都留给内容。
fn cover_layout(inner: Rect) -> Option<(Rect, Rect)> {
    if inner.width < 12 || inner.height < 8 {
        return None;
    }

    let rows = (inner.height / 2).clamp(COVER_MIN_ROWS, COVER_MAX_ROWS);
    let columns = (rows * 2).min(inner.width);
    let [cover_area, rest] =
        Layout::vertical([Constraint::Length(rows + 1), Constraint::Min(1)]).areas(inner);

    // 居中：封面比可用宽度窄时左右留白
    let x = cover_area.x + cover_area.width.saturating_sub(columns) / 2;
    Some((Rect::new(x, cover_area.y, columns, rows), rest))
}

/// 在当前区域的**上半部分**画封面，返回留给下方内容的区域。
///
/// 抽出来是因为「首页」「歌词页」「封面页」都要它。
///
/// 有图形协议就用 `ratatui-image` 的 widget：它把图片写进 ratatui 的 Buffer，
/// 由框架的 diff 统一输出——不再自己往 stdout 写几百 KB 的转义序列（那会阻塞
/// 写入并打乱光标跟踪），而且内容不变时一个字节都不会重发。探测不出终端能力
/// 时才退回字符画。
///
/// 用的是默认的 `Resize::Fit`：等比缩到区域里，**不放大**。所以封面页把区域
/// 给得再大，一张 256 见方的图也只按原始像素铺开，不会被拉成糊图；代价是换
/// 到不同大小的区域（切页、改窗口）要重新编码一次——每页最多一次，不是每帧。
fn draw_cover_block(frame: &mut Frame, inner: Rect, state: &mut AppState, theme: &Theme) -> Rect {
    if !state.cover.is_drawable() {
        return inner;
    }
    let Some((cover_area, rest)) = cover_layout(inner) else {
        return inner;
    };

    if let Some(protocol) = state.cover.protocol.as_mut() {
        frame.render_stateful_widget(StatefulImage::default(), cover_area, protocol);
        // 缩放与编码发生在渲染时（只在区域或图片变化时）。失败只记日志：
        // 下一帧会重试，不该因为一张图把界面搞崩。
        if let Some(Err(error)) = protocol.last_encoding_result() {
            crate::logger::tlog!(crate::logger::LEVEL_WARN, "封面编码失败：{error}");
        }
        return rest;
    }

    let lines: Vec<Line> = state
        .cover
        .lines
        .iter()
        .map(|line| Line::from(Span::styled(line.clone(), theme.now_playing())))
        .collect();
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        cover_area,
    );
    rest
}

/// 普通页面右下角的歌词面板：上方小封面 + 下方歌词。
///
/// 这里的面板**没有自己的边框**（外层已经由调用方画好了），所以不要在内部
/// 再套一层 `panel`。
pub fn render_lyric_panel(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
    if area.height < 3 || area.width < 8 {
        return;
    }
    let rest = draw_cover_block(frame, area, state, theme);
    render_lyric(frame, rest, state, theme);
}

/// 歌词页：上方封面、下方歌词。
///
/// 封面与歌词都是「当前这首歌」的信息，放一起语义最顺。组合方式与首页一致，
/// 都走 `draw_cover_block`。
pub fn render_lyrics_page(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
    let block = panel("歌词", false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 4 || inner.width < 8 {
        return;
    }

    let rest = draw_cover_block(frame, inner, state, theme);
    render_lyric(frame, rest, state, theme);
}

/// 封面页：整块主区只放封面，配上曲名与歌手。
pub fn render_cover_page(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
    let block = panel("封面", false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 8 {
        return;
    }

    let Some(song) = state.current.as_ref() else {
        frame.render_widget(empty_placeholder("播放歌曲后显示封面", theme), inner);
        return;
    };
    // 先把要显示的文字取出来，释放对 state 的不可变借用——
    // 下面 draw_cover_block 要可变借用它（图片协议状态是可变的）
    let title = song.name.clone();
    let subtitle = format!("{} · {}", song.singer_text(), song.album_name);

    // 先把曲名信息留在底部一行，封面占其余空间
    let [cover_area, info_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);

    let _ = draw_cover_block(frame, cover_area, state, theme);

    let info = vec![
        Line::from(Span::styled(title, theme.now_playing())),
        Line::from(Span::styled(subtitle, theme.dim())),
    ];
    frame.render_widget(Paragraph::new(info).alignment(Alignment::Center), info_area);
}

/// 首页：正在播放的总览——封面在左，曲目信息与歌词在右。
pub fn render_home(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
    let block = panel("正在播放", false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 4 || inner.width < 20 {
        return;
    }

    if state.current.is_none() {
        frame.render_widget(
            empty_placeholder("还没有播放任何歌曲 · 去搜索页按 / 找一首", theme),
            inner,
        );
        return;
    }

    // 宽屏左右分栏（封面 | 歌词），窄屏上下堆叠——和歌词页的断点保持一致
    if inner.width >= 60 {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(45), Constraint::Min(24)]).areas(inner);
        let left_block = panel("封面", false, theme);
        let left_inner = left_block.inner(left);
        frame.render_widget(left_block, left);
        let _ = draw_cover_block(frame, left_inner, state, theme);
        render_lyric(frame, right, state, theme);
    } else {
        let rest = draw_cover_block(frame, inner, state, theme);
        render_lyric(frame, rest, state, theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::ThemeName;

    /// 封面区是「列 = 行 × 2」的方形（字符宽高比 1:2），并水平居中。
    #[test]
    fn cover_block_is_twice_as_wide_as_tall_and_centered() {
        let inner = Rect::new(0, 0, 60, 20);
        let (cover, _rest) = cover_layout(inner).expect("60x20 够放封面");

        assert_eq!(cover.height, 10, "最多占一半高");
        assert_eq!(cover.width, 20, "列数是行数的两倍");
        assert_eq!(cover.x, 20, "剩余宽度左右平分");
        assert_eq!(cover.y, inner.y);
    }

    /// 区域太小就整块留给内容——半个封面对阅读毫无帮助。
    #[test]
    fn cover_block_is_skipped_when_area_is_tiny() {
        assert!(cover_layout(Rect::new(0, 0, 11, 20)).is_none());
        assert!(cover_layout(Rect::new(0, 0, 60, 7)).is_none());
    }

    /// 行数被夹在 [6, 24]：矮区域不至于缩成一条，高区域也不会把内容挤没。
    #[test]
    fn cover_rows_are_clamped() {
        let (short, _) = cover_layout(Rect::new(0, 0, 40, 8)).expect("8 行够放最小封面");
        assert_eq!(short.height, COVER_MIN_ROWS);

        let (tall, _) = cover_layout(Rect::new(0, 0, 80, 100)).expect("100 行够放封面");
        assert_eq!(tall.height, COVER_MAX_ROWS);
        assert_eq!(tall.width, 48);
    }

    /// 窄区域里宽度是硬约束：宁可矮一点也不让封面超出边界。
    #[test]
    fn cover_width_is_capped_by_area_width() {
        let (cover, _) = cover_layout(Rect::new(0, 0, 14, 20)).expect("14x20 够放封面");
        assert_eq!(cover.width, 14, "10 行本该要 20 列，被宽度压到 14");
        assert_eq!(cover.x, 0);
    }

    /// 端到端：封面真的画进了 ratatui 的 Buffer。
    ///
    /// 用 `TestBackend` 就是为了能拿到绘制后的 Buffer——「有没有偷偷写 stdout」
    /// 在真实终端上根本无从断言，而那正是之前卡死与闪烁的来源。
    /// 这里选 `Picker::halfblocks()`：它不碰 stdio，测试里能稳定跑。
    #[test]
    fn cover_is_rendered_into_the_frame_buffer() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        // 渐变而不是纯色：半块字符在上下两格同色时会退化成空格（用底色表示），
        // 纯色图渲染出来就是一片空白，测不出东西。
        let mut pixels = image::RgbImage::new(32, 32);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x * 8) as u8, (y * 8) as u8, 128]);
        }
        let protocol = ratatui_image::picker::Picker::halfblocks()
            .new_resize_protocol(image::DynamicImage::ImageRgb8(pixels));

        let mut state = AppState::new(crate::config::Config::default());
        state.cover = crate::app::state::CoverArt {
            hash: Some("test-hash".to_string()),
            lines: Vec::new(),
            protocol: Some(protocol),
        };

        let area = Rect::new(0, 0, 60, 20);
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).expect("测试后端可用");
        let mut rest = Rect::default();
        let drawn = terminal
            .draw(|frame| {
                let theme = Theme::for_config(ThemeName::Default, false);
                rest = draw_cover_block(frame, area, &mut state, &theme);
            })
            .expect("绘制成功");

        assert_eq!(rest.y, 11, "下方内容从封面（10 行 + 1 行间距）之后开始");

        // 落在封面区里的非空格单元格：全是空格就说明图根本没进去
        let painted = drawn
            .buffer
            .content()
            .iter()
            .filter(|cell| cell.symbol() != " ")
            .count();
        assert!(painted > 0, "封面应当写进 Buffer，而不是 stdout");
    }

    /// 根治点：区域不变时**不会**重新编码。
    ///
    /// 之前的病根就是每帧重发——474KB 的转义序列堵死 stdout。这里直接断言
    /// 「第二帧没有任何编码动作」：不编码就没有新的图片数据，ratatui 的 diff
    /// 也就无从输出，自然不会阻塞写入、也不会闪。
    /// 这里直接驱动 widget 而不是走 `draw_cover_block`：后者为了记日志会把
    /// `last_encoding_result()` 取走（它是 `take()` 语义），读不到编码次数。
    #[test]
    fn cover_is_encoded_only_when_the_area_changes() {
        use ratatui::buffer::Buffer;
        use ratatui::widgets::StatefulWidget;

        // 尺寸要贴近真实封面（256 见方）。`Resize::Fit` 不会把小图放大，
        // 用一张装得下图会让「换区域要重新编码」这条永远成立不了。
        let mut pixels = image::RgbImage::new(256, 256);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            *pixel = image::Rgb([x as u8, y as u8, 128]);
        }
        let mut protocol = ratatui_image::picker::Picker::halfblocks()
            .new_resize_protocol(image::DynamicImage::ImageRgb8(pixels));

        // 交给 widget 的区域由纯函数算出，因此连续两帧必然是同一个矩形——
        // 区域稳定是「不重发」的前提
        let (first_area, _) = cover_layout(Rect::new(0, 0, 60, 20)).expect("够放封面");
        let (second_area, _) = cover_layout(Rect::new(0, 0, 60, 20)).expect("够放封面");
        assert_eq!(first_area, second_area);

        let mut first = Buffer::empty(first_area);
        StatefulImage::default().render(first_area, &mut first, &mut protocol);
        assert!(
            protocol.last_encoding_result().is_some(),
            "首帧必须编码一次"
        );

        let mut second = Buffer::empty(second_area);
        StatefulImage::default().render(second_area, &mut second, &mut protocol);
        assert!(
            protocol.last_encoding_result().is_none(),
            "区域没变就不该再编码——每帧编码正是之前卡死的原因"
        );
        assert_eq!(
            first, second,
            "两帧内容一致 → ratatui 的 diff 一个字节都不会输出"
        );

        // 换到更大的区域才重新编码一次：切页 / 改窗口大小走的就是这条路
        let (bigger, _) = cover_layout(Rect::new(0, 0, 60, 40)).expect("够放封面");
        assert_ne!(bigger.height, first_area.height);
        let mut third = Buffer::empty(bigger);
        StatefulImage::default().render(bigger, &mut third, &mut protocol);
        assert!(
            protocol.last_encoding_result().is_some(),
            "区域变了应当重新编码一次"
        );
    }

    /// 剩余区域紧接封面下方，且两者高度加起来仍是原区域高度。
    #[test]
    fn remainder_sits_below_the_cover() {
        let inner = Rect::new(3, 5, 60, 20);
        let (cover, rest) = cover_layout(inner).expect("60x20 够放封面");

        // rows + 1：多留一行当间距，不然封面和下面的内容会糊在一起
        assert_eq!(rest.y, cover.y + cover.height + 1);
        assert_eq!(rest.height, inner.height - cover.height - 1);
        assert_eq!(rest.width, inner.width, "剩余区用满宽度");
    }
}
