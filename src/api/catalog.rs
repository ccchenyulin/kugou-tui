//! 目录类接口：搜索、歌单、歌手、排行榜、播放直链。
//!
//! 所有路径都对照 KuGouMusicApi 的接口文档核对过：
//!
//! | 功能 | 路径 |
//! |---|---|
//! | 单曲搜索 | `GET /search?type=song` |
//! | 歌单广场 | `GET /top/playlist?category_id=` |
//! | 歌单详情 | `GET /playlist/detail?ids=` |
//! | 歌单歌曲 | `GET /playlist/track/all?id=` |
//! | 用户歌单 | `GET /user/playlist` |
//! | 用户歌单歌曲 | `GET /playlist/track/all/new?listid=` |
//! | 歌手列表 | `GET /artist/lists` |
//! | 歌手单曲 | `GET /artist/audios?id=` |
//! | 排行榜列表 | `GET /rank/list` |
//! | 排行榜歌曲 | `GET /rank/audio?rankid=` |
//! | 播放直链 | `GET /song/url?hash=` |
//!
//! # 关于搜索
//!
//! 上游 `/search` 支持 `type=song|special|author`，但本客户端**只用了 `type=song`**：
//! 歌单与歌手改为在各自的标签页里浏览完整目录，一次搜索只发一个请求。
//!
//! # 关于分页
//!
//! `pagesize` 在歌单类接口上是**硬上限 30**（传更大值也只回 30 条），
//! 且响应里的 `count` 只是回显当页条数、不是总数。因此"取全"一律靠翻页，
//! 见本文件末尾的 `*_all` 系列函数。

use serde_json::Value;

use crate::api::client::ApiClient;
use crate::api::model::{
    Artist, Playlist, RankBoard, Song, artist_from_json, extract_songs, pick_i64, pick_string,
    playlist_from_json, rank_board_from_json,
};
use crate::api::{data_of, extract_list};
use crate::error::{AppError, Result};

/// 歌单 / 榜单类接口每页的硬上限。
///
/// 实测 `/playlist/track/all` 传 `pagesize=100|300|500` 都只回 30 条，所以取全只能翻页。
/// 榜单接口其实能吃下更大的值，但统一用这个上限更省心，也免得榜单扩容后要回头改。
/// 歌单 / 榜单类接口每页的硬上限。首屏加载也用它，保证与翻页一致。
pub(crate) const PAGE_LIMIT: u32 = 30;

/// 翻页上限，防止接口异常（比如始终回满页）时无限循环。60 页 = 1800 首，足够极端歌单。
const MAX_PAGES: u32 = 60;

/// 并发翻页时一批发多少页。太大容易触发上游限流，太小提速不明显；6 是个折中。
const CONCURRENT_PAGES: u32 = 6;

impl ApiClient {
    // ------------------------------------------------------------------
    // 搜索
    // ------------------------------------------------------------------

