//! QQ 音乐音源（jsososo/QQMusicApi）。
//!
//! 部署方式与 KuGouMusicApi 一致，默认 `http://127.0.0.1:3003`。
//!
//! # ⚠️ 验证状态
//!
//! **本模块未经真实服务验证。** 字段布局依据 QQMusicApi 的公开说明与 QQ 音乐
//! 通用的响应习惯编写，官方并未给出稳定的字段文档。所有取值都走
//! 「多候选键名 + 全 `Option`」的防御式解析，缺字段只会丢一条数据，不会 panic。
//!
//! 首次使用请对照日志核对；若搜索结果异常，多半是某个候选键名不对，
//! 在对应的 `pick_*` 调用里补一个候选即可。
//!
//! 能力范围同网易云：只有搜索 / 播放 / 歌词 / 封面，目录与云端同步不支持。

use serde_json::Value;

use crate::api::client::ApiClient;
use crate::api::model::{Lyric, Singer, Song, pick_i64, pick_string, pick_u64};
use crate::api::{data_of, extract_list};
use crate::error::Result;

/// 单曲搜索。
///
/// 参数名是 `key`（不是 `keywords`），分页用 `pageNo` + `pageSize`。
pub async fn search_songs(
    client: &ApiClient,
    keyword: &str,
    page: u32,
    page_size: u32,
) -> Result<Vec<Song>> {
    let size = if page_size == 0 { 30 } else { page_size };

    // 实测：路径是 `/getSearchByKey`，参数走 **query**（`?key=`）。
    // 按路由声明的 `:key?` 用路径参数传会被判成空——服务端返回
    // `{"response":"search key is null"}`，看着像没搜到，其实是没传进去。
    let root = client
        .get_json_uncached(
            "/getSearchByKey",
            &[
                ("key", keyword.to_string()),
                ("limit", size.to_string()),
                ("page", page.max(1).to_string()),
            ],
        )
        .await?;

    // 响应嵌得比较深：`response.data.song.list[]`。
    // 同层还有个 `semantic.list`（空的），extract_list 会跳过解析不出内容的那组。
    let songs = extract_list(data_of(&root), &["list"], song_from_json);
    Ok(songs)
}

/// 从搜索结果的一条记录里解析出 [`Song`]。
fn song_from_json(value: &Value) -> Option<Song> {
    // songmid 是取播放链接与歌词的主键
    let mid = pick_string(value, &["songmid", "mid", "songid", "id"])?;
    let name = pick_string(value, &["songname", "name", "title"])
        .unwrap_or_else(|| "未知曲目".to_string());

    let album_mid = pick_string(value, &["albummid", "album_mid"]).or_else(|| {
        value
            .get("album")
            .and_then(|a| pick_string(a, &["mid", "albummid"]))
    });

    let album_name = pick_string(value, &["albumname", "album_name"])
        .or_else(|| value.get("album").and_then(|a| pick_string(a, &["name"])))
        .unwrap_or_default();

    // QQ 音乐的时长字段 `interval` 是**秒**，要换算成毫秒
    let duration_ms = pick_u64(value, &["interval"])
        .map(|seconds| seconds.saturating_mul(1_000))
        .or_else(|| pick_u64(value, &["duration"]))
        .unwrap_or_default();

    Some(Song {
        name,
        // 与网易云一致：把主键放进 hash 字段
        hash: mid.clone(),
        album_id: album_mid.clone().unwrap_or_default(),
        album_audio_id: pick_i64(value, &["songid", "id"]).unwrap_or_default(),
        album_name,
        singers: parse_singers(value),
        duration_ms,
        cover: album_mid.as_deref().map(cover_url),
        privilege: None,
        file_id: None,
    })
}

/// QQ 音乐的封面要按 `albummid` 拼出来，接口本身不直接给 URL。
fn cover_url(album_mid: &str) -> String {
    format!("https://y.gtimg.cn/music/photo_new/T002R300x300M000{album_mid}.jpg")
}

/// 歌手数组；也有只给 `singerName` 字符串的情况。
fn parse_singers(value: &Value) -> Vec<Singer> {
    if let Some(singers) = value.get("singer").and_then(Value::as_array) {
        let parsed: Vec<Singer> = singers
            .iter()
            .filter_map(|singer| {
                Some(Singer {
                    id: pick_i64(singer, &["id", "mid"]).unwrap_or_default(),
                    name: pick_string(singer, &["name", "singerName"])?,
                })
            })
            .collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }

    pick_string(value, &["singerName", "singer_name"])
        .map(|name| vec![Singer { id: 0, name }])
        .unwrap_or_default()
}

/// 取播放直链。
pub async fn song_stream_url(
    client: &ApiClient,
    song: &Song,
    quality: &str,
) -> Result<crate::api::catalog::StreamUrl> {
    // 实测：路径参数 `/getMusicPlay/:songmid`，不是 query
    let root = client
        .get_json_uncached(&format!("/getMusicPlay/{}", song.hash), &[])
        .await?;

    let data = data_of(&root);
    // 常见形状：{ url } / { data: { url } } / { data: { midUrlInfo: [{ purl }] } }
    let url = pick_string(data, &["url", "playUrl", "purl"])
        .or_else(|| {
            data.get("midUrlInfo")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| pick_string(item, &["purl", "url"]))
        })
        .filter(|url| !url.trim().is_empty());

    match url {
        Some(url) => {
            // 部分接口返回的是不含协议头的地址
            let url = if url.starts_with("http") {
                url
            } else {
                format!("https://{url}")
            };
            Ok(crate::api::catalog::StreamUrl {
                url,
                is_trial: false,
                reason: None,
            })
        }
        None => Err(crate::error::AppError::NotFound(format!(
            "《{}》没有可用的播放地址（可能需要 VIP 或已下架）",
            song.name
        ))),
    }
}

/// 音质档位。QQ 音乐用前缀名而不是具体码率。
fn bitrate_for(quality: &str) -> String {
    match quality {
        "320" => "320".to_string(),
        "flac" | "super" => "flac".to_string(),
        _ => "128".to_string(),
    }
}

/// 取歌词。
pub async fn fetch_lyric(client: &ApiClient, song: &Song) -> Result<Lyric> {
    // 实测：路径参数 `/getLyric/:songmid`
    let root = client
        .get_json_uncached(&format!("/getLyric/{}", song.hash), &[])
        .await?;
    let data = data_of(&root);

    let main = pick_string(data, &["lyric", "lrc"])
        .or_else(|| {
            data.get("data")
                .and_then(|d| pick_string(d, &["lyric", "lrc"]))
        })
        .unwrap_or_default();
    if main.trim().is_empty() {
        return Err(crate::error::AppError::NotFound(format!(
            "未找到《{}》的歌词",
            song.name
        )));
    }

    let mut lyric = crate::api::lyric::parse_lrc(&main);
    // 译文：字段可能是 trans / tlyric
    let translation = pick_string(data, &["trans", "tlyric"])
        .or_else(|| data.get("data").and_then(|d| pick_string(d, &["trans"])));
    if let Some(text) = translation {
        let translated = crate::api::lyric::parse_lrc(&text);
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
    Ok(lyric)
}
