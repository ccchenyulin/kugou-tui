//! 播放条、歌词面板、播放队列。
//!
//! 三者共同回答用户最关心的三个问题：**在放什么**（播放条）、**唱到哪了**
//! （歌词）、**接下来放什么**（队列）。

use image::imageops::FilterType;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, ListState, Paragraph};
use ratatui_image::{FontSize, Resize, StatefulImage};

use crate::api::model::{WordState, format_duration_ms};
use crate::app::queue::PlayQueue;
use crate::app::state::{AppState, HitTarget};
use crate::audio::engine::PlaybackState;
use crate::config::CoverFill;
use crate::ui::theme::Theme;
use crate::ui::views::{empty_placeholder, loading_placeholder};
use crate::ui::widgets::{RowContext, panel, selection_list, song_row, truncate_to_width};

/// 播放条高度：2 行内容 + 上下边框。
///
/// 2 行 = 曲目信息 / 进度条。
///
/// 这里**刻意不放封面**：播放条总共才 4 行，给封面最多 8×4 格——那个尺寸下
/// 专辑图只是一团糊色，既看不清又白占宽度。封面挪到「首页」和「歌词」页，
/// 那里有整块区域可以按真实比例放大（见 `draw_cover_block`）。
pub const PLAYER_HEIGHT: u16 = 4;

/// 播放条：封面 + 曲目信息 + 进度条 + 下一首。
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

    // 两行：曲目信息 / 进度条。高度只有 1 行时（极端窄终端）只给进度条。
    if inner.height >= 2 {
        let [info_area, gauge_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
        render_song_info(frame, info_area, state, theme);
        render_progress(frame, gauge_area, state, theme);
    } else {
        render_progress(frame, inner, state, theme);
    }
}

/// 第 1 行：播放状态 + 歌名 · 歌手 · 专辑（左），下一首与音量/模式（右）。
///
/// 播放条只有两行，所以把歌手、专辑并进歌名那一行——拆成独立一行的话进度条
/// 就得再让一行出来，列表能显示的内容反而更少。右侧那一小段用暗色，不抢歌名。
fn render_song_info(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let marker = match state.playback {
        PlaybackState::Playing => crate::ui::icons::now_playing(true),
        PlaybackState::Paused => crate::ui::icons::now_playing(false),
        PlaybackState::Loading => crate::ui::icons::loading(),
        PlaybackState::Stopped => crate::ui::icons::stopped(),
    };

    let (left_text, emphasis) = match state.current.as_ref() {
        Some(song) => {
            let mut text = song.name.clone();
            let singer = song.singer_text();
            if !singer.is_empty() {
                text.push_str(" · ");
                text.push_str(&singer);
            }
            if !song.album_name.is_empty() {
                text.push_str(" · ");
                text.push_str(&song.album_name);
            }
            (text, theme.title())
        }
        None => ("未在播放（在列表里按 Enter 播放）".to_string(), theme.dim()),
    };

    // 右侧：下一首 + 音量 + 循环模式。宽度不够就整段不画，别把歌名挤没了。
    let right_text = next_up_text(state);
    let right_width = (right_text.chars().count() as u16).min(area.width / 2);
    let show_right = !right_text.is_empty() && area.width >= 60;

    let (left_area, right_area) = if show_right {
        let [left, right] =
            Layout::horizontal([Constraint::Min(20), Constraint::Length(right_width)])
                .spacing(1)
                .areas(area);
        (left, Some(right))
    } else {
        (area, None)
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{marker} "), theme.playback(state.playback)),
            Span::styled(
                truncate_to_width(&left_text, left_area.width.saturating_sub(3) as usize),
                emphasis,
            ),
        ])),
        left_area,
    );

    if let Some(right_area) = right_area {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                truncate_to_width(&right_text, right_area.width as usize),
                theme.dim(),
            )))
            .alignment(Alignment::Right),
            right_area,
        );
    }
}

/// 播放条右侧那一小段：「下一首 X · 播放中 · 音量 80% · 顺序」。
///
/// 拼一整串再整体截断，而不是各段分别截——分别截会出现「下一首 稻… · 播」这种
/// 两半都被切坏的残句。
fn next_up_text(state: &AppState) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(next) = state.queue.peek_next() {
        parts.push(format!(
            "{} 下一首 {}",
            crate::ui::icons::next_up(),
            next.name
        ));
    }
    parts.push(state.playback.label().to_string());
    parts.push(if state.is_muted() {
        "静音".to_string()
    } else {
        format!("音量 {:.0}%", state.volume * 100.0)
    });
    parts.push(state.queue.mode().label().to_string());

    parts.join(" · ")
}

