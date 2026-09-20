//! 配置持久化。
//!
//! 落盘位置：`$XDG_CONFIG_HOME/kugou-tui/config.toml`（Linux 下即
//! `~/.config/kugou-tui/config.toml`）。缓存与日志放在 `$XDG_CACHE_HOME/kugou-tui/`，
//! 这样备份配置时不会把几百 MB 的音频缓存一起带走。
//!
//! 优先级：命令行参数 > 环境变量 > 配置文件 > 内置默认值。
//! 环境变量由 clap 的 `env` 属性直接读入 [`Cli`]，因此这里只需实现后两级。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::queue::PlaybackMode;
use crate::cli::Cli;
use crate::error::{AppError, Result};
use crate::logger::tlog;
use crate::source::{SourceKind, SourceSet};
use crate::ui::theme::ThemeName;

/// KuGouMusicApi 的默认监听地址。
pub const DEFAULT_API_BASE: &str = "http://127.0.0.1:3000";

const DEFAULT_VOLUME: f32 = 0.7;
const DEFAULT_PAGE_SIZE: u32 = 30;
const DEFAULT_TICK_MS: u64 = 200;
const DEFAULT_CACHE_LIMIT_MIB: u64 = 512;
const DEFAULT_QUALITY: &str = "128";
const APP_DIR_NAME: &str = "kugou-tui";

/// `/song/url` 支持的音质取值。
///
/// 前五个是常规音质；`viper_*` 是酷狗的「蝰蛇音效」系列，仅部分歌曲支持，
/// 拿不到时服务端会返回空 url，客户端的错误提示会说明原因。
pub const SUPPORTED_QUALITIES: &[&str] = &[
    "128",
    "320",
    "flac",
    "high",
    "super",
    "viper_clear",
    "viper_atmos",
    "viper_tape",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// KuGouMusicApi 服务地址，例如 `http://127.0.0.1:3000`。
    pub api_base: String,

    /// 登录态 cookie，形如 `token=xxx; userid=xxx; dfid=xxx`。
    ///
    /// 搜索接口缺少认证信息会返回 `error_code: 152`，云歌单同步也依赖它。
    pub cookie: Option<String>,

    /// 设备指纹。为空时由 `/register/dev` 自动获取并回写。
    pub dfid: Option<String>,

    /// 音量，范围 `0.0 ~ 1.0`。
    pub volume: f32,

    /// 播放模式。
    pub playback_mode: PlaybackMode,

    /// 音频缓存目录。
    pub cache_dir: PathBuf,

    /// 缓存上限（MiB），`0` 表示不限制。
    pub cache_limit_mib: u64,

    /// 歌词时间偏移（毫秒）。正值表示歌词提前显示。
    pub lyric_offset_ms: i64,

    /// 界面刷新间隔（毫秒）。这是控制 CPU 占用的主要旋钮。
    pub tick_ms: u64,

    /// 列表每页条目数。
    pub page_size: u32,

    /// 访问 KuGouMusicApi 时使用的 HTTP 代理。
    pub proxy: Option<String>,

    /// 音质。可选 `128` / `320` / `flac` / `high`。
    ///
    /// 128 是普通音质，下载体积最小、缓存最省空间，作为默认值。
    pub quality: String,

    /// 是否强制使用 16 色板（适配老终端）。
    pub basic_color: bool,

    /// 单曲下载目录。**不设置时**按 `~/Music` 处理（首次启动也要能正常下载）。
    /// 设置页里限定在几个常见位置之间选，避免路径写错把下载弄失败。
    pub download_dir: Option<String>,

    /// 界面主题。存的是 [`crate::ui::theme::ThemeName::id`]，解析不出来时回落
    /// 到默认主题——改坏配置文件不该让程序起不来。
    pub theme: ThemeName,

    /// 自定义键位：`动作名 -> 按键`，例如 `quit = "Q"`、`open_sources = "s"`。
    ///
    /// 动作名是 [`crate::keymap::Action`] 变体的 snake_case（见 keymap.rs 的
    /// `action_from_name`）。未列出的动作沿用默认键位，因此老配置文件不需要改。
    #[serde(default)]
    pub keymap: std::collections::BTreeMap<String, String>,

    /// 各音源的连接与身份配置，以及当前选中的音源。
    ///
    /// 切换音源时，`api_base` / `cookie` / `dfid` 会从选中的音源同步过来。
    /// 这三个字段仍是运行时实际读取的值，这样改动面最小，也不会漏掉某处引用。
    pub sources: SourceSet,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_base: DEFAULT_API_BASE.to_string(),
            cookie: None,
            dfid: None,
            volume: DEFAULT_VOLUME,
            playback_mode: PlaybackMode::Sequential,
            cache_dir: default_cache_dir(),
            cache_limit_mib: DEFAULT_CACHE_LIMIT_MIB,
            lyric_offset_ms: 0,
            tick_ms: DEFAULT_TICK_MS,
            page_size: DEFAULT_PAGE_SIZE,
            proxy: None,
            quality: DEFAULT_QUALITY.to_string(),
            basic_color: false,
            theme: ThemeName::default(),
            download_dir: None,
            keymap: std::collections::BTreeMap::new(),
            sources: SourceSet::default(),
        }
    }
}