    /// 单曲搜索。
    ///
    /// 需要认证信息：cookie 缺失时服务端返回 `error_code: 152`，
    /// [`AppError::is_auth_related`] 会把它翻译成「需要登录」提示。
    pub async fn search_songs(
        &self,
        keywords: &str,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Song>> {
        let root = self
            .get_json_uncached(
                "/search",
                &[
                    ("keywords", keywords.to_string()),
                    ("type", "song".to_string()),
                    ("page", page.to_string()),
                    ("pagesize", page_size.to_string()),
                ],
            )
            .await?;
        Ok(extract_songs(data_of(&root)))
    }

    // ------------------------------------------------------------------
    // 歌单
    // ------------------------------------------------------------------

    /// 歌单广场。`category_id = 0` 表示推荐，`11292` 是 HI-RES。
    pub async fn plaza_playlists(
        &self,
        category_id: i64,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Playlist>> {
        let root = self
            .get_json(
                "/top/playlist",
                &[
                    ("category_id", category_id.to_string()),
                    ("withsong", "0".to_string()),
                    ("withtag", "1".to_string()),
                    ("page", page.to_string()),
                    ("pagesize", page_size.to_string()),
                ],
            )
            .await?;
        Ok(collect_playlists(&root, false))
    }

    /// 歌单内一页歌曲（公开歌单，用 `global_collection_id`）。
    ///
    /// # `pagesize` 是硬上限 30
    ///
    /// 实测传 `100` / `300` / `500`，服务端一律只回 30 条；而 `data.count` 只是
    /// **回显当页条数**，不是总数（歌单元数据里的 `percount` 恒为 0，也拿不到总数）。
    /// 所以要取全必须自己翻页，见 [`Self::playlist_tracks_all`]。
    pub async fn playlist_tracks(
        &self,
        global_id: &str,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Song>> {
        let root = self
            .get_json(
                "/playlist/track/all",
                &[
                    ("id", global_id.to_string()),
                    ("page", page.to_string()),
                    ("pagesize", page_size.to_string()),
                ],
            )
            .await?;
        Ok(extract_songs(data_of(&root)))
    }

    /// 当前用户歌单（含自建与收藏）。
    pub async fn user_playlists(&self) -> Result<Vec<Playlist>> {
        let root = self
            .get_json_uncached(
                "/user/playlist",
                &[("page", "1".to_string()), ("pagesize", "100".to_string())],
            )
            .await?;
        Ok(collect_playlists(&root, true))
    }

    /// 用户歌单内的歌曲（新版接口，按数字 `listid`）。
    pub async fn user_playlist_tracks(
        &self,
        list_id: i64,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Song>> {
        let root = self
            .get_json(
                "/playlist/track/all/new",
                &[
                    ("listid", list_id.to_string()),
                    ("page", page.to_string()),
                    ("pagesize", page_size.to_string()),
                ],
            )
            .await?;
        Ok(extract_songs(data_of(&root)))
    }

    // ------------------------------------------------------------------
    // 歌手
    // ------------------------------------------------------------------

    /// 歌手列表。`kind`: 0 全部 / 1 华语 / 2 欧美 / 3 日韩。
    pub async fn artist_list(&self, kind: i64, hot_size: u32) -> Result<Vec<Artist>> {
        let root = self
            .get_json(
                "/artist/lists",
                &[
                    ("sextypes", "0".to_string()),
                    ("type", kind.to_string()),
                    ("musician", "0".to_string()),
                    ("hotsize", hot_size.to_string()),
                ],
            )
            .await?;
        Ok(extract_list(
            &root,
            &["info", "list", "singer_list", "authors"],
            artist_from_json,
        ))
    }

    /// 歌手单曲。`sort`: `hot` 热门 / `new` 最新。
    pub async fn artist_tracks(
        &self,
        artist_id: i64,
        sort: &str,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Song>> {
        let root = self
            .get_json(
                "/artist/audios",
                &[
                    ("id", artist_id.to_string()),
                    ("sort", sort.to_string()),
                    ("page", page.to_string()),
                    ("pagesize", page_size.to_string()),
                ],
            )
            .await?;
        Ok(extract_songs(data_of(&root)))
    }

    // ------------------------------------------------------------------
    // 排行榜
    // ------------------------------------------------------------------

    /// 排行榜列表。
    pub async fn rank_boards(&self) -> Result<Vec<RankBoard>> {
        let root = self
            .get_json("/rank/list", &[("withsong", "0".to_string())])
            .await?;
        Ok(extract_list(
            &root,
            &["info", "list", "rank_list"],
            rank_board_from_json,
        ))
    }

    /// 排行榜歌曲。
    pub async fn rank_tracks(&self, rank_id: i64, page: u32, page_size: u32) -> Result<Vec<Song>> {
        let root = self
            .get_json(
                "/rank/audio",
                &[
                    ("rankid", rank_id.to_string()),
                    ("page", page.to_string()),
                    ("pagesize", page_size.to_string()),
                ],
            )
            .await?;
        Ok(extract_songs(data_of(&root)))
    }

    // ------------------------------------------------------------------
    // 取全（翻页）
    // ------------------------------------------------------------------

    /// 逐页取全量歌曲的通用实现（**并发**翻页）。
    ///
    /// # 为什么要并发
    ///
    /// 歌单接口硬限每页 30 条，400 首歌就是 14 次往返。串行做的话 RTT 直接累加，
    /// 大歌单要等好几秒——而这几秒里界面只有一个"加载中"。改成一批页同时发，
    /// 耗时就接近单页的延迟。
    ///
    /// # 批次策略
    ///
    /// 不知道总数，所以**先取第 1 页**：它满页说明后面可能还有，就按
    /// [`CONCURRENT_PAGES`] 一批继续取；某一页不满就停（同串行版的判定）。
    /// 一批里只要有一页失败就整体报错，避免静默漏歌。
    async fn collect_all_pages<F, Fut>(&self, make: F) -> Result<Vec<Song>>
    where
        F: Fn(ApiClient, u32) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<Song>>> + Send + 'static,
    {
        let mut all: Vec<Song> = Vec::new();

        // 第 1 页单独取：确定后面还有没有内容，避免一上来就并发一堆空请求
        let first = make(self.clone(), 1).await?;
        // 只有**空**才说明真没了。
        //
        // 不能用「不足 PAGE_LIMIT」判断：解析时会有条目被过滤掉
        // （缺 hash、字段类型不对等），一页 30 条剩 29 条是常事，
        // 那样会误判成"没有下一页"，歌单直接被截断在 29 首。
        all.extend(first);
        if all.is_empty() {
            return Ok(all);
        }

        let mut page: u32 = 2;
        while page <= MAX_PAGES {
            let batch_end = (page + CONCURRENT_PAGES - 1).min(MAX_PAGES);

            let mut set = tokio::task::JoinSet::new();
            for batch_page in page..=batch_end {
                // 每页一份克隆：ApiClient 内部是 Arc，克隆很便宜
                set.spawn(make(self.clone(), batch_page));
            }

            let mut stop = false;
            while let Some(joined) = set.join_next().await {
                match joined {
                    Ok(Ok(songs)) => {
                        // 同理，只有空页才停；短页后面可能还有内容
                        if songs.is_empty() {
                            stop = true;
                        }
                        all.extend(songs);
                    }
                    Ok(Err(error)) => {
                        // 页码越界 = 后面没有了，属正常终止，保留已取到的内容。
                        // 不这样做的话，搜「周杰伦」（上限 16 页）会整次失败，
                        // 界面只剩首屏那 30 条。
                        if error.is_page_out_of_range() {
                            stop = true;
                            continue;
                        }
                        return Err(error);
                    }
                    Err(error) => {
                        // 任务本身panic/取消。不该发生，报出来而不是静默丢页
                        crate::logger::tlog!(crate::logger::LEVEL_WARN, "翻页任务失败：{error}");
                        return Err(AppError::Other(format!("翻页任务失败：{error}")));
                    }
                }
            }

            if stop {
                break;
            }
            page = batch_end + 1;
        }

        Ok(all)
    }

    /// 歌单内**全部**歌曲（公开歌单）。
    pub async fn playlist_tracks_all(&self, global_id: &str) -> Result<Vec<Song>> {
        self.collect_all_pages(|client, page| {
            let global_id = global_id.to_string();
            async move { client.playlist_tracks(&global_id, page, PAGE_LIMIT).await }
        })
        .await
    }

    /// 用户歌单内**全部**歌曲（自建/收藏，按数字 `listid`）。
    pub async fn user_playlist_tracks_all(&self, list_id: i64) -> Result<Vec<Song>> {
        self.collect_all_pages(move |client, page| async move {
            client.user_playlist_tracks(list_id, page, PAGE_LIMIT).await
        })
        .await
    }

    /// 歌手**全部**歌曲。`sort` 同 [`Self::artist_tracks`]。
    pub async fn artist_tracks_all(&self, artist_id: i64, sort: &str) -> Result<Vec<Song>> {
        self.collect_all_pages(|client, page| {
            let sort = sort.to_string();
            async move {
                client
                    .artist_tracks(artist_id, &sort, page, PAGE_LIMIT)
                    .await
            }
        })
        .await
    }

    /// 榜单**全部**歌曲。
    ///
    /// `/rank/audio` 不像歌单那样硬限 30（传 100 能回 100），但统一按页取更省心，
    /// 也避免榜单扩容后要回头改。
    pub async fn rank_tracks_all(&self, rank_id: i64) -> Result<Vec<Song>> {
        self.collect_all_pages(move |client, page| async move {
            client.rank_tracks(rank_id, page, PAGE_LIMIT).await
        })
        .await
    }

    // ------------------------------------------------------------------
    // 播放直链
    // ------------------------------------------------------------------

    /// 取播放直链。
    ///
    /// 注意：该接口依赖 `dfid`，缺失时酷狗会返回「本次请求需要验证」并给出空 url。
    /// 调用方应保证 [`crate::api::cloud::ApiClient::fetch_device_fingerprint`] 已成功执行。
    ///
    /// # 为什么分两步
    ///
    /// `free_part=true` 的含义是「返回试听部分」。实测只要带上它，服务端就直接给
    /// 60 秒试听片段（937 KiB ≈ 60 秒），**即使账号有会员也被降级**——这正是
    /// 「VIP 歌曲只能听几十秒」的直接原因。
    ///
    /// 所以先按完整版请求；只有完整版确实拿不到（未登录 / 会员类型不匹配 /
    /// 需单独购买）时，才退而求其次要试听片段，并明确标记 [`StreamUrl::is_trial`]，
    /// 由界面告诉用户这是片段、不要误当成播完了。
    pub async fn song_stream_url(&self, song: &Song, quality: &str) -> Result<StreamUrl> {
        let full = self.request_song_url(song, quality, false).await?;
        if let Some(url) = extract_stream_url(&full) {
            return Ok(StreamUrl {
                url,
                is_trial: false,
                reason: None,
            });
        }

        // 完整版拿不到，先记下原因再退到试听片段
        let reason = Self::fail_reason(&full);

        let trial = self.request_song_url(song, quality, true).await?;
        if let Some(url) = extract_stream_url(&trial) {
            return Ok(StreamUrl {
                url,
                is_trial: true,
                reason,
            });
        }

        // 两次都没有。优先把服务端给的原因透出去，比笼统的「可能需要 VIP」有用得多。
        for root in [&full, &trial] {
            if let Some(reason) = pick_string(root, &["error", "error_msg", "msg"]) {
                return Err(AppError::Api {
                    path: "/song/url".to_string(),
                    code: pick_i64(root, &["errcode", "error_code"]).unwrap_or_default(),
                    message: reason,
                });
            }

            // `fail_process` 会说明卡在哪一步，实测见过 ["pkg","buy"]（需购买/开通）
            if let Some(process) = root.get("fail_process").and_then(Value::as_array) {
                if !process.is_empty() {
                    let steps = process
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("/");
                    return Err(AppError::NotFound(format!(
                        "《{}》需要开通或购买（{}）",
                        song.name, steps
                    )));
                }
            }
        }

        Err(AppError::NotFound(format!(
            "《{}》没有可用的播放地址（可能需要 VIP、已下架或版权受限）",
            song.name
        )))
    }

    /// 从响应里读出「为什么给不了完整版」。
    ///
    /// `fail_process` 是服务端给的步骤数组，实测见过 `["pkg","buy"]`（需开通或购买）。
    /// 读不到就返回 None，交给调用方用通用文案。
    fn fail_reason(root: &Value) -> Option<String> {
        let array = root.get("fail_process").and_then(Value::as_array)?;
        let steps: Vec<&str> = array.iter().filter_map(Value::as_str).collect();
        if steps.is_empty() {
            return None;
        }
        Some(match steps.join("/").as_str() {
            "pkg" => "需要开通会员".to_string(),
            "pkg/buy" => "需要开通会员或单独购买该专辑".to_string(),
            other => format!("服务端返回 {other}"),
        })
    }

    /// 发一次 `/song/url`。`free_part` 决定是否只要试听片段。
    async fn request_song_url(&self, song: &Song, quality: &str, free_part: bool) -> Result<Value> {
        let mut query = vec![
            ("hash", song.hash.clone()),
            ("album_id", song.album_id.clone()),
            ("album_audio_id", song.album_audio_id.to_string()),
            ("quality", quality.to_string()),
        ];
        if free_part {
            query.push(("free_part", "true".to_string()));
        }

        self.get_json_uncached("/song/url", &query).await
    }
}

/// `/song/url` 的结果。
pub struct StreamUrl {
    pub url: String,
    /// 是否为试听片段（完整版拿不到时的降级结果，通常只有 60 秒）。
    pub is_trial: bool,
    /// 完整版为什么拿不到。取自服务端 `fail_process`（如 `["pkg","buy"]`）。
    ///
    /// 有了它，用户看到「只有试听」时就知道是被什么挡住了，而不是只能瞎猜
    /// 「是不是会员没生效」。
    pub reason: Option<String>,
}

/// 从响应里收集歌单列表。
///
/// `assume_own` 为真时，对服务端未返回 `is_self` 但带数字 `listid` 的条目按
/// 「当前用户可写」处理。**只有 `/user/playlist` 能这么假设**：歌单广场返回的是
/// 别人创建的歌单，同样带 `specialid` 却没有 `is_self`，误判会把陌生人的歌单
/// 标成「可写」，还能被设成同步目标。
fn collect_playlists(root: &Value, assume_own: bool) -> Vec<Playlist> {
    extract_list(
        root,
        &["info", "list", "lists", "special_list", "data"],
        |value| {
            let mut playlist = playlist_from_json(value)?;
            if assume_own && playlist.list_id.is_some() && value.get("is_self").is_none() {
                playlist.is_own = true;
            }
            Some(playlist)
        },
    )
}

/// 从 `/song/url` 的响应里挖出直链。
///
/// 这个接口的布局尤其不统一，实测见过四种：
///
/// * **顶层** `url` / `backupUrl`（数组）—— 目前最常见，`data` 根本不存在
/// * `data[0].url`
/// * `data.url`
/// * `data.play_url`
///
/// 且 `url` 有时是字符串、有时是数组。所以按「data 数组 → data 对象 → 顶层」
/// 三级找，每级内部再试多个键名。
fn extract_stream_url(root: &Value) -> Option<String> {
    const URL_KEYS: &[&str] = &["url", "play_url", "backup_url", "backupUrl"];

    if let Some(items) = root.get("data").and_then(Value::as_array) {
        if let Some(found) = items.iter().find_map(|item| pick_url(item, URL_KEYS)) {
            return Some(found);
        }
    }

    if let Some(data) = root.get("data") {
        if let Some(found) = pick_url(data, URL_KEYS) {
            return Some(found);
        }
    }

    pick_url(root, URL_KEYS)
}

fn pick_url(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        match value.get(*key) {
            Some(Value::String(url)) if is_http_url(url) => return Some(url.trim().to_string()),
            Some(Value::Array(items)) => {
                let found = items.iter().find_map(|item| match item {
                    Value::String(text) if is_http_url(text) => Some(text.trim().to_string()),
                    _ => None,
                });
                if found.is_some() {
                    return found;
                }
            }
            _ => {}
        }
    }
    None
}

fn is_http_url(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with("http://") || trimmed.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_url_in_array_payload() {
        let root = json!({"data": [{"url": ["http://a/1.mp3"], "play_url": "http://a/2.mp3"}]});
        assert_eq!(extract_stream_url(&root).as_deref(), Some("http://a/1.mp3"));
    }

    #[test]
    fn finds_url_at_top_level() {
        // 实测最常见的新形状：没有 `data`，url / backupUrl 直接挂在顶层
        let root = json!({
            "status": 1,
            "url": ["http://fsandroid.tx.kugou.com/a.mp3", "http://fsmobile.kugou.com/a.mp3"],
            "backupUrl": ["http://fsmobile.kugou.com/b.mp3"]
        });
        assert_eq!(
            extract_stream_url(&root).as_deref(),
            Some("http://fsandroid.tx.kugou.com/a.mp3")
        );
    }

    #[test]
    fn falls_back_to_backup_url() {
        let root = json!({
            "status": 1,
            "url": [],
            "backupUrl": ["http://fsmobile.kugou.com/b.mp3"]
        });
        assert_eq!(
            extract_stream_url(&root).as_deref(),
            Some("http://fsmobile.kugou.com/b.mp3")
        );
    }

    #[test]
    fn finds_url_in_object_payload() {
        let root = json!({"data": {"play_url": "https://b/1.flac"}});
        assert_eq!(
            extract_stream_url(&root).as_deref(),
            Some("https://b/1.flac")
        );
    }

    #[test]
    fn ignores_non_http_placeholder() {
        let root = json!({"data": {"url": ""}});
        assert_eq!(extract_stream_url(&root), None);
    }
}
