//! 网易云音乐音源（NeteaseCloudMusicApi）。
//!
//! 部署方式与 KuGouMusicApi 一致：`git clone` + `npm install` + `node app.js`，
//! 默认 `http://127.0.0.1:3002`。
//!
//! # 与酷狗的差异
//!
//! 只实现了「搜索 / 播放直链 / 歌词 / 封面」四项核心能力。歌单广场、榜单、歌手
//! 目录与云端歌单同步不提供——第三方服务没有对应的登录态，硬凑出来的结果只会
//! 误导用户。UI 通过 [`crate::source::Capability`] 感知这一点，不会去调它们。
//!
//! # 验证状态
//!
//! **本模块未经真实服务验证**（当前环境只部署了 KuGouMusicApi）。字段布局依据
//! NeteaseCloudMusicApi 的公开文档编写，解析全部走防御式取值，缺字段不会 panic。
//! 首次使用时请对照日志核对。

use serde_json::Value;

use crate::api::client::ApiClient;
use crate::api::model::{Lyric, Singer, Song, pick_i64, pick_string, pick_u64};
use crate::api::{data_of, extract_list};
use crate::error::Result;

/// 搜索结果里每页取多少条。与酷狗保持一致，便于 UI 分页逻辑复用。
const DEFAULT_PAGE_SIZE: u32 = 30;

/// 单曲搜索。
///
/// 接口 `GET /search`，关键词参数名是 `keywords`（不是酷狗的 `keywords` 之外的
/// 其它叫法），分页用 `offset` + `limit` 而不是 `page` + `pagesize`。
pub async fn search_songs(
    client: &ApiClient,
    keyword: &str,
    page: u32,
    page_size: u32,
) -> Result<Vec<Song>> {
    let limit = if page_size == 0 {
        DEFAULT_PAGE_SIZE
    } else {
        page_size
    };
    // 第一页 offset 为 0
    let offset = page.saturating_sub(1).saturating_mul(limit);

    let root = client
        .get_json_uncached(
            "/search",
            &[
                ("keywords", keyword.to_string()),
                ("limit", limit.to_string()),
                ("offset", offset.to_string()),
            ],
        )
        .await?;

    let songs = extract_list(data_of(&root), &["songs"], song_from_json);
    Ok(songs)
}

/// 从搜索结果的一条记录里解析出 [`Song`]。
///
/// 网易云的字段是嵌套的：歌手在 `artists[]`、专辑在 `album{}`、封面在
/// `album.picUrl`。全部走 `Option` 取值，任何一段缺失只丢这一条。
fn song_from_json(value: &Value) -> Option<Song> {
    // 网易云用数字 id 标识歌曲，它是取播放链接与歌词的唯一依据
    let id = pick_i64(value, &["id"])?;
    let name = pick_string(value, &["name"]).unwrap_or_else(|| "未知曲目".to_string());

    let album = value.get("album");
    let cover = album
        .and_then(|album| pick_string(album, &["picUrl", "pic", "img1v1Url"]))
        // 网易云的封面 URL 可以带尺寸参数，缩到 300 够终端与桌面控件用
        .map(|url| {
            if url.contains('?') {
                url
            } else {
                format!("{url}?param=300y300")
            }
        });

    Some(Song {
        name,
        // 复用酷狗的 hash 字段存放 id：它是本音源取链接与歌词的主键，
        // 与酷狗的 FileHash 在各自音源内语义等价（都唯一标识一首歌）。
        hash: id.to_string(),
        album_id: album
            .and_then(|album| pick_i64(album, &["id"]))
            .map(|id| id.to_string())
            .unwrap_or_default(),
        // 网易云用 duration 表示毫秒，与领域模型一致，无需换算
        duration_ms: pick_u64(value, &["duration"]).unwrap_or_default(),
        singers: parse_singers(value),
        album_name: album
            .and_then(|album| pick_string(album, &["name"]))
            .unwrap_or_default(),
        cover,
        // 网易云不返回版权标记，按可播处理；取不到链接时会有明确报错
        privilege: None,
        album_audio_id: 0,
        file_id: None,
    })
}