impl Config {
    /// 当前选中的音源种类。
    pub fn active_source_kind(&self) -> SourceKind {
        self.sources.active
    }

    /// 切换到指定音源：把它的连接与身份信息同步到运行时字段。
    ///
    /// 只改配置，**不碰播放队列与当前曲目**，因此切换过程中播放不会中断。
    pub fn switch_source(&mut self, kind: SourceKind) {
        let profile = self.sources.profile(kind).clone();
        self.sources.active = kind;
        self.api_base = profile.api_base;
        self.cookie = profile.cookie;
        self.dfid = profile.device_id;
    }

    /// 把当前运行时字段回填进选中音源的档案。
    ///
    /// 登录成功或自动取到 dfid 后调用，保证切走再切回来时身份还在。
    pub fn sync_active_source(&mut self) {
        let kind = self.sources.active;
        let profile = self.sources.profile_mut(kind);
        // 刻意**不**回写 api_base：它是音源自己的身份，不该被运行时的地址覆盖。
        // 否则一旦用 `--api-base` 临时指向别处，就会把该音源的地址改坏——
        // 表现是「明明选了概念版，却一直在打标准版」，而 dfid 又只对概念版有效，
        // 于是取链报 20028。地址只由 switch_source() 从档案里取。
        profile.cookie = self.cookie.clone();
        profile.device_id = self.dfid.clone();
    }
}

impl Config {
    /// 配置文件路径。
    pub fn path() -> PathBuf {
        config_root().join("config.toml")
    }

    /// 日志文件路径。
    pub fn log_path() -> PathBuf {
        default_cache_dir().join("kugou-tui.log")
    }

