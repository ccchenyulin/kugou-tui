//! 登录态相关接口：设备指纹、云端歌单增删。
//!
//! # 云端歌单同步怎么工作
//!
//! 酷狗的「云端歌单」就是账号下的普通歌单。同步 = 调 `/playlist/tracks/add`
//! 把本地队列写进去。写操作需要登录 cookie，并且需要数字 `listid`
//! （不是公开歌单的 `global_collection_id`）。
//!
//! 因此 [`ApiClient::user_playlists`](crate::api::ApiClient::user_playlists) 返回的
//! 歌单里只有 [`Playlist::is_writable`] 为真的才能作为同步目标。

use serde_json::Value;

use crate::api::client::ApiClient;
use crate::api::data_of;
use crate::api::model::{Song, pick_i64, pick_string};
use crate::error::{AppError, Result};

/// 会员形态。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum VipKind {
    /// 未识别到有效会员。
    #[default]
    None,
    /// 标准版豪华 VIP（顶层 `is_vip = 1`）。
    Standard,
    /// 酷狗概念版会员（`busi_vip` 里 `busi_type = "concept"`）。
    Concept,
    /// 其它形态，`0` 号元素是 `busi_type` 原文。
    Other(String),
}

/// 会员信息摘要，用于界面显示。
#[derive(Debug, Clone, Default)]
pub struct VipInfo {
    pub kind: VipKind,
    /// 产品类型，如 `svip` / `tvip` / `VIP`。
    pub product: String,
    /// 到期时间原文（服务端给的是 `YYYY-MM-DD HH:MM:SS`）。
    pub end_time: String,
}

impl VipInfo {
    /// 是否有有效会员。
    pub fn is_vip(&self) -> bool {
        !matches!(self.kind, VipKind::None)
    }

    /// 界面用的一行摘要，例如「概念版 SVIP · 至 09-21」。
    pub fn label(&self) -> String {
        if !self.is_vip() {
            return "非会员".to_string();
        }

        let kind = match &self.kind {
            VipKind::Standard => "豪华",
            VipKind::Concept => "概念版",
            VipKind::Other(name) => name.as_str(),
            VipKind::None => "",
        };
        let product = self.product.to_uppercase();

        // 只取日期部分，界面上一行放不下完整时间戳
        let end = match self.end_time.split(' ').next() {
            Some(date) if date.len() >= 10 => date[5..10].to_string(),
            _ => String::new(),
        };

        if end.is_empty() {
            format!("{kind} {product}")
        } else {
            format!("{kind} {product} · 至 {end}")
        }
    }
}

/// 二维码扫码状态。
///
/// 取值来自 `/login/qr/check` 的 `data.status`：
/// `0` 过期 / `1` 等待扫码 / `2` 待确认 / `4` 授权成功。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrStatus {
    Expired,
    Waiting,
    Pending,
    Success,
}

/// 一次扫码状态查询的结果。
pub struct QrCheck {
    pub status: QrStatus,
    /// 授权成功后才有的登录令牌。
    pub token: Option<String>,
    pub userid: Option<String>,
}

impl ApiClient {
    /// 二维码登录第 1 步：取 key。
    pub async fn login_qr_key(&self) -> Result<String> {
        let root = self.get_json_uncached("/login/qr/key", &[]).await?;
        pick_string(data_of(&root), &["qrcode", "key"])
            .ok_or_else(|| AppError::NotFound("`/login/qr/key` 未返回 key".to_string()))
    }

    /// 第 2 步：取二维码内容（一段 URL，由客户端自己渲染成图片）。
    ///
    /// 接口同时会返回 `base64` 的 PNG，但终端里显示不了图片，所以只取 `url` 自行编码。
    pub async fn login_qr_create(&self, key: &str) -> Result<String> {
        let root = self
            .get_json_uncached("/login/qr/create", &[("key", key.to_string())])
            .await?;
        pick_string(data_of(&root), &["url", "qrcode", "qrurl"])
            .ok_or_else(|| AppError::NotFound("`/login/qr/create` 未返回二维码内容".to_string()))
    }

