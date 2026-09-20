//! 音频可视化页面。
//!
//! 数据来自音频线程：它在把采样透传给播放器的同时记下原始波形，这里对最近
//! [`WINDOW_SIZE`](crate::audio::spectrum::WINDOW_SIZE) 个采样做 FFT，再按**对数**
//! 分成 [`BAND_COUNT`](crate::audio::spectrum::BAND_COUNT) 个频段（见
//! [`crate::audio::spectrum`]）。所以每根柱子是真的对应一段频率：贝斯亮左边，
//! 人声亮中间，镲片亮右边。
//!
//! 拿随机数画动画当然更省事，但那样做出来的是装饰品，和音乐没关系。
//!
//! 顺带一提，早先这里画的是**时域**音量历史（最近半秒的响度），铺成柱子看着
//! 像频谱其实不是——低频高频挤在一起，动起来是一整片此起彼伏的墙。
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
use ratatui::style::{Modifier, Style};
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
    let [bars_area, _gap, info_area] = Layout::vertical([
        Constraint::Min(2),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    // 左右各留 2 列内边距，柱子不会贴着边框，观感更稳
    let bars_area = shrink_horizontal(bars_area, 2);

    match state.playback {
        PlaybackState::Loading => render_buffering(frame, bars_area, state, theme),
        PlaybackState::Playing if !state.smooth_spectrum.is_empty() => {
            render_bars(
                frame,
                bars_area,
                &state.smooth_spectrum,
                &state.peak_spectrum,
                theme,
            );
        }
        _ => render_idle(frame, bars_area, state, theme),
    }

    render_track_info(frame, info_area, state, theme);
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

/// 每根柱子占几列：柱体 1 列 + 右侧 1 列空隙。
///
/// 空隙是观感的关键。之前把频段直接铺满每一列，柱子之间**没有分隔**，
/// 相邻柱子一旦都亮起来就粘成一整片——用户说的「糊在一起」就是这个。
/// 空出一列之后，多密的频谱都能看清是一根一根的。
const BAR_STRIDE: usize = 2;

/// 画柱状频谱：底部对齐，越高越亮，柱顶带一条缓慢下落的峰值刻度。
///
/// 每行只用一个 `Span`：整行共用一个颜色即可（按行做渐变），不必为每个字符建 Span。
/// 一个 80×20 的网格若逐字符建 Span 就是 1600 个，白白拖慢大尺寸终端下的帧。
fn render_bars(frame: &mut Frame, area: Rect, levels: &[f32], peaks: &[f32], theme: &Theme) {
    let height = area.height as usize;
    if height == 0 || levels.is_empty() {
        return;
    }

    // 柱子数由列数决定，但不超过频段数——柱子比频段还多只会是同一根重复画
    let bars = (area.width as usize / BAR_STRIDE).min(levels.len());
    if bars == 0 {
        return;
    }

    // 频段数通常多于柱子数，把一段频段压成一根柱子。
    // 取**最大值**而不是平均值：平均会把鼓点那一下的尖峰抹平，柱子就只剩一团钝钝的起伏。
    let aggregate = |values: &[f32]| -> Vec<f32> {
        (0..bars)
            .map(|bar| {
                let start = bar * values.len() / bars;
                let end = ((bar + 1) * values.len() / bars).max(start + 1);
                values[start..end.min(values.len())]
                    .iter()
                    .fold(0.0f32, |max, &value| max.max(value))
                    .clamp(0.0, 1.0)
            })
            .collect()
    };
    let columns = aggregate(levels);
    let caps = aggregate(peaks);

    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        // 从底部数起的行号，用来判断这一格要不要点亮
        let from_bottom = height - row;
        let style = bar_style(row, height, theme);

        let mut text = String::with_capacity(bars * BAR_STRIDE);
        for (bar, &level) in columns.iter().enumerate() {
            let filled = (level * height as f32).round() as usize;
            let cap = (caps[bar] * height as f32).round() as usize;
            text.push(if filled > 0 && from_bottom <= filled {
                '█'
            } else if cap > 0 && from_bottom == cap {
                '▔'
            } else {
                ' '
            });
            // 柱间空隙。最后一根后面不留，否则右边会多出一列空白
            if bar + 1 < bars {
                text.push(' ');
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    /// 把柱子画进一块 Buffer 里，方便逐格断言。
    fn bars_of(width: u16, height: u16, levels: &[f32]) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("测试后端可用");
        let theme = Theme::for_config(false);
        let drawn = terminal
            .draw(|frame| render_bars(frame, area, levels, levels, &theme))
            .expect("绘制成功");
        drawn.buffer.clone()
    }

    /// 用户抱怨的「糊在一起」：柱子之间没有空隙时，相邻柱子一旦都亮着就连成一片。
    /// 这里直接断言每一列是柱子还是空隙。
    #[test]
    fn bars_are_separated_by_a_gap() {
        // 8 个频段、16 列 → 8 根柱子：柱体在偶数列，奇数列是空隙
        let buffer = bars_of(16, 4, &[1.0; 8]);
        let bottom = 3;
        for x in 0..16 {
            let expected = if x % 2 == 0 { "█" } else { " " };
            assert_eq!(
                buffer[(x, bottom)].symbol(),
                expected,
                "第 {x} 列应当是{expected}"
            );
        }
    }

    /// 柱子高度按比例，且底部对齐：能量低的柱子只亮下面几行。
    #[test]
    fn bar_height_follows_the_level_and_sits_on_the_floor() {
        // 满格与半格交替：满的那根四行全亮，半的那根只亮两行
        let levels = [1.0, 0.5, 1.0, 0.5];
        let buffer = bars_of(8, 4, &levels);

        for row in 0..4 {
            // 柱子 0（第 0 列）满格、柱子 1（第 2 列）半格
            assert_eq!(
                buffer[(0, row)].symbol(),
                "█",
                "满格柱子在第 {row} 行也该亮"
            );
            let expected = if row >= 2 { "█" } else { " " };
            assert_eq!(buffer[(2, row)].symbol(), expected, "半格柱子第 {row} 行");
        }
    }

    /// 柱子数不超过频段数：比频段还多的柱子只能是同一根重复画，没有意义。
    #[test]
    fn bar_count_is_capped_by_band_count() {
        // 100 列够画 50 根，但只有 4 个频段 → 只画 4 根
        let buffer = bars_of(100, 2, &[1.0; 4]);
        let filled = (0..100).filter(|&x| buffer[(x, 1)].symbol() == "█").count();
        assert_eq!(filled, 4, "柱子数应当等于频段数");
    }
}
