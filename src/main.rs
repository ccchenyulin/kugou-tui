//! kugou-tui —— 轻量级酷狗音乐命令行 TUI 播放器。
//!
//! # 模块划分
//!
//! ```text
//! cli / config       命令行参数与配置持久化
//! error              统一错误类型
//! logger             极简文件日志（不引入 tracing）
//! util               伪随机数与时间戳（不引入 rand/chrono）
//! event              全进程唯一的事件总线
//! keymap             按键 → 语义动作
//!
//! api/               酷狗接口封装（KuGouMusicApi 客户端）
//!   client.rs          HTTP 层
//!   model.rs           领域模型 + 防御性 JSON 解析
//!   catalog.rs         搜索 / 歌单 / 歌手 / 排行榜 / 播放直链
//!   lyric.rs           歌词获取与 LRC 解析
//!   cloud.rs           设备指纹 / 云端歌单增删
//!
//! audio/             音频子系统
//!   cache.rs           磁盘缓存与容量回收
//!   download.rs        直链下载（先落盘再播）
//!   engine.rs          独占线程的 rodio 播放引擎
//!
//! app/               编排层
//!   state.rs           纯数据状态
//!   queue.rs           播放队列与播放模式
//!   update.rs          事件 → 状态变更（唯一改状态的地方）
//!   mod.rs             App 装配与主循环
//!
//! ui/                终端界面
//!   theme.rs           配色
//!   widgets.rs         渲染原语
//!   views/             各面板
//! ```
//!
//! # 数据流
//!
//! ```text
//! 用户按键 ─┐
//! 音频事件 ─┼─▶ EventBus ─▶ App::handle_event ─▶ AppState ─▶ ui::render ─▶ 终端
//! 网络结果 ─┘                      │
//!                                 └─▶ runtime.spawn(...) ─▶ 新的网络请求
//! ```

mod api;
mod app;
mod audio;
mod cli;
mod config;
mod error;
mod event;
mod keymap;
mod logger;
mod mpris;
mod source;
mod ui;
mod util;

use anyhow::Context;
use clap::Parser;

use crate::cli::Cli;
use crate::config::Config;
use crate::logger::tlog;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 优先级：命令行 > 环境变量（clap 直接读入 Cli）> 配置文件 > 默认值
    let mut config = Config::load();

    // 启动对齐：顶层 api_base / cookie / dfid 一律跟随选中的音源。
    //
    // 这三者是分别持久化的，历史上出现过「界面显示概念版、实际却在打标准版」的
    // 不一致——原因是某次用 `--api-base` 临时指向别处后被写回了顶层。以 `sources.active`
    // 为准统一一次即可；放在 merge_cli 之前，所以命令行的 --api-base 依旧能覆盖本次会话。
    // 【诊断】记录对齐前后的值，排查「启动时地址不对」。
    // 注意：必须在 logger::init 之后才能打日志，所以先记下来，初始化完再输出。
    let before = format!("{:?}", config.active_source_kind());
    let before_base = config.api_base.clone();

    let active = config.active_source_kind();
    config.switch_source(active);

    let after_base = config.api_base.clone();
    let after = format!("{:?}", config.active_source_kind());

    config.merge_cli(&cli);

    // 日志失败不阻塞使用，只在 stderr 提一句
    let log_path = Config::log_path();
    if let Err(error) = logger::init(&log_path) {
        eprintln!("警告：无法创建日志文件 {}：{error}", log_path.display());
    }
    tlog!(
        logger::LEVEL_INFO,
        "kugou-tui {} 启动，API={}，日志={}",
        env!("CARGO_PKG_VERSION"),
        config.api_base,
        log_path.display()
    );
    tlog!(
        logger::LEVEL_INFO,
        "[诊断] 音源对齐：{} ({}) → {} ({})，merge_cli 后 API={}",
        before_base,
        before,
        after_base,
        after,
        config.api_base
    );

    if cli.print_config {
        print_effective_config(&config);
        return Ok(());
    }

    let mut app = app::App::new(config).context("初始化失败")?;

    // `--search` 让用户直接进入结果页，省一次按键
    if let Some(keyword) = cli
        .search
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        app.startup_search(keyword);
    }

    app.run()
}

/// `--print-config`：把最终生效的配置打印出来，方便排查「为什么没读我的配置文件」。
fn print_effective_config(config: &Config) {
    let cache_limit = if config.cache_limit_mib == 0 {
        "不限".to_string()
    } else {
        format!("{} MiB", config.cache_limit_mib)
    };

    println!("kugou-tui {}", env!("CARGO_PKG_VERSION"));
    println!("配置文件  : {}", Config::path().display());
    println!("日志文件  : {}", Config::log_path().display());
    println!("API 地址  : {}", config.api_base);
    // 顺带提示服务端该配什么 `platform`：两个平台的鉴权不通用，配错了会退化成试听
    let kind = config.active_source_kind();
    println!(
        "当前音源  : {}{}",
        kind.label(),
        match kind.platform_env() {
            Some(value) => format!("（服务端需 platform={value}）"),
            None => "（服务端不设 platform）".to_string(),
        }
    );
    println!(
        "登录状态  : {}",
        if config.is_logged_in() {
            "已登录"
        } else {
            "未登录（云端歌单不可用）"
        }
    );
    println!(
        "设备指纹  : {}",
        config.dfid.as_deref().unwrap_or("（尚未获取）")
    );
    println!("音质      : {}", config.quality);
    println!("音量      : {:.0}%", config.volume * 100.0);
    println!("播放模式  : {}", config.playback_mode.label());
    println!("缓存目录  : {}", config.cache_dir.display());
    println!("缓存上限  : {cache_limit}");
    println!("刷新间隔  : {} ms", config.tick_ms);
    println!("每页条目  : {}", config.page_size);
    println!(
        "代理      : {}",
        config.proxy.as_deref().unwrap_or("（未设置）")
    );
    println!(
        "16 色模式 : {}",
        if config.basic_color { "是" } else { "否" }
    );
}