/// 歌手数组：`artists[].name`。
fn parse_singers(value: &Value) -> Vec<Singer> {
    let Some(artists) = value.get("artists").and_then(Value::as_array) else {
        return Vec::new();
    };
    artists
        .iter()
        .filter_map(|artist| {
            Some(Singer {
                id: pick_i64(artist, &["id"]).unwrap_or_default(),
                name: pick_string(artist, &["name"])?,
            })
        })
        .collect()
}

/// 取播放直链。
///
/// `br` 是码率，按配置里的音质映射过去；拿不到 `url` 时把服务端给的原因透出去。
///
/// 网易云没有酷狗那样的「试听片段」降级机制，取不到就是取不到，因此
/// `is_trial` 恒为 `false`。
pub async fn song_stream_url(
    client: &ApiClient,
    song: &Song,
    quality: &str,
) -> Result<crate::api::catalog::StreamUrl> {
    let bitrate = bitrate_for(quality);
    let root = client
        .get_json_uncached(
            "/song/url",
            &[("id", song.hash.clone()), ("br", bitrate.to_string())],
        )
        .await?;

    let data = data_of(&root);
    // 响应是 { data: [ { id, url, ... } ] }
    let url = data
        .get(0)
        .or(Some(data))
        .and_then(|entry| pick_string(entry, &["url"]))
        .filter(|url| !url.trim().is_empty());

    match url {
        Some(url) => Ok(crate::api::catalog::StreamUrl {
            url,
            is_trial: false,
            reason: None,
        }),
        None => Err(crate::error::AppError::NotFound(format!(
            "《{}》没有可用的播放地址（可能需要 VIP 或已下架）",
            song.name
        ))),
    }
}

/// 音质 → 码率。网易云用具体码率而不是酷狗那样的档位名。
fn bitrate_for(quality: &str) -> u32 {
    match quality {
        "320" => 320_000,
        "flac" | "super" | "high" => 999_000,
        // 默认 128：体积最小、成功率最高
        _ => 128_000,
    }
}

/// 取歌词。
///
/// 网易云把译文单独放在 `tlyric`，这里按行挂到 [`LyricLine::translation`]，
/// 与酷狗从 KRC 里提取译文后的结果一致，UI 不需要区分音源。
pub async fn fetch_lyric(client: &ApiClient, song: &Song) -> Result<Lyric> {
    let root = client
        .get_json_uncached("/lyric", &[("id", song.hash.clone())])
        .await?;
    let data = data_of(&root);

    let main = data
        .get("lrc")
        .and_then(|lrc| pick_string(lrc, &["lyric"]))
        .unwrap_or_default();
    if main.trim().is_empty() {
        return Err(crate::error::AppError::NotFound(format!(
            "未找到《{}》的歌词",
            song.name
        )));
    }

    let mut lyric = crate::api::lyric::parse_lrc(&main);
    attach_netease_translation(&mut lyric, data);
    Ok(lyric)
}

/// 把 `tlyric` 的译文按行挂上去。
///
/// 译文与原文都是 LRC 文本，时间标签一一对应；按时间戳配对比按行号配对稳，
/// 能容忍译文缺行。
fn attach_netease_translation(lyric: &mut Lyric, data: &Value) {
    let Some(translation) = data
        .get("tlyric")
        .and_then(|tlyric| pick_string(tlyric, &["lyric"]))
    else {
        return;
    };
    let translated = crate::api::lyric::parse_lrc(&translation);
    if translated.is_empty() {
        return;
    }

    for line in lyric.lines.iter_mut() {
        let Some(found) = translated
            .lines
            .iter()
            .find(|candidate| candidate.time_ms == line.time_ms)
        else {
            continue;
        };
        if !found.text.trim().is_empty() {
            line.translation = Some(found.text.clone());
        }
    }
}

