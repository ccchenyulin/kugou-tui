//! 配色。
//!
//! 两套色板：
//!
//! * [`Theme::truecolor`] —— 24 位真彩，默认；
//! * [`Theme::basic`] —— 只用 16 色 ANSI，给老终端或 `--basic-color` 用。
//!
//! 所有颜色都集中在这里，界面代码只引用语义名（`accent` / `error` / `selection_bg`），
//! 换配色不需要翻遍渲染代码。

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// 主强调色：焦点边框、当前播放曲目、进度条。
    pub accent: Color,
    /// 弱化的强调色：次级标题。
    pub accent_dim: Color,
    /// 正文。
    pub text: Color,
    /// 次要文字：歌手、专辑、提示。
    pub text_dim: Color,
    /// 非焦点边框。
    pub border: Color,
    /// 焦点边框。
    pub border_focus: Color,
    pub selection_bg: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    /// 进度条填充 / 底槽。
    pub progress: Color,
    pub progress_bg: Color,
}

impl Theme {
    /// 24 位真彩色板。
    pub const fn truecolor() -> Self {
        Self {
            accent: Color::Rgb(122, 200, 255),
            accent_dim: Color::Rgb(86, 138, 180),
            text: Color::Rgb(222, 228, 238),
            text_dim: Color::Rgb(134, 145, 162),
            border: Color::Rgb(66, 76, 92),
            border_focus: Color::Rgb(122, 200, 255),
            selection_bg: Color::Rgb(38, 66, 96),
            success: Color::Rgb(122, 220, 162),
            warning: Color::Rgb(240, 200, 110),
            error: Color::Rgb(246, 124, 124),
            progress: Color::Rgb(122, 200, 255),
            progress_bg: Color::Rgb(48, 56, 70),
        }
    }

    /// 16 色 ANSI 色板。
    pub const fn basic() -> Self {
        Self {
            accent: Color::Cyan,
            accent_dim: Color::DarkGray,
            text: Color::White,
            text_dim: Color::Gray,
            border: Color::DarkGray,
            border_focus: Color::Cyan,
            selection_bg: Color::Blue,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            progress: Color::Cyan,
            progress_bg: Color::DarkGray,
        }
    }

    pub fn for_config(basic_color: bool) -> Self {
        if basic_color {
            Self::basic()
        } else {
            Self::truecolor()
        }
    }

    // ---- 常用样式 ----

    /// 焦点边框：亮色。
    pub fn focused_border(&self) -> Style {
        Style::default().fg(self.border_focus)
    }

    /// 非焦点边框：暗色。
    pub fn idle_border(&self) -> Style {
        Style::default().fg(self.border)
    }

    /// 面板标题。
    pub fn title(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// 正文。
    pub fn body(&self) -> Style {
        Style::default().fg(self.text)
    }

    /// 次要文字。
    pub fn dim(&self) -> Style {
        Style::default().fg(self.text_dim)
    }

    /// 二维码专用配色：强制「深色模块 + 浅色底」。
    ///
    /// 二维码识别依赖明暗对比，也依赖**极性**（深模块、浅底）。若跟着终端主题走，
    /// 深色主题下就会变成「浅模块、深底」，等于把码反了色——多数扫码器能容错，
    /// 但部分会直接失败。所以这里固定极性，不随主题变化。
    pub fn qr(&self) -> Style {
        Style::default().fg(Color::Black).bg(Color::White)
    }

    /// 选中行。
    ///
    /// 背景 + 前景 + 加粗三重叠加：只靠背景色在深色终端上几乎看不出来，
    /// 用户会以为光标没动。前景复用正文色（近白），与选中背景对比充分。
    pub fn selection(&self) -> Style {
        Style::default()
            .bg(self.selection_bg)
            .fg(self.text)
            .add_modifier(Modifier::BOLD)
    }

    /// 正在播放的曲目。
    pub fn now_playing(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// 当前歌词行。
    pub fn lyric_active(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// 其它歌词行。
    pub fn lyric_idle(&self) -> Style {
        Style::default().fg(self.text_dim)
    }

    /// 状态栏消息按级别取色。
    pub fn status(&self, level: crate::app::state::StatusLevel) -> Style {
        use crate::app::state::StatusLevel;
        let color = match level {
            StatusLevel::Info => self.text,
            StatusLevel::Success => self.success,
            StatusLevel::Warning => self.warning,
            StatusLevel::Error => self.error,
        };
        Style::default().fg(color)
    }

    /// 播放状态指示。
    pub fn playback(&self, state: crate::audio::engine::PlaybackState) -> Style {
        use crate::audio::engine::PlaybackState;
        let color = match state {
            PlaybackState::Playing => self.success,
            PlaybackState::Paused => self.warning,
            PlaybackState::Loading => self.accent,
            PlaybackState::Stopped => self.text_dim,
        };
        Style::default().fg(color)
    }

    /// 快捷键标签。
    pub fn key_hint(&self) -> Style {
        Style::default()
            .fg(self.accent_dim)
            .add_modifier(Modifier::BOLD)
    }
}