    /// 读取配置。任何异常都退化为默认配置，保证程序总能启动。
    pub fn load() -> Self {
        let path = Self::path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Self::default();
            }
            Err(error) => {
                tlog!(
                    crate::logger::LEVEL_WARN,
                    "读取配置文件 {} 失败：{error}，改用默认配置",
                    path.display()
                );
                return Self::default();
            }
        };

        match toml::from_str::<Self>(&text) {
            Ok(config) => config.normalized(),
            Err(error) => {
                tlog!(
                    crate::logger::LEVEL_WARN,
                    "解析配置文件 {} 失败：{error}，改用默认配置",
                    path.display()
                );
                Self::default()
            }
        }
    }

    /// 写回配置文件，使用 pretty 格式方便用户手工编辑。
    ///
    /// 文件里存着登录 token，所以目录收成 `0700`、文件收成 `0600`——默认的
    /// `0644` 会让同机器上的其他用户直接读到你的账号凭据。
    ///
    /// # 为什么这里要自己 sync 一次
    ///
    /// 顶层 `cookie` / `dfid` 是当前会话的真相（`App` 里所有读写都走它们），
    /// 而启动时 `switch_source()` 会**反过来**用音源档案覆盖这两个顶层字段。
    /// 如果写盘前不先回填进档案，登录后重启就会退回未登录、dfid 也要重新探测。
    ///
    /// 曾经只在「按 v 切音源」这一个地方做同步，结果漏掉了登录与自动取 dfid
    /// 两条路径。同步收进 `save()` 之后，任何一次落盘都不可能再漏。
    pub fn save(&mut self) -> Result<()> {
        self.sync_active_source();

        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| AppError::io_at(parent.display().to_string(), error))?;
            restrict_permissions(parent, 0o700)?;
        }

        let text = toml::to_string_pretty(self)
            .map_err(|error| AppError::Config(format!("序列化配置失败：{error}")))?;
        std::fs::write(&path, text)
            .map_err(|error| AppError::io_at(path.display().to_string(), error))?;
        restrict_permissions(&path, 0o600)
    }

    /// 用命令行参数覆盖配置。仅当用户显式传参时才覆盖。
    pub fn merge_cli(&mut self, cli: &Cli) {
        if let Some(api_base) = cli.api_base.as_deref() {
            let trimmed = api_base.trim();
            if !trimmed.is_empty() {
                self.api_base = trimmed.to_string();
            }
        }
        if let Some(cookie) = cli.cookie.as_deref() {
            let trimmed = cookie.trim();
            if !trimmed.is_empty() {
                self.cookie = Some(trimmed.to_string());
            }
        }
        if let Some(volume) = cli.volume {
            self.volume = f32::from(volume) / 100.0;
        }
        if let Some(cache_dir) = cli.cache_dir.as_ref() {
            self.cache_dir = cache_dir.clone();
        }
        if let Some(limit) = cli.cache_limit {
            self.cache_limit_mib = limit;
        }
        if let Some(tick_ms) = cli.tick_ms {
            self.tick_ms = tick_ms;
        }
        if let Some(page_size) = cli.page_size {
            self.page_size = page_size;
        }
        if let Some(proxy) = cli.proxy.as_deref() {
            let trimmed = proxy.trim();
            if !trimmed.is_empty() {
                self.proxy = Some(trimmed.to_string());
            }
        }
        if cli.basic_color {
            self.basic_color = true;
        }
        self.normalize();
    }

    /// 把配置约束到合法区间，避免手改配置文件写出越界值。
    pub fn normalized(mut self) -> Self {
        self.normalize();
        self
    }

    fn normalize(&mut self) {
        // 从未初始化过的音源：地址为空时补默认值并置为启用。
        //
        // 老配置文件里没有这些音源的段，serde 会走 `SourceProfile::default()`，
        // 那里给的是空地址——照着空地址发请求必然失败，用户只会看到「连不上」
        // 却不知道要填什么。这里按音源种类补上默认端口，并视为启用：
        // 显式填过地址的音源则尊重用户设置，不动它的 enabled。
        for kind in SourceKind::ALL {
            let profile = self.sources.profile_mut(kind);
            if profile.api_base.trim().is_empty() {
                profile.api_base = kind.default_api_base().to_string();
                profile.enabled = true;
            }
        }

        self.api_base = self.api_base.trim().trim_end_matches('/').to_string();
        if self.api_base.is_empty() {
            self.api_base = DEFAULT_API_BASE.to_string();
        }
        if !self.api_base.starts_with("http://") && !self.api_base.starts_with("https://") {
            self.api_base = format!("http://{}", self.api_base);
        }

        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0.0, 1.0)
        } else {
            DEFAULT_VOLUME
        };

        self.tick_ms = self.tick_ms.clamp(50, 5_000);
        self.page_size = self.page_size.clamp(5, 200);
        self.lyric_offset_ms = self.lyric_offset_ms.clamp(-10_000, 10_000);

        let quality = self.quality.trim().to_ascii_lowercase();
        self.quality = if SUPPORTED_QUALITIES.contains(&quality.as_str()) {
            quality
        } else {
            DEFAULT_QUALITY.to_string()
        };

        if self.cache_dir.as_os_str().is_empty() {
            self.cache_dir = default_cache_dir();
        } else {
            // 允许在配置文件里写 `~/music-cache`
            self.cache_dir = expand_tilde(&self.cache_dir);
        }

        if let Some(cookie) = self.cookie.as_ref() {
            let trimmed = cookie.trim();
            if trimmed.is_empty() {
                self.cookie = None;
            } else if trimmed.len() != cookie.len() {
                self.cookie = Some(trimmed.to_string());
            }
        }
    }

    /// 组装最终发给 KuGouMusicApi 的 cookie 串。
    ///
    /// 若用户配置里已包含 `dfid=`，则以用户配置为准；否则用自动探测到的 dfid 补齐。
    /// `/song/url` 接口缺少 dfid 会返回「本次请求需要验证」。
    pub fn cookie_header(&self) -> Option<String> {
        let base = self.cookie.as_deref().unwrap_or_default().trim();
        let has_dfid = base.split(';').any(|pair| pair.trim().starts_with("dfid="));

        match (base.is_empty(), has_dfid, self.dfid.as_deref()) {
            (true, _, Some(dfid)) => Some(format!("dfid={dfid}")),
            (true, _, None) => None,
            (false, false, Some(dfid)) => Some(format!("{base}; dfid={dfid}")),
            (false, _, _) => Some(base.to_string()),
        }
    }

    /// 是否已配置登录态。
    pub fn is_logged_in(&self) -> bool {
        self.cookie
            .as_deref()
            .map(|cookie| cookie.contains("token=") && cookie.contains("userid="))
            .unwrap_or(false)
    }

    /// 确保缓存目录存在。
    pub fn ensure_cache_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.cache_dir)
            .map_err(|error| AppError::io_at(self.cache_dir.display().to_string(), error))
    }
}