/// 第 3 行：进度条。自带居中标签，把时间放在条上。
fn render_progress(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
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
        area,
    );

    // 点击进度条可跳转：登记命中区，由 app 层按 x 比例换算成目标时间。
    state.add_hit_zone(area, HitTarget::Progress, 0, 1);
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

    // 逐字着色要用当前播放位置，取一次即可（毫秒）
    let position_ms = state.position_ms;

    let lines: Vec<Line> = display
        .iter()
        .skip(offset)
        .take(viewport)
        .map(|(index, is_translation, text)| {
            // 译文始终暗一档（包括"当前行"的译文），只有原文才有活动态高亮，
            // 这样一眼能看出「上面那行亮的是原文，下面那行是它的译文」。
            if *is_translation {
                return Line::from(Span::styled(text.clone(), theme.dim()));
            }
            if Some(*index) != active {
                return Line::from(Span::styled(text.clone(), theme.lyric_idle()));
            }

            // 当前行：拿得到逐字时间戳就逐字染色（唱到哪亮到哪），
            // 拿不到就退回整行高亮——绝不为了效果让歌词和时间错位。
            let words = state
                .lyric
                .lyric
                .lines
                .get(*index)
                .map(|line| line.words.as_slice())
                .unwrap_or(&[]);
            if words.len() != text.chars().count() {
                return Line::from(Span::styled(text.clone(), theme.lyric_active()));
            }

            let spans: Vec<Span> = text
                .chars()
                .zip(words.iter())
                .map(|(character, word)| {
                    let style = match word.state_at(position_ms) {
                        WordState::Sung => theme.lyric_sung(),
                        WordState::Singing => theme.lyric_singing(),
                        WordState::Pending => theme.lyric_pending(),
                    };
                    Span::styled(character.to_string(), style)
                })
                .collect();
            Line::from(spans)
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

/// 缩略图封面的行数下限 / 上限。
///
/// 上限不只是审美：图片协议按区域尺寸编码，区域越大单次编码的数据越多，
/// 而缩略图那块本来就窄，再大没有意义。
const COVER_MIN_ROWS: u16 = 6;
const COVER_MAX_ROWS: u16 = 32;

/// 封面块在区域里怎么摆。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoverPlace {
    /// 只占区域上半部分，按图片比例居中（歌词面板上方、窄屏首页的封面）。
    ///
    /// 这块只有几十个字符大，裁剪只会更看不清，所以固定「完整显示」，
    /// 不跟随 [`CoverFill`] 配置。
    AboveContent,
    /// 铺满整块区域（宽屏首页左栏那块大封面），怎么铺由配置决定。
    Fill,
}

/// 在 `inner` 上方划出一块居中的封面缩略图区，返回（缩略图区, 剩余区）。
///
/// 抽成纯函数是为了能直接测：这块几何一变，图片协议就得重新编码（尺寸变了），
/// 值得有断言兜着。
///
/// 字符宽高比约 1:2，所以方形区域的列数取行数的两倍；最多占一半高，
/// 剩下的留给下方内容。区域太小时返回 `None`，调用方把整块都留给内容。
fn thumbnail_layout(inner: Rect, aspect: f32, cell_aspect: f32) -> Option<(Rect, Rect)> {
    if inner.width < 12 || inner.height < 8 {
        return None;
    }

    // 图被压成 0 宽会让下面的除法炸掉；宽高比拿不到时按方图算
    let aspect = aspect.max(0.05);
    let rows = (inner.height / 2).clamp(COVER_MIN_ROWS, COVER_MAX_ROWS);

    // 按真实比例算列数（横图列数大于行数）。缩略图**不追求填满**——它本来就只占
    // 上半部分，左右留白反而显得居中。真要填满的是首页那块，走 `prepare_cover`
    // 的裁剪路径。
    let columns = ((f32::from(rows) * aspect * cell_aspect).round() as u16).clamp(1, inner.width);

    // 太宽就按 inner.width 反算（缩略图不能横向溢出）
    let rows = if columns >= inner.width {
        let rows = (f32::from(inner.width) / (aspect * cell_aspect)).round() as u16;
        rows.clamp(COVER_MIN_ROWS, inner.height.saturating_sub(1))
    } else {
        rows
    };

    let [_cover_area, rest] =
        Layout::vertical([Constraint::Length(rows + 1), Constraint::Min(1)]).areas(inner);

    let x = inner.x + inner.width.saturating_sub(columns) / 2;
    Some((Rect::new(x, inner.y, columns, rows), rest))
}

/// 区域换算成像素尺寸：`列 × 单元格宽`, `行 × 单元格高`。
///
/// **必须用 `Picker` 报的 `FontSize`**——图片协议内部就是按它把像素折回单元格的。
/// 这里换个比例算，裁出来的图比例就对不上，渲染时又会留边。
fn pixel_size(area: Rect, font: FontSize) -> (u32, u32) {
    (
        (u32::from(area.width) * u32::from(font.width)).max(1),
        (u32::from(area.height) * u32::from(font.height)).max(1),
    )
}

/// 等比放大到**盖住** `(width, height)`，再居中裁到正好这个尺寸。
///
/// 等价 CSS 的 `object-fit: cover`：铺满 100%、不变形，代价是裁掉溢出的边。
fn crop_to_cover(image: &image::DynamicImage, (width, height): (u32, u32)) -> image::DynamicImage {
    let src_w = image.width().max(1);
    let src_h = image.height().max(1);
    // 取较大的那个比例：两个方向都要盖住，取小的会留边
    let scale = f64::max(
        f64::from(width) / f64::from(src_w),
        f64::from(height) / f64::from(src_h),
    );
    // 向上取整：宁可多放大半个像素，也不能因为取整让某一边差一点盖不满
    let scaled_w = (f64::from(src_w) * scale).ceil().max(f64::from(width)) as u32;
    let scaled_h = (f64::from(src_h) * scale).ceil().max(f64::from(height)) as u32;

    let scaled = image.resize_exact(scaled_w, scaled_h, FilterType::Lanczos3);
    scaled.crop_imm(
        (scaled_w - width) / 2,
        (scaled_h - height) / 2,
        width,
        height,
    )
}

/// 直接拉到目标尺寸——**不保持比例**，只给 [`CoverFill::Stretch`] 用。
fn stretch_to(image: &image::DynamicImage, (width, height): (u32, u32)) -> image::DynamicImage {
    image.resize_exact(width, height, FilterType::Lanczos3)
}

/// 在 `area` 里找出与图片像素比例一致的最大矩形，居中放置。
fn fit_box(image: &image::DynamicImage, area: Rect, font: FontSize) -> Rect {
    let image_aspect = image.width().max(1) as f32 / image.height().max(1) as f32;
    // 单元格是「高 : 宽 = font.height : font.width」，换算成列数要乘上去
    let cell_aspect = f32::from(font.height) / f32::from(font.width.max(1));

    let mut columns = (f32::from(area.height) * image_aspect * cell_aspect).round() as u16;
    let mut rows = area.height;
    if columns > area.width {
        columns = area.width;
        rows = (f32::from(columns) / (image_aspect * cell_aspect)).round() as u16;
    }
    let columns = columns.clamp(1, area.width);
    let rows = rows.clamp(1, area.height);

    Rect::new(
        area.x + (area.width - columns) / 2,
        area.y + (area.height - rows) / 2,
        columns,
        rows,
    )
}

/// 按 `mode` 把 `image` 塞进 `area`，返回（交给图片协议的图, 真正要渲染的矩形）。
///
/// # 为什么必须自己预处理像素
///
/// `ratatui-image` 的三种 `Resize` **全都保持宽高比**：
///
/// * `Fit` 是「装得下就不放大」；
/// * `Scale` 是「允许放大」，但仍然等比；
/// * `Crop` 甚至不放大，图比区域小就原样画。
///
/// 而封面区的像素比例（列 × 单元格宽 : 行 × 单元格高）几乎永远不等于图片比例，
/// 于是不管选哪个，图都只占区域的一部分，剩下的地方是空的——**这才是「封面没有
/// 完全填满」的真正原因，不是没放大**。想真正铺满，只能先把图裁/拉到与区域完全
/// 一致的比例，再交给它渲染。
pub fn prepare_cover(
    image: &image::DynamicImage,
    area: Rect,
    font: FontSize,
    mode: CoverFill,
) -> (image::DynamicImage, Rect) {
    match mode {
        CoverFill::Crop => (crop_to_cover(image, pixel_size(area, font)), area),
        CoverFill::Stretch => (stretch_to(image, pixel_size(area, font)), area),
        // 不裁不拉：把「要渲染的区域」缩到图片自己的比例，居中放进 area
        CoverFill::Fit => (image.clone(), fit_box(image, area, font)),
    }
}

/// 画封面，返回留给下方内容的区域。
///
/// 首页与歌词面板都要它，区别只在 [`CoverPlace`]。
///
/// 图片走 `ratatui-image` 的 widget：它把图写进 ratatui 的 Buffer，由框架的
/// diff 统一输出——不再自己往 stdout 写几百 KB 的转义序列（那会阻塞写入并打乱
/// 光标跟踪），而且内容不变时一个字节都不会重发。
fn draw_cover_block(
    frame: &mut Frame,
    inner: Rect,
    state: &mut AppState,
    place: CoverPlace,
) -> Rect {
    if !state.cover.is_drawable() {
        return inner;
    }

    // 字符高宽比：配的 qr_aspect 就是「字符高:宽」，同一个概念，直接复用。
    // 注意它只决定**框的形状**（看起来是不是方的）；裁图用的像素尺寸另算，
    // 那个必须跟图片协议内部的 `FontSize` 一致，见 `pixel_size`。
    let cell_aspect = state.config.qr_aspect.max(0.1);

    let (mode, area, rest) = match place {
        CoverPlace::AboveContent => {
            let Some((box_area, rest)) = thumbnail_layout(inner, state.cover.aspect, cell_aspect)
            else {
                return inner;
            };
            (CoverFill::Fit, box_area, rest)
        }
        CoverPlace::Fill => {
            let mode = state.config.cover_fill;
            (mode, inner, inner)
        }
    };

    if let Some(picker) = state.picker.as_ref() {
        if let Some((protocol, render_area)) = state.cover.fit_to(mode, area, picker) {
            // `Scale`：允许放大，等比铺到 `render_area`。图的比例已经被
            // `prepare_cover` 对齐到区域了，所以这里正好铺满、不留边。
            frame.render_stateful_widget(
                StatefulImage::default().resize(Resize::Scale(None)),
                render_area,
                protocol,
            );
            // 编码发生在渲染时（只在区域或图片变化时）。失败只记日志：
            // 下一帧会重试，不该因为一张图把界面搞崩。
            if let Some(Err(error)) = protocol.last_encoding_result() {
                crate::logger::tlog!(crate::logger::LEVEL_WARN, "封面编码失败：{error}");
            }
        }
    }
    // 没有终端图形能力（`picker` 为空）或没有原图时留白——画不出东西比画错好
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
    let rest = draw_cover_block(frame, area, state, CoverPlace::AboveContent);
    render_lyric(frame, rest, state, theme);
}

/// 头像占的列数：6 行内容 × 字符高宽比 2 = 12 列。
const AVATAR_COLUMNS: u16 = 12;

/// 首页账号区的高度：边框 2 行 + 内容 6 行（头像要能看清，2 行只能画 4 列宽）。
///
/// 固定值，不能被封面框挤掉——封面用剩下的空间。
const ACCOUNT_HEIGHT: u16 = 8;

/// 首页左下角的账号区：头像 + 昵称 · 等级 · 累计听歌时长。
///
/// **刻意不放**粉丝数、关注数、访客、星座、勋章——`/user/detail` 返回十几个
/// 字段，全摆上来这里就成了数据表，而首页要回答的是「在放什么」。只留一眼
/// 能读完的三项。
fn render_account(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
    let block = panel("我的资料", false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width < 8 {
        return;
    }

    let Some(info) = state.user_info.clone() else {
        // 已登录但资料还没回来时显示「加载中」——直接写「未登录」会让人以为
        // 登录掉了，其实只是这一秒还没到。
        let text = if state.logged_in {
            "加载中…"
        } else {
            "未登录（按 L 扫码）"
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                truncate_to_width(text, inner.width as usize),
                theme.dim(),
            )),
            inner,
        );
        return;
    };

    // 左头像 / 右文字。窄到放不下头像就整块给文字
    let (avatar_area, text_area) = if inner.width >= AVATAR_COLUMNS + 14 {
        let [left, right] =
            Layout::horizontal([Constraint::Length(AVATAR_COLUMNS), Constraint::Min(10)])
                .spacing(1)
                .areas(inner);
        (Some(left), right)
    } else {
        (None, inner)
    };

    if let Some(avatar_area) = avatar_area {
        if let Some(protocol) = state.avatar.protocol.as_mut() {
            frame.render_stateful_widget(StatefulImage::default(), avatar_area, protocol);
            if let Some(Err(error)) = protocol.last_encoding_result() {
                crate::logger::tlog!(crate::logger::LEVEL_WARN, "头像编码失败：{error}");
            }
        }
    }

    // 昵称 + 等级
    let mut lines: Vec<Line> = Vec::new();
    let name = if info.nickname.is_empty() {
        "（无名）".to_string()
    } else {
        info.nickname.clone()
    };
    lines.push(Line::from(vec![
        Span::styled(
            truncate_to_width(&name, text_area.width.saturating_sub(8) as usize),
            theme.title(),
        ),
        Span::styled(
            info.grade
                .map(|grade| format!("  Lv.{grade}"))
                .unwrap_or_default(),
            theme.dim(),
        ),
    ]));

    // 会员摘要（有就显示）
    if let Some(label) = state.vip_label.as_deref() {
        lines.push(Line::from(Span::styled(
            truncate_to_width(label, text_area.width as usize),
            theme.now_playing(),
        )));
    }

    // 累计听歌时长
    if let Some(duration) = info.duration_text() {
        lines.push(Line::from(Span::styled(
            truncate_to_width(&format!("听过 {duration}"), text_area.width as usize),
            theme.dim(),
        )));
    }

    frame.render_widget(Paragraph::new(lines), text_area);
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

    // 宽屏左右分栏（封面 | 歌词），窄屏上下堆叠
    if inner.width >= 60 {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(45), Constraint::Min(24)]).areas(inner);

        // 左栏竖着切：上方封面（铺满整块）、下方账号信息（固定 8 行，不能被挤掉）。
        //
        // 之前让封面框按图片比例自己算高度，结果框能把整个左栏吃掉，
        // 账号区高度变成 0 —— 用户看到「个人信息被挤掉了」。
        // 现在账号区固定，封面用剩下的全部空间，按 `cover_fill` 铺满
        // （见 `prepare_cover`）。
        let [cover_col, account_col] =
            Layout::vertical([Constraint::Min(6), Constraint::Length(ACCOUNT_HEIGHT)]).areas(left);
        let left_block = panel("封面", false, theme);
        let left_inner = left_block.inner(cover_col);
        frame.render_widget(left_block, cover_col);
        draw_cover_block(frame, left_inner, state, CoverPlace::Fill);
        render_account(frame, account_col, state, theme);
        render_lyric(frame, right, state, theme);
    } else {
        // 窄屏没有左右分栏的余地：封面缩到上半部分，下半部分留给歌词
        let rest = draw_cover_block(frame, inner, state, CoverPlace::AboveContent);
        render_lyric(frame, rest, state, theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一张纯色图，用来断言「裁完的尺寸对不对」。
    fn solid(width: u32, height: u32) -> image::DynamicImage {
        image::DynamicImage::ImageRgb8(image::RgbImage::new(width, height))
    }

    /// 缩略图区是「列 = 行 × 2」的方形（字符宽高比 1:2），并水平居中。
    #[test]
    fn thumbnail_is_twice_as_wide_as_tall_and_centered() {
        let inner = Rect::new(0, 0, 60, 20);
        let (cover, _rest) = thumbnail_layout(inner, 1.0, 2.0).expect("60x20 够放缩略图");

        assert_eq!(cover.height, 10, "最多占一半高");
        assert_eq!(cover.width, 20, "列数是行数的两倍");
        assert_eq!(cover.x, 20, "剩余宽度左右平分");
        assert_eq!(cover.y, inner.y);
    }

    /// 区域太小就整块留给内容——半个缩略图对阅读毫无帮助。
    #[test]
    fn thumbnail_is_skipped_when_area_is_tiny() {
        assert!(thumbnail_layout(Rect::new(0, 0, 11, 20), 1.0, 2.0).is_none());
        assert!(thumbnail_layout(Rect::new(0, 0, 60, 7), 1.0, 2.0).is_none());
    }

    /// 行数被夹在 [6, 32]：矮区域不至于缩成一条，高区域也不会把内容挤没。
    #[test]
    fn thumbnail_rows_are_clamped() {
        let (short, _) = thumbnail_layout(Rect::new(0, 0, 40, 8), 1.0, 2.0).expect("8 行够放");
        assert_eq!(short.height, COVER_MIN_ROWS);

        let (tall, _) = thumbnail_layout(Rect::new(0, 0, 80, 100), 1.0, 2.0).expect("100 行够放");
        assert_eq!(tall.height, COVER_MAX_ROWS);
        assert_eq!(tall.width, 64, "32 行 × 2（方图 + 字符 2:1）");
    }

    /// 非正方形封面按真实比例算列数——16:9 的头图不该被压成方的。
    #[test]
    fn thumbnail_respects_image_aspect_ratio() {
        let (square, _) = thumbnail_layout(Rect::new(0, 0, 80, 20), 1.0, 2.0).expect("够放");
        let (wide, _) = thumbnail_layout(Rect::new(0, 0, 80, 20), 16.0 / 9.0, 2.0).expect("够放");

        assert!(
            wide.width > square.width,
            "16:9 的图应该比方图宽：wide={} square={}",
            wide.width,
            square.width
        );

        // 竖图（比如 3:4 的歌手照）应该更窄
        let (tall, _) = thumbnail_layout(Rect::new(0, 0, 80, 20), 0.75, 2.0).expect("够放");
        assert!(tall.width < square.width, "竖图应该比方图窄");
    }

    /// 窄区域里宽度是硬约束：宁可矮一点也不让缩略图超出边界。
    #[test]
    fn thumbnail_width_is_capped_by_area_width() {
        let (cover, _) = thumbnail_layout(Rect::new(0, 0, 14, 20), 1.0, 2.0).expect("够放");
        assert_eq!(cover.width, 14, "10 行本该要 20 列，被宽度压到 14");
        assert_eq!(cover.x, 0);
    }

    /// 剩余区域紧接缩略图下方，且两者高度加起来仍是原区域高度。
    #[test]
    fn remainder_sits_below_the_thumbnail() {
        let inner = Rect::new(3, 5, 60, 20);
        let (cover, rest) = thumbnail_layout(inner, 1.0, 2.0).expect("够放");

        // rows + 1：多留一行当间距，不然缩略图和下面的内容会糊在一起
        assert_eq!(rest.y, cover.y + cover.height + 1);
        assert_eq!(rest.height, inner.height - cover.height - 1);
        assert_eq!(rest.width, inner.width, "剩余区用满宽度");
    }

    // ---- 铺满（`prepare_cover`）：这是「封面填不满」的根治点 ----

    /// 裁剪模式的**核心不变量**：裁完的图，像素比例与目标区域完全一致。
    ///
    /// 只要这一条成立，`ratatui-image` 的等比缩放就会正好铺满整个区域，不留黑边。
    /// 以前填不满就是因为区域是 45% 宽 × 剩下高（像素比例约 1.5:1），而图是方的。
    #[test]
    fn crop_matches_the_area_pixel_ratio_exactly() {
        let font = FontSize::new(10, 20);
        // 典型的宽扁封面区：34 列 × 11 行 → 340 × 220 像素
        let area = Rect::new(0, 0, 34, 11);
        let (width, height) = pixel_size(area, font);
        assert_eq!((width, height), (340, 220));

        let cropped = crop_to_cover(&solid(256, 256), (width, height));
        assert_eq!(cropped.width(), 340, "裁完正好是区域的像素宽");
        assert_eq!(cropped.height(), 220, "裁完正好是区域的像素高");
    }

    /// 裁剪模式是「盖住再裁」：短边不裁，长边裁掉，且两侧对称。
    #[test]
    fn crop_keeps_the_short_side_and_centers_the_overflow() {
        // 100x100 的方图 → 目标 200x100（宽是高的两倍）
        // 等比放大到盖住 → 200x200，再上下各裁 50 行
        let cropped = crop_to_cover(&solid(100, 100), (200, 100));
        assert_eq!((cropped.width(), cropped.height()), (200, 100));
    }

    /// 目标比原图小也要正确缩小（下载的封面 256 见方，区域常常比它小）。
    #[test]
    fn crop_also_shrinks() {
        let cropped = crop_to_cover(&solid(256, 256), (60, 40));
        assert_eq!((cropped.width(), cropped.height()), (60, 40));
    }

    /// 拉伸模式：直接变成目标尺寸，不保持比例。
    #[test]
    fn stretch_resizes_without_keeping_the_ratio() {
        let stretched = stretch_to(&solid(100, 400), (200, 100));
        assert_eq!((stretched.width(), stretched.height()), (200, 100));
    }

    /// 完整显示模式：区域缩到图片比例，居中，绝不超出区域。
    #[test]
    fn fit_box_matches_the_image_ratio_and_stays_inside() {
        let font = FontSize::new(10, 20);
        let area = Rect::new(0, 0, 34, 11);

        // 方图：列 = 行 × 1（图） × 2（字符高宽比） = 22，水平居中
        let square = fit_box(&solid(100, 100), area, font);
        assert_eq!((square.width, square.height), (22, 11));
        assert_eq!(square.x, (34 - 22) / 2, "居中");
        assert_eq!(square.y, 0);

        // 16:9 的横图：列 = 11 × 1.778 × 2 ≈ 39，比区域宽 → 反过来按宽算行
        let wide = fit_box(&solid(160, 90), area, font);
        assert!(wide.width <= area.width && wide.height <= area.height);
        assert!(wide.width > square.width, "横图应该更宽");
    }

    /// 三种模式都不该让图溢出区域——溢出会被 ratatui 裁掉，看起来就是「图缺了一块」。
    #[test]
    fn every_mode_stays_within_the_area() {
        let font = FontSize::new(10, 20);
        let area = Rect::new(0, 0, 34, 11);
        for mode in [CoverFill::Crop, CoverFill::Stretch, CoverFill::Fit] {
            let (image, render) = prepare_cover(&solid(256, 256), area, font, mode);
            assert!(
                render.width <= area.width && render.height <= area.height,
                "{mode:?} 溢出区域：{render:?} 不在 {area:?} 里"
            );
            assert!(
                image.width() > 0 && image.height() > 0,
                "{mode:?} 不该产生空图"
            );
        }
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

        let mut state = AppState::new(crate::config::Config::default());
        state.picker = Some(ratatui_image::picker::Picker::halfblocks());
        state.cover.set_image(
            "test-hash".to_string(),
            image::DynamicImage::ImageRgb8(pixels),
            1.0,
        );

        let area = Rect::new(0, 0, 60, 20);
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).expect("测试后端可用");
        let mut rest = Rect::default();
        let drawn = terminal
            .draw(|frame| {
                rest = draw_cover_block(frame, area, &mut state, CoverPlace::AboveContent);
            })
            .expect("绘制成功");

        assert_eq!(rest.y, 11, "下方内容从缩略图（10 行 + 1 行间距）之后开始");

        // 落在封面区里的非空格单元格：全是空格就说明图根本没进去
        let painted = drawn
            .buffer
            .content()
            .iter()
            .filter(|cell| cell.symbol() != " ")
            .count();
        assert!(painted > 0, "封面应当写进 Buffer，而不是 stdout");
    }

    /// 铺满模式画出来的格子必须**严格多于**「完整显示」——多出来的正是原先空着的那条。
    ///
    /// 用差分而不是绝对像素值：半块字符在上下同色时会退化成空格（用底色表示），
    /// 「某个格子是不是空格」并不可靠，但「哪种模式覆盖得更广」是可靠的：
    /// `Fit` 只能画进 22 列，`Crop` 铺满 34 列。
    #[test]
    fn fill_mode_paints_more_than_fit_mode() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut pixels = image::RgbImage::new(256, 256);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x * 7) as u8, (y * 7) as u8, 128]);
        }

        // 34 × 11 就是宽屏首页左栏那块封面的真实尺寸：像素比例约 1.5:1，而封面是方图
        let area = Rect::new(0, 0, 34, 11);

        let painted = |mode: CoverFill| {
            let mut state = AppState::new(crate::config::Config::default());
            state.picker = Some(ratatui_image::picker::Picker::halfblocks());
            state.config.cover_fill = mode;
            state.cover.set_image(
                "h".to_string(),
                image::DynamicImage::ImageRgb8(pixels.clone()),
                1.0,
            );

            let mut terminal = Terminal::new(TestBackend::new(34, 11)).expect("测试后端可用");
            let drawn = terminal
                .draw(|frame| {
                    draw_cover_block(frame, area, &mut state, CoverPlace::Fill);
                })
                .expect("绘制成功");
            drawn
                .buffer
                .content()
                .iter()
                .filter(|cell| cell.symbol() != " ")
                .count()
        };

        let fitted = painted(CoverFill::Fit);
        let cropped = painted(CoverFill::Crop);
        assert!(
            cropped > fitted,
            "铺满模式应当比完整显示覆盖得更广：crop={cropped} fit={fitted}"
        );
    }

    /// 区域变了才重新编码；区域不变时**一帧都不重编**。
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

        let mut pixels = image::RgbImage::new(256, 256);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            *pixel = image::Rgb([x as u8, y as u8, 128]);
        }
        let mut protocol = ratatui_image::picker::Picker::halfblocks()
            .new_resize_protocol(image::DynamicImage::ImageRgb8(pixels));

        // 交给 widget 的区域由纯函数算出，因此连续两帧必然是同一个矩形——
        // 区域稳定是「不重发」的前提
        let (first_area, _) = thumbnail_layout(Rect::new(0, 0, 60, 20), 1.0, 2.0).expect("够放");
        let (second_area, _) = thumbnail_layout(Rect::new(0, 0, 60, 20), 1.0, 2.0).expect("够放");
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
        let (bigger, _) = thumbnail_layout(Rect::new(0, 0, 60, 40), 1.0, 2.0).expect("够放");
        assert_ne!(bigger.height, first_area.height);
        let mut third = Buffer::empty(bigger);
        StatefulImage::default().render(bigger, &mut third, &mut protocol);
        assert!(
            protocol.last_encoding_result().is_some(),
            "区域变了应当重新编码一次"
        );
    }

    /// `CoverArt::fit_to` 的缓存语义：区域不变时协议被复用，**一帧都不重编**；
    /// 区域一变就必须重编。
    ///
    /// 重编意味着重新裁图 + 重新编码（几百 KB 的数据），每帧都做就是之前卡死的
    /// 那条路。这里必须真的渲染一次才能读到编码结果——`last_encoding_result()`
    /// 是 widget 在 `resize_encode_render` 里填的，光建协议不算编码。
    #[test]
    fn fit_to_reuses_the_protocol_while_the_area_is_stable() {
        use ratatui::buffer::Buffer;
        use ratatui::widgets::StatefulWidget;

        let picker = ratatui_image::picker::Picker::halfblocks();
        let mut cover = crate::app::state::CoverArt::default();
        cover.set_image("h".to_string(), solid(256, 256), 1.0);

        // 画一帧，返回这一帧是否发生了编码
        fn draw(
            cover: &mut crate::app::state::CoverArt,
            area: Rect,
            picker: &ratatui_image::picker::Picker,
        ) -> bool {
            let mut buffer = Buffer::empty(area);
            let (protocol, render) = cover.fit_to(CoverFill::Crop, area, picker).expect("有原图");
            assert_eq!(render, area, "裁剪模式的渲染区就是请求区");
            StatefulImage::default().resize(Resize::Scale(None)).render(
                render,
                &mut buffer,
                protocol,
            );
            protocol.last_encoding_result().is_some()
        }

        let area = Rect::new(0, 0, 34, 11);
        assert!(draw(&mut cover, area, &picker), "首帧必须编码一次");
        assert!(
            !draw(&mut cover, area, &picker),
            "区域没变就不该再编码——每帧编码正是之前卡死的原因"
        );

        // 换区域 → 必须重编，否则图还是旧尺寸、右下留空
        let wider = Rect::new(0, 0, 60, 11);
        assert!(
            draw(&mut cover, wider, &picker),
            "区域变了必须重新编码，否则图还是旧尺寸、填不满"
        );
    }

    /// 没有原图时 `fit_to` 返回 `None`——调用方据此留白，而不是画出半张图。
    #[test]
    fn fit_to_yields_nothing_without_a_source_image() {
        let picker = ratatui_image::picker::Picker::halfblocks();
        let mut cover = crate::app::state::CoverArt::default();
        assert!(!cover.is_drawable());
        assert!(
            cover
                .fit_to(CoverFill::Crop, Rect::new(0, 0, 34, 11), &picker)
                .is_none()
        );
    }
}
