//! 音频可视化页面。
//!
//! 数据来自 [`crate::audio::levels::AudioLevels`]——音频线程在把采样透传给播放器的
//! 同时记下的真实峰值。所以这里的柱子是跟着音乐走的：静音会掉到底，鼓点会顶到头。
//! 拿随机数画动画当然更省事，但那样做出来的是装饰品，和音量、和音乐都没关系。
//!
//! # 观感是怎么来的
//!
//! * **缓动**：原始电平每 ~15ms 跳一次，直接画会抖。这里用「快起慢落」（起振 20ms、
//!   回落 140ms）做指数平滑，柱子才既跟手又不抖。缓动按**真实时间**计算，所以帧率
//!   从 5fps 提到 30fps 时快慢观感不变（见 `AppState::advance_visualizer`）。
//! * **峰值刻度**：柱顶那条线落得比柱子慢，是频谱仪的标志性观感。
//! * 本页没有列表，整块主区都归它（见 `ui::render_main`）。

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::state::AppState;
use crate::audio::engine::PlaybackState;
use crate::ui::theme::Theme;
use crate::ui::widgets::panel;

/// 缓冲动画的一帧序列。用点字符做旋转，比整块文字闪烁克制。
const SPINNER: [char; 8] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧'];

/// 绘制可视化页面。
pub fn render_visualizer(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
    focused: bool,
    theme: &Theme,
) {
    let block = panel("可视化", focused, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // 太小时只留标题，避免画出半个格子导致的错位
    if inner.height < 4 || inner.width < 8 {
        return;
    }

    // 节奏：频谱占满剩余空间，底部依次是「曲目信息」和「说明」，中间留一行呼吸
    let [bars_area, _gap, info_area, hint_area] = Layout::vertical([
        Constraint::Min(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    // 左右各留 2 列内边距，柱子不会贴着边框，观感更稳
    let bars_area = shrink_horizontal(bars_area, 2);

    match state.playback {
        PlaybackState::Loading => render_buffering(frame, bars_area, state, theme),
        PlaybackState::Playing if !state.smooth_levels.is_empty() => {
            render_bars(
                frame,
                bars_area,
                &state.smooth_levels,
                &state.peak_levels,
                theme,
            );
        }
        _ => render_idle(frame, bars_area, state, theme),
    }

    render_track_info(frame, info_area, state, theme);
    render_hint(frame, hint_area, theme);
}

/// 两侧各收缩 `amount` 列（总量不够时不收缩）。
fn shrink_horizontal(area: Rect, amount: u16) -> Rect {
    let shrink = amount.saturating_mul(2);
    if area.width <= shrink {
        return area;
    }
    Rect {
        x: area.x + amount,
        width: area.width - shrink,
        ..area
    }
}

/// 画柱状频谱：底部对齐，越高越亮，柱顶带一条缓慢下落的峰值刻度。
///
/// 每行只用一个 `Span`：整行共用一个颜色即可（按行做渐变），不必为每个字符建 Span。
/// 一个 80×20 的网格若逐字符建 Span 就是 1600 个，白白拖慢大尺寸终端下的帧。
fn render_bars(frame: &mut Frame, area: Rect, levels: &[f32], peaks: &[f32], theme: &Theme) {
    let width = area.width as usize;
    let height = area.height as usize;
    if width == 0 || height == 0 || levels.is_empty() {
        return;
    }

    // 电平格子数与可用列数不一定相等，按列重采样，保证不会折行
    let columns: Vec<f32> = (0..width)
        .map(|column| {
            let index = (column * levels.len() / width).min(levels.len() - 1);
            levels[index].clamp(0.0, 1.0)
        })
        .collect();
    let caps: Vec<f32> = (0..width)
        .map(|column| {
            let index = (column * peaks.len().max(1) / width).min(peaks.len().saturating_sub(1));
            peaks.get(index).copied().unwrap_or(0.0).clamp(0.0, 1.0)
        })
        .collect();

    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        // 从底部数起的行号，用来判断这一格要不要点亮
        let from_bottom = height - row;
        let style = bar_style(row, height, theme);

        let mut text = String::with_capacity(width);
        for column in 0..width {
            let bar = (columns[column] * height as f32).round() as usize;
            let cap = (caps[column] * height as f32).round() as usize;
            text.push(if bar > 0 && from_bottom <= bar {
                '█'
            } else if cap > 0 && from_bottom == cap {
                '▔'
            } else {
                ' '
            });
        }
        lines.push(Line::from(Span::styled(text, style)));
    }

    frame.render_widget(ratatui::widgets::Paragraph::new(lines), area);
}

/// 柱子的颜色：底部暗、顶部亮，形成渐变。
///
/// 用主题的语义色而不是硬编码色值，16 色模式下也能正确降级。
fn bar_style(row: usize, height: usize, theme: &Theme) -> Style {
    let ratio = if height <= 1 {
        1.0
    } else {
        row as f32 / (height - 1) as f32
    };

    let base = if ratio > 0.8 {
        theme.accent
    } else if ratio > 0.5 {
        theme.accent_dim
    } else {
        theme.text_dim
    };

    Style::default().fg(base).add_modifier(Modifier::BOLD)
}

/// 未播放：给一句能直接照做的引导，而不是干瘪的「无数据」。
fn render_idle(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let text = if state.current.is_some() {
        "已暂停 —— 按 Space 继续"
    } else {
        "未在播放 —— 到「歌单」或「排行榜」里按 Enter 播一首"
    };
    frame.render_widget(
        ratatui::widgets::Paragraph::new(Line::from(Span::styled(text, theme.dim())))
            .alignment(Alignment::Center),
        area,
    );
}

/// 缓冲中：转点 + 百分比。下载进度已有节流，这里只做显示。
fn render_buffering(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    // 每 3 拍换一帧，约 600ms 转一圈（按默认 tick），不刺眼
    let glyph = SPINNER[(state.ticks / 3) as usize % SPINNER.len()];
    let mut spans = vec![Span::styled(format!("{glyph} 缓冲中"), theme.now_playing())];
    if let Some((received, total)) = state.download_progress {
        if let Some(total) = total
            && total > 0
        {
            spans.push(Span::styled(
                format!("  {}%", (received * 100 / total).min(100)),
                theme.dim(),
            ));
        }
    }
    frame.render_widget(
        ratatui::widgets::Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        area,
    );
}

/// 曲目信息：歌名居中，后面跟当前进度，两者用不同层级区分。
fn render_track_info(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let title = state
        .current
        .as_ref()
        .map(|song| format!("{} - {}", song.singer_text(), song.name))
        .unwrap_or_else(|| "—".to_string());
    let position = format!(
        "  {:02}:{:02}",
        state.position_ms / 60_000,
        (state.position_ms / 1000) % 60
    );

    frame.render_widget(
        ratatui::widgets::Paragraph::new(Line::from(vec![
            Span::styled(title, theme.title()),
            Span::styled(position, theme.dim()),
        ]))
        .alignment(Alignment::Center),
        area,
    );
}

fn render_hint(frame: &mut Frame, area: Rect, theme: &Theme) {
    frame.render_widget(
        ratatui::widgets::Paragraph::new(Span::styled(
            "数据来自实时音频采样，不是随机动画",
            theme.dim(),
        ))
        .alignment(Alignment::Center),
        area,
    );
}

/// 把颜色按 `ratio` 压暗。保留给需要连续渐变的场景。
#[allow(dead_code)]
fn dim(color: Color, ratio: f32) -> Color {
    let (r, g, b) = match color {
        Color::Rgb(r, g, b) => (r, g, b),
        other => return other,
    };
    let factor = (0.45 + 0.55 * ratio.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    Color::Rgb(
        (r as f32 * factor) as u8,
        (g as f32 * factor) as u8,
        (b as f32 * factor) as u8,
    )
}