// ============================================================================
// 扫码登录
// ============================================================================
//
// NeteaseCloudMusicApi 的扫码三步：
//   1. `/login/qr/key`            → unikey
//   2. `/login/qr/create?key=...` → 二维码内容（这里要的是文本，见下）
//   3. `/login/qr/check?key=...`  → 状态
//
// ⚠️ 与本模块其它方法一样**未经真实服务验证**。状态码语义是公开的：
// 800 过期 / 801 等待扫码 / 802 待确认 / 803 已授权。

/// 取二维码 key（unikey）。
pub async fn login_qr_key(client: &ApiClient) -> Result<String> {
    let root = client.get_json_uncached("/login/qr/key", &[]).await?;
    let data = data_of(&root);
    pick_string(data, &["unikey", "key"]).ok_or_else(|| {
        crate::error::AppError::Other("取登录二维码 key 失败：响应里没有 unikey".to_string())
    })
}

/// 取二维码内容。
///
/// 酷狗那套接口返回的就是二维码文本（由 TUI 自己渲染成方块）；网易云这个接口
/// 默认返回**图片链接**（`qrimg`），要文本得显式带上 `qrimg=false`。
/// 统一取文本，渲染交给 `qr_lines()`，与酷狗共用一套画法。
pub async fn login_qr_create(client: &ApiClient, key: &str) -> Result<String> {
    let root = client
        .get_json_uncached(
            "/login/qr/create",
            &[("key", key.to_string()), ("qrimg", "false".to_string())],
        )
        .await?;
    let data = data_of(&root);

    pick_string(data, &["qrurl", "url", "qrCode"])
        .or_else(|| {
            data.get("data")
                .and_then(|inner| pick_string(inner, &["qrurl", "url"]))
        })
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| {
            crate::error::AppError::Other("生成登录二维码失败：响应里没有二维码内容".to_string())
        })
}

/// 轮询扫码状态。
pub async fn login_qr_check(client: &ApiClient, key: &str) -> Result<crate::api::cloud::QrCheck> {
    use crate::api::cloud::{QrCheck, QrStatus};

    let root = client
        .get_json_uncached("/login/qr/check", &[("key", key.to_string())])
        .await?;
    let data = data_of(&root);
    let code = pick_i64(data, &["code"]).unwrap_or(800);

    // ⚠️ 网易云的登录态由**服务端**持有（NeteaseCloudMusicApi 自己管理 cookie），
    // 授权成功时响应里没有 token / userid 可给客户端。因此这里 token 留 None，
    // 业务层据此把「已登录」标记为该音源的状态，而不是去写 config.cookie。
    let status = match code {
        800 => QrStatus::Expired,
        801 => QrStatus::Waiting,
        802 => QrStatus::Pending,
        803 => QrStatus::Success,
        // 其它一律当过期，避免出现「一直卡在等待中」的假象
        _ => QrStatus::Expired,
    };

    Ok(QrCheck {
        status,
        token: None,
        userid: None,
    })
}

/// 取封面 URL。
///
/// ⚠️ 实测：这个版本的搜索响应里 album **只有 picId 没有 picUrl**，
/// 拿不到地址，必须再查一次 \`/song/detail\`（那里是 \`al.picUrl\`）。
/// 多数第三方 API 都在搜索结果里直接给 URL，这里是例外。
pub async fn cover_url(client: &ApiClient, song: &Song) -> Result<Option<String>> {
    // 已经有了就别多问一次
    if let Some(cover) = song.cover.as_ref() {
        return Ok(Some(cover.clone()));
    }
    if song.hash.is_empty() {
        return Ok(None);
    }

    let root = client
        .get_json_uncached("/song/detail", &[("ids", song.hash.clone())])
        .await?;
    let data = data_of(&root);

    // 实测响应形状：{ songs: [ { al: { picUrl } } ] }
    let url = data
        .get("songs")
        .and_then(Value::as_array)
        .and_then(|songs| songs.first())
        .and_then(|song| song.get("al").or_else(|| song.get("album")))
        .and_then(|album| pick_string(album, &["picUrl", "pic"]));

    Ok(url)
}
