//! 音源管理页。
//!
//! 展示全部音源，支持启用/禁用、设为默认、调整优先级，并显示各音源具备哪些能力。
//!
//! 列表数据直接来自 [`crate::source::SourceSet`]，不额外缓存一份——配置就是唯一
//! 真相，改完立即反映到界面上。

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use crate::app::state::AppState;
use crate::ui::theme::Theme;
use crate::ui::widgets::{panel, selection_list};

pub fn render_sources(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    focused: bool,
    theme: &Theme,
) {
    let block = panel("音源管理", focused, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width < 20 {
        return;
    }

    let kinds = state.config.sources.ordered();
    let active = state.config.active_source_kind();

    let items: Vec<_> = kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            let profile = state.config.sources.profile(*kind);
            let is_active = *kind == active;
            let marker = if is_active { "*" } else { " " };

            // 状态：启用的用实心点，禁用的用空心点
            let (status, status_style) = if profile.enabled {
                ("●", theme.now_playing())
            } else {
                ("○", theme.dim())
            };

            let mut spans = vec![
                Span::styled(format!("{marker}{:>2} ", index + 1), theme.dim()),
                Span::styled(format!("{status} "), status_style),
                Span::styled(
                    kind.label().to_string(),
                    if profile.enabled {
                        theme.body()
                    } else {
                        theme.dim()
                    },
                ),
            ];

            // 能力简写：看一眼就知道这个音源能干嘛
            let capability = kind.capability();
            let mut tags = Vec::new();
            if capability.lyric {
                tags.push("词");
            }
            if capability.cover {
                tags.push("封");
            }
            if capability.catalog {
                tags.push("目");
            }
            if capability.cloud {
                tags.push("云");
            }
            if !tags.is_empty() {
                spans.push(Span::styled(format!("  [{}]", tags.join("")), theme.dim()));
            }

            if is_active {
                spans.push(Span::styled("  当前", theme.now_playing()));
            }

            spans.push(Span::styled(format!("  {}", profile.api_base), theme.dim()));

            ratatui::widgets::ListItem::new(Line::from(spans))
        })
        .collect();

    let widget = selection_list(items, theme);
    frame.render_stateful_widget(widget, inner, &mut state.sources_cursor);

    // 底部一行说明，避免用户不知道怎么操作
    if inner.height > kinds.len() as u16 + 1 {
        let hint_area = Rect::new(inner.x, inner.y + kinds.len() as u16 + 1, inner.width, 1);
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                "Enter 启用/禁用 · E 设为默认 · K/J 调优先级 · 词=歌词 封=封面 目=目录 云=云端",
                theme.dim(),
            ))),
            hint_area,
        );
    }
}