    /// 第 3 步：轮询扫码状态。
    pub async fn login_qr_check(&self, key: &str) -> Result<QrCheck> {
        let root = self
            .get_json_uncached("/login/qr/check", &[("key", key.to_string())])
            .await?;
        let data = data_of(&root);
        let status = match pick_i64(data, &["status", "code"]) {
            Some(0) => QrStatus::Expired,
            Some(2) => QrStatus::Pending,
            Some(4) => QrStatus::Success,
            _ => QrStatus::Waiting,
        };
        Ok(QrCheck {
            status,
            token: pick_string(data, &["token"]),
            userid: pick_string(data, &["userid"]),
        })
    }

    /// 查当前账号的会员信息（`/user/vip/detail`）。
    ///
    /// # 为什么必须看 `busi_vip`
    ///
    /// 顶层 `is_vip` 只反映**标准版豪华 VIP**。酷狗把「概念版」等其它形态的会员放在
    /// `data.busi_vip[]` 里，每项带 `busi_type`（如 `concept`）与 `product_type`
    /// （如 `svip` / `tvip`）。只认顶层字段会把真正的会员判成「没会员」——
    /// 实测某账号顶层 `is_vip: 0`，但 `busi_vip` 里概念版 SVIP 仍在有效期内。
    pub async fn user_vip_detail(&self) -> Result<VipInfo> {
        let root = self.get_json_uncached("/user/vip/detail", &[]).await?;
        let data = data_of(&root);

        let mut info = VipInfo::default();

        // 标准版豪华 VIP
        if pick_i64(data, &["is_vip", "vip_type"]) == Some(1) {
            info.kind = VipKind::Standard;
            info.product = "VIP".to_string();
            info.end_time = pick_string(data, &["vip_end_time"]).unwrap_or_default();
            return Ok(info);
        }

        // 其它形态的会员（概念版等）
        if let Some(entries) = data.get("busi_vip").and_then(Value::as_array) {
            for entry in entries {
                if pick_i64(entry, &["is_vip"]) != Some(1) {
                    continue;
                }
                let busi_type = pick_string(entry, &["busi_type"]).unwrap_or_default();
                info.kind = if busi_type == "concept" {
                    VipKind::Concept
                } else {
                    VipKind::Other(busi_type)
                };
                info.product = pick_string(entry, &["product_type"]).unwrap_or_default();
                info.end_time = pick_string(entry, &["vip_end_time"]).unwrap_or_default();
                return Ok(info);
            }
        }

        Ok(info)
    }

    /// 获取设备指纹 `dfid`。
    ///
    /// `/song/url` 缺了这个参数会返回「本次请求需要验证」。拿到后应回写配置，
    /// 后续启动就不必再请求。
    pub async fn fetch_device_fingerprint(&self) -> Result<String> {
        let root = self.get_json_uncached("/register/dev", &[]).await?;
        let data = data_of(&root);

        pick_string(data, &["dfid", "DFID"])
            .or_else(|| pick_string(&root, &["dfid"]))
            .ok_or_else(|| AppError::NotFound("`/register/dev` 未返回 dfid".to_string()))
    }

    /// 批量把歌曲加入云端歌单。
    ///
    /// 返回实际提交的歌曲数量，便于界面给出「已同步 N 首」的反馈。
    pub async fn add_tracks_to_playlist(&self, list_id: i64, songs: &[Song]) -> Result<usize> {
        if songs.is_empty() {
            return Ok(0);
        }

        // 服务端按逗号分隔多首、按竖线分隔字段，单次提交太多会被截断
        const BATCH_SIZE: usize = 20;
        let mut written = 0usize;

        for chunk in songs.chunks(BATCH_SIZE) {
            let payload = chunk
                .iter()
                .map(encode_track_entry)
                .collect::<Vec<_>>()
                .join(",");

            self.get_json_uncached(
                "/playlist/tracks/add",
                &[("listid", list_id.to_string()), ("data", payload)],
            )
            .await?;

            written += chunk.len();
        }

        Ok(written)
    }

