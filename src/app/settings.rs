//! 设置页的条目定义。
//!
//! # 为什么把「候选值」集中在这里
//!
//! 每个条目要么是枚举（主题、音质、播放模式），要么是离散档位（刷新间隔、
//! 每页条数、缓存上限）。散在各处写死的话，设置页显示的顺序和实际能取到的
//! 值很容易对不上——用户按右键切出来的档位和他看到的不一致，是最难查的那种 bug。
//!
//! 这里只放**纯数据**：界面怎么画、值怎么落到 [`crate::config::Config`] 上，
//! 分别归 `ui::views::settings` 与 `app::update`。

use crate::app::queue::PlaybackMode;
use crate::app::state::AppState;
use crate::ui::views::settings::toggle_text;

/// 设置项。顺序即设置页里的显示顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// 界面主题。
    Theme,
    /// 播放音质。
    Quality,
    /// 播放模式（顺序 / 列表循环 / 单曲循环 / 随机）。
    PlaybackMode,
    /// 界面刷新间隔，越大越省电。
    RefreshMs,
    /// 歌词时间偏移。
    LyricOffsetMs,
    /// 列表每页条数。
    PageSize,
    /// 音频缓存上限。
    CacheLimitMib,
    /// 强制 16 色（老终端）。
    BasicColor,
    /// 侧栏歌词面板。
    LyricPanel,
    /// 侧边栏（标签导航）。
    Sidebar,
}

impl Setting {
    pub const ALL: [Setting; 10] = [
        Self::Theme,
        Self::Quality,
        Self::PlaybackMode,
        Self::RefreshMs,
        Self::LyricOffsetMs,
        Self::PageSize,
        Self::CacheLimitMib,
        Self::BasicColor,
        Self::LyricPanel,
        Self::Sidebar,
    ];

    /// 左侧的名字。
    pub fn label(self) -> &'static str {
        match self {
            Self::Theme => "主题",
            Self::Quality => "音质",
            Self::PlaybackMode => "播放模式",
            Self::RefreshMs => "刷新间隔",
            Self::LyricOffsetMs => "歌词偏移",
            Self::PageSize => "每页条数",
            Self::CacheLimitMib => "缓存上限",
            Self::BasicColor => "16 色模式",
            Self::LyricPanel => "歌词面板",
            Self::Sidebar => "侧边导航",
        }
    }

    /// 右侧的说明：讲清楚改了会怎样，而不是重复一遍当前值。
    pub fn description(self) -> &'static str {
        match self {
            Self::Theme => "配色方案，即时生效",
            Self::Quality => "越高越占缓存，切歌后生效",
            Self::PlaybackMode => "顺序 / 列表循环 / 单曲循环 / 随机",
            Self::RefreshMs => "越小越流畅，也越费 CPU",
            Self::LyricOffsetMs => "正值让歌词提前显示",
            Self::PageSize => "搜索与歌单一次取多少条",
            Self::CacheLimitMib => "超了自动删最旧的，0 为不限",
            Self::BasicColor => "老终端画不出真彩时打开",
            Self::LyricPanel => "右侧歌词栏（快捷键 y）",
            Self::Sidebar => "左侧标签导航（快捷键 b）",
        }
    }
}

/// 刷新间隔候选（毫秒）：从省电到流畅。
pub const REFRESH_MS_OPTIONS: [u64; 4] = [200, 100, 50, 33];

/// 每页条数候选。
pub const PAGE_SIZE_OPTIONS: [u32; 4] = [20, 30, 50, 100];

/// 缓存上限候选（MiB），`0` 表示不限制。
pub const CACHE_LIMIT_OPTIONS: [u64; 5] = [0, 256, 512, 1024, 2048];

/// 歌词偏移的步进与上下限（毫秒）。
pub const LYRIC_OFFSET_STEP: i64 = 100;
pub const LYRIC_OFFSET_LIMIT: i64 = 5_000;

/// 播放模式候选，顺序与 `r` 键轮转一致。
pub const PLAYBACK_MODES: [PlaybackMode; 4] = [
    PlaybackMode::Sequential,
    PlaybackMode::RepeatAll,
    PlaybackMode::RepeatOne,
    PlaybackMode::Shuffle,
];

