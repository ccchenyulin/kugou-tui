//! 配色。
//!
//! 每套主题（[`ThemeName`]）有两组色板：
//!
//! * 24 位真彩 —— 默认，颜色可以精调；
//! * 16 色 ANSI —— 给老终端或 `--basic-color` 用，只保留「主色 + 中性色」的区分度。
//!
//! 所有颜色都集中在这里，界面代码只引用语义名（`accent` / `error` / `selection_bg`），
//! 换配色不需要翻遍渲染代码。
//!
//! # 为什么 16 色版本只换主色
//!
//! ANSI 只有 16 个颜色，硬凑出六套完整色板只会得到六套都很脏的配色。真正决定
//! 「这套主题长什么样」的其实只有主色（`accent`）和选中底色，所以降级时只换
//! 这三处，其余沿用读得最清楚的中性组合。

use ratatui::style::{Color, Modifier, Style};

use serde::{Deserialize, Serialize};

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
    /// 24 位真彩 + 16 色降级共用的中性部分。
    ///
    /// 这两组颜色几乎不参与「主题辨识度」，只负责文字与层级的清晰度，
    /// 因此六套主题共用，不重复定义六遍。
    const fn with_accent(accent: Color, accent_dim: Color, selection_bg: Color) -> Self {
        Self {
            accent,
            accent_dim,
            text: Color::Rgb(222, 228, 238),
            text_dim: Color::Rgb(134, 145, 162),
            border: Color::Rgb(66, 76, 92),
            border_focus: accent,
            selection_bg,
            // 语义色保持固定：成功/警告/错误是用颜色**传达含义**的，
            // 跟着主题变会让人认不出来（比如主题是红的，那「错误」怎么画？）
            success: Color::Rgb(122, 220, 162),
            warning: Color::Rgb(240, 200, 110),
            error: Color::Rgb(246, 124, 124),
            progress: accent,
            progress_bg: Color::Rgb(48, 56, 70),
        }
    }

    /// 16 色降级：只换主色与选中底色，其余沿用最通用的中性组合。
    const fn ansi(accent: Color, accent_dim: Color, selection_bg: Color) -> Self {
        Self {
            accent,
            accent_dim,
            text: Color::White,
            text_dim: Color::Gray,
            border: Color::DarkGray,
            border_focus: accent,
            selection_bg,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            progress: accent,
            progress_bg: Color::DarkGray,
        }
    }

    /// 按主题名与「是否强制 16 色」取色板。
    pub fn for_config(theme: ThemeName, basic_color: bool) -> Self {
        if basic_color {
            theme.ansi_palette()
        } else {
            theme.truecolor_palette()
        }
    }
}

/// 可选主题。
///
/// 顺序即设置页里的显示顺序，往后加不会打乱已有配置（配置文件存的是
/// [`Self::id`] 这个字符串，不是序号）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    /// 冷蓝。默认，也是最初那一版配色。
    #[default]
    Default,
    /// 石墨：近乎无彩，只有灰度与一点冷白。适合长时间盯着，也最不挑终端。
    Graphite,
    /// 日落：橙红主色配暖紫底。
    Sunset,
    /// 森林：绿主色。
    Forest,
    /// 霓虹：高饱和品红 + 青，对比强烈。
    Neon,
    /// Dracula：紫主色，取自同名配色方案。
    Dracula,
}

impl ThemeName {
    /// 全部主题，按显示顺序。
    pub const ALL: [ThemeName; 6] = [
        Self::Default,
        Self::Graphite,
        Self::Sunset,
        Self::Forest,
        Self::Neon,
        Self::Dracula,
    ];

    /// 设置页里显示的名字。
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "冷蓝",
            Self::Graphite => "石墨",
            Self::Sunset => "日落",
            Self::Forest => "森林",
            Self::Neon => "霓虹",
            Self::Dracula => "暗紫",
        }
    }

    fn truecolor_palette(self) -> Theme {
        match self {
            Self::Default => Theme::with_accent(
                Color::Rgb(122, 200, 255),
                Color::Rgb(86, 138, 180),
                Color::Rgb(38, 66, 96),
            ),
            Self::Graphite => Theme::with_accent(
                Color::Rgb(226, 232, 240),
                Color::Rgb(148, 156, 168),
                Color::Rgb(70, 74, 82),
            ),
            Self::Sunset => Theme::with_accent(
                Color::Rgb(255, 146, 88),
                Color::Rgb(186, 106, 74),
                Color::Rgb(96, 56, 46),
            ),
            Self::Forest => Theme::with_accent(
                Color::Rgb(134, 226, 158),
                Color::Rgb(92, 152, 112),
                Color::Rgb(40, 74, 52),
            ),
            Self::Neon => Theme::with_accent(
                Color::Rgb(255, 92, 214),
                Color::Rgb(178, 74, 152),
                Color::Rgb(84, 34, 74),
            ),
            Self::Dracula => Theme::with_accent(
                Color::Rgb(189, 147, 249),
                Color::Rgb(138, 108, 184),
                Color::Rgb(68, 52, 92),
            ),
        }
    }

    fn ansi_palette(self) -> Theme {
        // 16 色里能当主色的就那几个亮色，够区分六套了
        match self {
            Self::Default => Theme::ansi(Color::Cyan, Color::DarkGray, Color::Blue),
            Self::Graphite => Theme::ansi(Color::White, Color::Gray, Color::DarkGray),
            Self::Sunset => Theme::ansi(Color::LightRed, Color::Red, Color::Red),
            Self::Forest => Theme::ansi(Color::LightGreen, Color::Green, Color::Green),
            Self::Neon => Theme::ansi(Color::LightMagenta, Color::Magenta, Color::Magenta),
            Self::Dracula => Theme::ansi(Color::LightBlue, Color::Blue, Color::Blue),
        }
    }
}

impl Theme {
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