/// 缓存根目录。
pub fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR_NAME)
}

fn config_root() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR_NAME)
}

/// 把 `~` 展开成用户主目录，供配置文件里手写路径时使用。
pub fn expand_tilde(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    let Some(rest) = text.strip_prefix('~') else {
        return path.to_path_buf();
    };
    let Some(home) = dirs::home_dir() else {
        return path.to_path_buf();
    };
    home.join(rest.trim_start_matches('/'))
}

/// 收紧文件或目录权限。非 Unix 平台上是空操作。
///
/// 刻意用 `set_permissions` 而不是依赖 umask：umask 是进程级、由启动环境决定的，
/// 不能让「凭据文件谁能读」取决于用户从哪个 shell 启动程序。
fn restrict_permissions(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|error| AppError::io_at(path.display().to_string(), error))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Config;
    use crate::source::SourceKind;

    #[test]
    fn normalizes_api_base_and_volume() {
        let mut config = Config {
            api_base: "127.0.0.1:3000/".to_string(),
            volume: 9.0,
            ..Config::default()
        };
        config = config.normalized();
        assert_eq!(config.api_base, "http://127.0.0.1:3000");
        assert!((config.volume - 1.0).abs() < f32::EPSILON);
    }

    /// 端到端锁住「登录态与 dfid 必须活过一次重启」。
    ///
    /// 启动时 `main.rs` 会无条件 `switch_source(active)`，用**音源档案**覆盖顶层字段。
    /// 所以只写顶层而不回填进档案，重启后登录态就没了（同理 dfid 要重新探测）。
    ///
    /// 这里跑真实落盘 + 真实 `load()`。为了不碰用户配置，先把 `XDG_CONFIG_HOME`
    /// 指到临时目录，结束后恢复原值——`catch_unwind` 保证失败时也会恢复。
    #[test]
    fn login_survives_restart() {
        let temp = std::env::temp_dir().join(format!("kugou-tui-cfgtest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).expect("建临时配置目录");

        // SAFETY: 单测进程内临时改写并恢复；其它测试不读配置路径，无交叉影响。
        let previous = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &temp) };

        let outcome = std::panic::catch_unwind(|| {
            let mut config = Config::default();
            config.sources.active = SourceKind::KugouConcept;
            config.switch_source(SourceKind::KugouConcept);
            // 模拟「应用内扫码登录成功 + 自动取到 dfid」：只写顶层，不手动 sync
            config.cookie = Some("token=abc; userid=42".to_string());
            config.dfid = Some("df-xyz".to_string());
            config.save().expect("保存配置");

            // 模拟重启：main.rs 在 merge_cli 之前会无条件 switch_source(active)
            let mut restarted = Config::load();
            let active = restarted.active_source_kind();
            restarted.switch_source(active);

            assert_eq!(
                restarted.cookie.as_deref(),
                Some("token=abc; userid=42"),
                "重启后登录态丢失"
            );
            assert_eq!(
                restarted.dfid.as_deref(),
                Some("df-xyz"),
                "重启后 dfid 丢失"
            );
            assert!(restarted.is_logged_in());
        });

        match previous {
            Some(value) => unsafe { std::env::set_var("XDG_CONFIG_HOME", value) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
        let _ = std::fs::remove_dir_all(&temp);

        assert!(outcome.is_ok(), "登录态/设备指纹在重启后丢失");
    }

    /// 锁住 H1 的修复：顶层 cookie/dfid 必须能被回填进选中音源的档案。
    ///
    /// 启动时 `switch_source()` 会**反过来**用档案覆盖顶层字段，所以只写顶层而不
    /// 同步进档案的话，登录态和 dfid 下次启动就没了。
    #[test]
    fn sync_active_source_writes_identity_into_profile() {
        let mut config = Config {
            cookie: Some("token=t; userid=1".to_string()),
            dfid: Some("df-1".to_string()),
            ..Config::default()
        };
        config.sources.active = SourceKind::KugouConcept;

        config.sync_active_source();

        let profile = config.sources.profile(SourceKind::KugouConcept);
        assert_eq!(profile.cookie.as_deref(), Some("token=t; userid=1"));
        assert_eq!(profile.device_id.as_deref(), Some("df-1"));
        // 另一个音源不该被牵连（两个平台的登录态不通用）
        assert!(config.sources.profile(SourceKind::Kugou).cookie.is_none());
    }

    /// 同步刻意**不**回写 api_base：地址属于音源自己，不该被运行时的临时值污染。
    #[test]
    fn sync_active_source_keeps_profile_address() {
        let mut config = Config {
            api_base: "http://127.0.0.1:9999".to_string(),
            ..Config::default()
        };
        config.sources.active = SourceKind::Kugou;
        config.sync_active_source();
        assert_eq!(
            config.sources.profile(SourceKind::Kugou).api_base,
            "http://127.0.0.1:3000"
        );
    }

    #[test]
    fn appends_dfid_only_when_absent() {
        let mut config = Config {
            cookie: Some("token=t; userid=1".to_string()),
            dfid: Some("abc".to_string()),
            ..Config::default()
        };
        assert_eq!(
            config.cookie_header().as_deref(),
            Some("token=t; userid=1; dfid=abc")
        );

        config.cookie = Some("token=t; userid=1; dfid=keep".to_string());
        assert_eq!(
            config.cookie_header().as_deref(),
            Some("token=t; userid=1; dfid=keep")
        );
    }
}
