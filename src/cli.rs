//! 命令行参数。
//!
//! 命令行 > 环境变量 > 配置文件 > 内置默认值，优先级由 [`crate::config::Config::merge_cli`]
//! 统一实现，避免优先级逻辑散落在各处。

use std::path::PathBuf;

use clap::Parser;

/// 酷狗音乐命令行 TUI 播放器。
///
/// 需要先在本机运行 KuGouMusicApi 服务（默认 http://127.0.0.1:3000）。
#[derive(Debug, Clone, Parser)]
#[command(
    name = "kugou-tui",
    version,
    about = "轻量级酷狗音乐 TUI 播放器",
    long_about = None,
)]
pub struct Cli {
    /// KuGouMusicApi 服务地址。
    #[arg(short = 'a', long, env = "KUGOU_API_BASE", value_name = "URL")]
    pub api_base: Option<String>,

    /// 登录 cookie，形如 `token=xxx; userid=xxx; dfid=xxx`。
    #[arg(short = 'c', long, env = "KUGOU_COOKIE", value_name = "COOKIE")]
    pub cookie: Option<String>,

    /// 启动后立刻搜索该关键词。
    #[arg(short = 's', long, value_name = "KEYWORDS")]
    pub search: Option<String>,

    /// 初始音量，取值 0-100。
    #[arg(long, value_name = "0-100", value_parser = clap::value_parser!(u8).range(0..=100))]
    pub volume: Option<u8>,

    /// 音频缓存目录。
    #[arg(long, value_name = "DIR")]
    pub cache_dir: Option<PathBuf>,

    /// 音频缓存上限（MiB），0 表示不限制。
    #[arg(long, value_name = "MiB")]
    pub cache_limit: Option<u64>,

    /// 界面刷新间隔（毫秒），调大可进一步降低 CPU 占用。
    #[arg(long, value_name = "MS", value_parser = clap::value_parser!(u64).range(50..=5000))]
    pub tick_ms: Option<u64>,

    /// 搜索结果与歌单广场的每页条目数。
    ///
    /// 只影响这两处：歌单 / 榜单 / 歌手的具体歌曲一律取全（那些接口每页硬限 30，
    /// 客户端会自动翻页）。
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(5..=200))]
    pub page_size: Option<u32>,

    /// 访问 KuGouMusicApi 时使用的 HTTP 代理。
    #[arg(long, env = "KUGOU_PROXY", value_name = "URL")]
    pub proxy: Option<String>,

    /// 启动时使用固定色板（16 色），适配老终端。
    #[arg(long)]
    pub basic_color: bool,

    /// 打印最终生效的配置、缓存目录与日志路径后退出。
    #[arg(long)]
    pub print_config: bool,
}
