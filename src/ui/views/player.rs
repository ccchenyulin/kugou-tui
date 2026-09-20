//! 播放条、歌词面板、播放队列。
//!
//! 三者共同回答用户最关心的三个问题：**在放什么**（播放条）、**唱到哪了**
//! （歌词）、**接下来放什么**（队列）。

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, ListState, Paragraph};

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

    // 封面不在这里画：`cover_area` 只能有一个写入点（见 draw_cover_block），
    // 多个渲染函数抢着写会让区域被覆盖、进而让图片每帧重发。
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

/// 在当前区域的**上半部分**画封面，返回留给下方内容的区域。
///
/// 抽出来是因为「首页」和「歌词页」都要它。kitty 终端把真图铺在预留区域上
/// （渲染完成后由 `App::paint_cover` 放），其它终端退回字符画。
fn draw_cover_block(frame: &mut Frame, inner: Rect, state: &mut AppState, theme: &Theme) -> Rect {
    if state.cover.lines.is_empty() || inner.width < 12 || inner.height < 8 {
        return inner;
    }

    // 字符宽高比约 1:2，方形区域的列数是行数的两倍；最多占一半高，留一行间距
    let rows = (inner.height / 2).clamp(6, 24);
    let columns = (rows * 2).min(inner.width);
    let [cover_area, rest] =
        Layout::vertical([Constraint::Length(rows + 1), Constraint::Min(1)]).areas(inner);

    let x = cover_area.x + cover_area.width.saturating_sub(columns) / 2;
    state.cover_area = Some(Rect::new(x, cover_area.y, columns, rows));

    if !crate::ui::kitty::is_supported() {
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
    }
    rest
}

/// 歌词页：上方封面、下方歌词。
///
/// 封面与歌词都是「当前这首歌」的信息，放一起语义最顺。组合方式与首页一致，
/// 都走 `draw_cover_block` —— 保证 `cover_area` 只有一个写入点。
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

    // 同上：清空交给 ui::render 每帧统一做，这里只管设置
    let Some(song) = state.current.as_ref() else {
        frame.render_widget(empty_placeholder("播放歌曲后显示封面", theme), inner);
        return;
    };
    // 先把要显示的文字取出来，释放对 state 的不可变借用——
    // 下面 draw_cover_block 要可变借用它（记录封面区域）
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