    /// 从云端歌单移除歌曲。
    ///
    /// `file_ids` 是**歌单条目的 `fileid`**，不是歌曲 hash —— 传 hash 会静默删不掉。
    /// fileid 只在歌单接口的返回里才有（见 `Song::file_id`）。
    pub async fn remove_tracks_from_playlist(
        &self,
        list_id: i64,
        file_ids: &[i64],
    ) -> Result<usize> {
        if file_ids.is_empty() {
            return Ok(0);
        }

        let payload = file_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");

        self.get_json_uncached(
            "/playlist/tracks/del",
            &[("listid", list_id.to_string()), ("fileids", payload)],
        )
        .await?;

        Ok(file_ids.len())
    }

    /// 删除（或取消收藏）一个云端歌单。
    pub async fn delete_playlist(&self, list_id: i64) -> Result<()> {
        self.get_json_uncached("/playlist/del", &[("listid", list_id.to_string())])
            .await?;
        Ok(())
    }

    /// 新建一个云端歌单。返回新歌单的 listid（服务端没给时返回 `None`）。
    ///
    /// 只传 `name` 与 `type=0`：文档把 `list_create_userid` / `list_create_listid` 列为必选，
    /// 但那两个是「收藏他人歌单」(`type=1`) 用的，自建歌单不适用。
    pub async fn create_playlist(&self, name: &str) -> Result<Option<i64>> {
        let root = self
            .get_json_uncached(
                "/playlist/add",
                &[("name", name.to_string()), ("type", "0".to_string())],
            )
            .await?;
        Ok(pick_i64(data_of(&root), &["listid", "list_id", "id"]))
    }
}

/// 拼 `/playlist/tracks/add` 的 `data` 参数：`歌名|hash|专辑id|album_audio_id`。
///
/// 歌名里混入 `|` 或 `,` 会破坏分隔结构（用户搜到的歌名完全可能带逗号），
/// 所以先做替换。
fn encode_track_entry(song: &Song) -> String {
    let safe_name = song.name.replace(['|', ','], " ");
    format!(
        "{}|{}|{}|{}",
        safe_name, song.hash, song.album_id, song.album_audio_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_track_entry_with_expected_field_order() {
        let song = Song {
            name: "海阔天空".to_string(),
            hash: "ABC".to_string(),
            album_id: "123".to_string(),
            album_audio_id: 456,
            ..Song::default()
        };
        assert_eq!(encode_track_entry(&song), "海阔天空|ABC|123|456");
    }

    #[test]
    fn sanitizes_separators_inside_song_name() {
        let song = Song {
            name: "Hello, World|Live".to_string(),
            hash: "H".to_string(),
            album_id: "1".to_string(),
            album_audio_id: 2,
            ..Song::default()
        };
        let encoded = encode_track_entry(&song);
        assert_eq!(encoded, "Hello  World Live|H|1|2");
        // 字段数必须仍然是 4
        assert_eq!(encoded.split('|').count(), 4);
    }

    /// 批量加歌走的是「逗号分隔多首、竖线分隔字段」的裸格式，任何残留的分隔符都会
    /// 让服务端解析错位。这里把所有名字里的分隔符都替换掉，保证批量插入顺序不错乱。
    #[test]
    fn batch_payload_keeps_field_count_stable() {
        let names = ["a", "b,c", "d|e", "f,g|h"];
        let songs: Vec<Song> = names
            .iter()
            .map(|name| Song {
                name: (*name).to_string(),
                hash: "H".to_string(),
                album_id: "1".to_string(),
                album_audio_id: 2,
                ..Song::default()
            })
            .collect();

        let payload = songs
            .iter()
            .map(encode_track_entry)
            .collect::<Vec<_>>()
            .join(",");

        // 逗号只能出现在「歌曲之间」，竖线只能出现在「字段之间」
        assert_eq!(payload.matches(',').count(), names.len() - 1);
        for entry in payload.split(',') {
            assert_eq!(entry.split('|').count(), 4);
        }
    }
}