/// 把音质档位翻成人话。
///
/// 服务端返回的就是 `128` / `flac` 这些原始值，直接显示用户不知道自己选了什么。
pub fn quality_label(quality: &str) -> String {
    match quality {
        "128" => "标准 128kbps".to_string(),
        "320" => "较高 320kbps".to_string(),
        "flac" => "无损 FLAC".to_string(),
        "high" => "高品".to_string(),
        "super" => "超高".to_string(),
        "viper_clear" => "蝰蛇母带".to_string(),
        other => other.to_string(),
    }
}

/// 某项当前值的显示文本。
pub fn value_text(setting: Setting, state: &AppState) -> String {
    match setting {
        Setting::Theme => state.config.theme.label().to_string(),
        Setting::Quality => quality_label(&state.config.quality),
        Setting::PlaybackMode => state.config.playback_mode.label().to_string(),
        Setting::RefreshMs => format!(
            "{}ms（{} fps）",
            state.config.tick_ms,
            1000 / state.config.tick_ms.max(1)
        ),
        Setting::LyricOffsetMs => format!("{:+}ms", state.config.lyric_offset_ms),
        Setting::PageSize => format!("{} 条", state.config.page_size),
        Setting::CacheLimitMib => {
            if state.config.cache_limit_mib == 0 {
                "不限".to_string()
            } else {
                format!("{} MiB", state.config.cache_limit_mib)
            }
        }
        Setting::BasicColor => toggle_text(state.config.basic_color),
        Setting::LyricPanel => toggle_text(state.show_lyric_panel),
        Setting::Sidebar => toggle_text(state.sidebar_visible),
    }
}

/// 设置页要显示的全部值，顺序与 [`Setting::ALL`] 一致。
pub fn values(state: &AppState) -> Vec<String> {
    Setting::ALL
        .iter()
        .map(|setting| value_text(*setting, state))
        .collect()
}

/// 在候选列表里按 `delta` 前进，越界回绕。
///
/// 返回 `None` 表示当前值不在候选里（比如手改过配置文件），此时调用方保持原值。
pub fn cycle<T: PartialEq + Copy>(options: &[T], current: T, delta: isize) -> Option<T> {
    if options.is_empty() {
        return None;
    }
    let index = options.iter().position(|option| *option == current)?;
    let len = options.len() as isize;
    let next = (index as isize + delta).rem_euclid(len) as usize;
    options.get(next).copied()
}

/// 同上，但用于字符串候选（`cycle` 要求 `Copy`，`String` 不满足）。
pub fn cycle_str<'a>(options: &'a [&'a str], current: &str, delta: isize) -> Option<&'a str> {
    if options.is_empty() {
        return None;
    }
    let index = options.iter().position(|option| *option == current)?;
    let len = options.len() as isize;
    let next = (index as isize + delta).rem_euclid(len) as usize;
    options.get(next).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_has_a_label_and_description() {
        for setting in Setting::ALL {
            assert!(!setting.label().is_empty());
            assert!(!setting.description().is_empty());
        }
    }

    #[test]
    fn cycle_wraps_around_in_both_directions() {
        let options = [1u8, 2, 3];
        assert_eq!(cycle(&options, 1, 1), Some(2));
        assert_eq!(cycle(&options, 3, 1), Some(1), "走到末尾要绕回开头");
        assert_eq!(cycle(&options, 1, -1), Some(3), "反向也要绕");
        assert_eq!(cycle(&options, 2, 0), Some(2));
    }

    #[test]
    fn cycle_returns_none_for_unknown_current_value() {
        // 配置文件被手改过、值不在候选里时，保持原样而不是跳到第一项
        assert_eq!(cycle(&[1u8, 2, 3], 9, 1), None);
        assert_eq!(cycle::<u8>(&[], 1, 1), None);
    }

    #[test]
    fn playback_modes_follow_the_r_key_order() {
        // 设置页里的顺序要和按 r 轮转的顺序一致，否则用户会以为切错了
        for (index, mode) in PLAYBACK_MODES.iter().enumerate() {
            let next = PLAYBACK_MODES[(index + 1) % PLAYBACK_MODES.len()];
            assert_eq!(mode.next(), next, "{mode:?} 的下一档对不上");
        }
    }
}
