//! 歌词获取与 LRC 解析。
//!
//! # 两步走
//!
//! 酷狗取歌词要两个请求：
//!
//! 1. `GET /search/lyric?hash=&keywords=` → 拿到 `(id, accesskey)`
//! 2. `GET /lyric?id=&accesskey=&fmt=lrc&decode=true` → 拿到 LRC 正文
//!
//! 服务端在 `decode=true` 时会把结果放进 `decodeContent`；如果只给了 base64 的
//! `content`，本地再解一次。
//!
//! # 关于 KRC
//!
//! 酷狗原生的逐字歌词格式是 KRC（`[起始毫秒,持续毫秒]字<偏移,时长,0>…`）。
//! 本项目只要整行歌词，所以请求 `fmt=lrc`；但解析器仍然兼容 KRC 的时间标签，
//! 万一服务端回落到 KRC 也不会整篇解析失败——只是退化成逐行。

use base64::Engine;
use serde_json::Value;

use crate::api::client::ApiClient;
use crate::api::model::{Lyric, LyricLine, Song, pick_string};
use crate::api::{data_of, extract_first};
use crate::error::{AppError, Result};

impl ApiClient {
    /// 取某首歌的歌词。
    pub async fn fetch_lyric(&self, song: &Song) -> Result<Lyric> {
        let (lyric_id, access_key) = self.find_lyric_candidate(song).await?;

        let query = [
            ("id", lyric_id),
            ("accesskey", access_key),
            // 必须是 krc：翻译与音译只在 KRC 的 [language:] 标签里，lrc 没有。
            // 参考 MoeKoeMusic 的实现。
            ("fmt", "krc".to_string()),
            ("decode", "true".to_string()),
            ("charset", "utf8".to_string()),
        ];

        // 该接口在 `decode=true` 下返回 JSON；偶尔直接吐纯文本，两种都接住。
        let body = self.get_text("/lyric", &query).await?;
        let text = match serde_json::from_str::<Value>(&body) {
            Ok(root) => {
                crate::api::model::check_error_code("/lyric", &root)?;
                extract_lyric_text(&root)
            }
            Err(_) => body,
        };

        let mut lyric = parse_lrc(&text);
        attach_translations(&mut lyric, &text);
        if lyric.is_empty() {
            return Err(AppError::NotFound(format!("《{}》的歌词为空", song.name)));
        }
        Ok(lyric)
    }

    /// 第一步：按 hash 找到歌词候选，拿到 `(id, accesskey)`。
    async fn find_lyric_candidate(&self, song: &Song) -> Result<(String, String)> {
        let root = self
            .get_json(
                "/search/lyric",
                &[
                    ("hash", song.hash.clone()),
                    (
                        "keywords",
                        format!("{} - {}", song.singer_text(), song.name),
                    ),
                    ("duration", song.duration_ms.to_string()),
                    // 只要一条，避免返回多个版本还得挑
                    ("man", "no".to_string()),
                ],
            )
            .await?;

        extract_first(&root, |value| {
            let id = pick_string(value, &["id", "lyric_id"])?;
            let access_key = pick_string(value, &["accesskey", "access_key"])?;
            Some((id, access_key))
        })
        .ok_or_else(|| AppError::NotFound(format!("未找到《{}》的歌词", song.name)))
    }
}

/// 从 `/lyric` 的 JSON 响应里取出 LRC 正文。
fn extract_lyric_text(root: &Value) -> String {
    let data = data_of(root);

    if let Some(text) = pick_string(data, &["decodeContent", "lyric", "lrc"])
        .or_else(|| pick_string(root, &["decodeContent", "lyric", "lrc"]))
    {
        return text;
    }

    // 只有 base64 的 `content` 时本地解码
    if let Some(encoded) =
        pick_string(data, &["content"]).or_else(|| pick_string(root, &["content"]))
    {
        if let Ok(decoded) = decode_base64_text(&encoded) {
            return decoded;
        }
        return encoded;
    }

    String::new()
}

fn decode_base64_text(encoded: &str) -> std::result::Result<String, base64::DecodeError> {
    let compact: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD.decode(compact.as_bytes())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// 解析 LRC 歌词。
///
/// 支持：
/// * 一行多个时间标签：`[00:01.00][00:05.00]副歌`（会展开成两行）
/// * 毫秒位数不定：`[00:01]` / `[00:01.5]` / `[00:01.23]` / `[00:01.234]`
/// * KRC 时间标签：`[1234,567]`
/// * 元信息行（`[ti:]` `[ar:]` `[language:...]`）自动跳过
///
/// 结果按时间升序排列，供 [`Lyric::index_at`] 做二分查找。
pub fn parse_lrc(text: &str) -> Lyric {
    let mut lines: Vec<LyricLine> = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        if line.is_empty() {
            continue;
        }

        let (timestamps, remainder) = consume_time_tags(line);
        if timestamps.is_empty() {
            continue;
        }

        let content = clean_krc_markup(remainder);
        if content.is_empty() {
            continue;
        }

        for time_ms in timestamps {
            lines.push(LyricLine {
                time_ms,
                text: content.clone(),
                translation: None,
                romanization: None,
            });
        }
    }

    lines.sort_by_key(|line| line.time_ms);
    lines.dedup_by(|a, b| a.time_ms == b.time_ms && a.text == b.text);

    Lyric { lines }
}

/// 吃掉行首连续的 `[...]` 时间标签，返回 (时间戳列表, 剩余文本)。
fn consume_time_tags(line: &str) -> (Vec<u64>, &str) {
    let mut timestamps = Vec::new();
    let mut offset = 0usize;

    while line[offset..].starts_with('[') {
        let Some(close) = line[offset..].find(']') else {
            break;
        };
        let tag = &line[offset + 1..offset + close];

        // 元信息标签（ti/ar/al/by/language…）不是时间戳，遇到就停止扫描
        let Some(time_ms) = parse_time_tag(tag) else {
            break;
        };

        timestamps.push(time_ms);
        offset += close + 1;
    }

    (timestamps, &line[offset..])
}

/// 解析单个时间标签。
fn parse_time_tag(tag: &str) -> Option<u64> {
    if tag.contains(':') {
        return parse_lrc_tag(tag);
    }
    // KRC：`[起始毫秒,持续毫秒]`
    let (start, _duration) = tag.split_once(',')?;
    start.trim().parse::<u64>().ok()
}

/// 解析 `mm:ss.fff` 形式的标签。
fn parse_lrc_tag(tag: &str) -> Option<u64> {
    let (minutes, rest) = tag.split_once(':')?;
    let minutes: u64 = minutes.trim().parse().ok()?;

    let (seconds_text, fraction_text) = match rest.split_once('.') {
        Some((seconds, fraction)) => (seconds, Some(fraction)),
        None => (rest, None),
    };
    let seconds: u64 = seconds_text.trim().parse().ok()?;
    if seconds >= 60 {
        // 秒数越界说明这不是合法时间标签（可能是 `[ar:xxx:yyy]`）
        return None;
    }

    let millis = fraction_text.map(parse_fraction).unwrap_or(0);
    Some(
        minutes
            .saturating_mul(60_000)
            .saturating_add(seconds.saturating_mul(1_000))
            .saturating_add(millis),
    )
}

/// 把小数部分归一化成毫秒。位数不定，按位权补零。
fn parse_fraction(text: &str) -> u64 {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let value: u64 = digits.parse().unwrap_or(0);
    match digits.len() {
        0 => 0,
        1 => value * 100,
        2 => value * 10,
        3 => value,
        // 超过 3 位就截断，不四舍五入——歌词对齐差 1ms 无感
        _ => digits[..3].parse().unwrap_or(0),
    }
}

/// 去掉 KRC 的内联标记 `<起始偏移,持续时长,0>`。
fn clean_krc_markup(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut depth = 0usize;

    for character in text.chars() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => output.push(character),
            _ => {}
        }
    }

    output.trim().to_string()
}

/// 从 KRC 的 `[language:base64]` 标签里取出译文，按行挂到歌词上。
///
/// 标签形如：
///
/// ```text
/// [language:eyJjb250ZW50IjpbeyJsYW5ndWFnZSI6MCwibHlyaWNDb250ZW50Ijpb...]]
/// ```
///
/// base64 解开后是：
///
/// ```json
/// {"content":[{"type":1,"lyricContent":["译文1","译文2"]},{"type":0,"lyricContent":["音译1"]}]}
/// ```
///
/// `type` 为 1 是翻译、0 是音译，两条轨各自填写、互不影响。
fn attach_translations(lyric: &mut Lyric, krc_text: &str) {
    let Some(payload) = extract_language_payload(krc_text) else {
        return;
    };
    let Some(content) = payload.get("content").and_then(|value| value.as_array()) else {
        return;
    };

    // 按 `type` 取轨道：1 = 翻译，0 = 音译。
    //
    // 注意**不能**用 `language` 区分：实测同一首歌里两个轨道的 `language` 都是 0，
    // 只有 `type` 不同（Bad Apple!! 就是这样）。按 `language` 找会两条都指向音译，
    // 译文永远取不到——这个坑踩过一次，下面的用例锁着它。
    let track = |kind: i64| -> Option<Vec<String>> {
        content
            .iter()
            .find(|section| section.get("type").and_then(serde_json::Value::as_i64) == Some(kind))
            .and_then(|section| section.get("lyricContent"))
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(flatten_lyric_content)
                    .collect::<Vec<_>>()
            })
    };

    let translation = track(1);
    let romanization = track(0);

    for (index, line) in lyric.lines.iter_mut().enumerate() {
        if let Some(items) = translation.as_ref() {
            if let Some(text) = items.get(index) {
                if !text.trim().is_empty() {
                    line.translation = Some(text.clone());
                }
            }
        }
        if let Some(items) = romanization.as_ref() {
            if let Some(text) = items.get(index) {
                if !text.trim().is_empty() {
                    line.romanization = Some(text.clone());
                }
            }
        }
    }
}

/// 取出 `[language:...]` 里的载荷字符串。
fn extract_language_payload(text: &str) -> Option<serde_json::Value> {
    let start = text.find("[language:")? + "[language:".len();
    let end = text[start..].find(']')? + start;
    let raw = &text[start..end];

    // 载荷里偶尔混进换行等字符，先清掉再补 padding
    let cleaned: String = raw.chars().filter(|ch| !ch.is_whitespace()).collect();
    let mut padded = cleaned;
    while padded.len() % 4 != 0 {
        padded.push('=');
    }

    let engine = base64::engine::general_purpose::STANDARD;
    let decoded = engine.decode(padded).ok()?;
    serde_json::from_slice(&decoded).ok()
}

/// `lyricContent` 的元素可能是字符串，也可能是 `["原文","译文"]` 这样的数组。
fn flatten_lyric_content(entry: &serde_json::Value) -> Option<String> {
    match entry {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Array(parts) => {
            let text: String = parts
                .iter()
                .filter_map(|part| part.as_str())
                .collect::<Vec<_>>()
                .join("");
            Some(text)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 译文与音译的提取。
    ///
    /// 锁住两个坑：
    /// 1. 轨道靠 `type` 区分，不是 `language`（实测同一首歌两轨 language 都是 0）
    /// 2. `lyricContent` 的元素是 `["原文","译文"]` 成对数组，取最后一个槽位
    #[test]
    fn attaches_translation_and_romanization() {
        use base64::Engine;

        let payload = r#"{"content":[
            {"language":0,"type":0,"lyricContent":[["","mo "]]},
            {"language":0,"type":1,"lyricContent":[["","就算身处流逝的时光里"]]}
        ]}"#;
        let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
        // 一行 KRC：时间标签形如 [起始毫秒,持续毫秒]
        let krc = format!("[id:1]\n[language:{encoded}]\n[0,1000]日文原文\n");

        let mut lyric = parse_lrc(&krc);
        attach_translations(&mut lyric, &krc);

        assert_eq!(lyric.lines.len(), 1, "应解析出 1 行");
        assert_eq!(lyric.lines[0].text, "日文原文");
        assert_eq!(
            lyric.lines[0].translation.as_deref(),
            Some("就算身处流逝的时光里"),
            "type=1 应填进 translation"
        );
        assert_eq!(
            lyric.lines[0].romanization.as_deref(),
            Some("mo "),
            "type=0 应填进 romanization"
        );
    }

    #[test]
    fn parses_plain_lrc() {
        let text = "[ti:测试]\n[ar:歌手]\n[00:01.00]第一句\n[00:05.50]第二句\n";
        let lyric = parse_lrc(text);
        assert_eq!(lyric.lines.len(), 2);
        assert_eq!(lyric.lines[0].time_ms, 1_000);
        assert_eq!(lyric.lines[1].time_ms, 5_500);
        assert_eq!(lyric.lines[1].text, "第二句");
    }

    #[test]
    fn expands_multiple_tags_on_one_line() {
        let lyric = parse_lrc("[00:01.00][00:05.00]副歌\n");
        assert_eq!(lyric.lines.len(), 2);
        assert_eq!(lyric.lines[0].time_ms, 1_000);
        assert_eq!(lyric.lines[1].time_ms, 5_000);
        assert_eq!(lyric.lines[0].text, lyric.lines[1].text);
    }

    #[test]
    fn handles_varying_fraction_widths() {
        assert_eq!(parse_time_tag("00:01"), Some(1_000));
        assert_eq!(parse_time_tag("00:01.5"), Some(1_500));
        assert_eq!(parse_time_tag("00:01.23"), Some(1_230));
        assert_eq!(parse_time_tag("00:01.234"), Some(1_234));
        assert_eq!(parse_time_tag("01:02.3456"), Some(62_345));
    }

    #[test]
    fn rejects_metadata_tags() {
        assert_eq!(parse_time_tag("ti:标题"), None);
        assert_eq!(parse_time_tag("ar:歌手"), None);
        // `[language:base64...]` 是 MoeKoeMusic 用的翻译元信息
        assert_eq!(parse_time_tag("language:eyJhbGciOi"), None);
        // 秒数越界
        assert_eq!(parse_time_tag("00:99.00"), None);
    }

    #[test]
    fn parses_krc_style_timestamps() {
        let lyric = parse_lrc("[1234,567]逐字歌词\n");
        assert_eq!(lyric.lines.len(), 1);
        assert_eq!(lyric.lines[0].time_ms, 1_234);
    }

    #[test]
    fn strips_krc_inline_markup() {
        let lyric = parse_lrc("[1000,500]海<100,200,0>阔<300,200,0>天空\n");
        assert_eq!(lyric.lines[0].text, "海阔天空");
    }

    #[test]
    fn skips_lines_without_text() {
        let lyric = parse_lrc("[00:01.00]\n[00:02.00]有词\n");
        assert_eq!(lyric.lines.len(), 1);
        assert_eq!(lyric.lines[0].text, "有词");
    }

    #[test]
    fn sorts_out_of_order_lines() {
        let lyric = parse_lrc("[00:05.00]后\n[00:01.00]前\n");
        assert_eq!(lyric.lines[0].text, "前");
    }

    #[test]
    fn extracts_decoded_content_from_json() {
        let root = json!({"status": 1, "decodeContent": "[00:01.00]嗨\n"});
        assert_eq!(extract_lyric_text(&root), "[00:01.00]嗨");
    }

    #[test]
    fn decodes_base64_content_when_decode_content_absent() {
        // "abc" 的 base64
        let root = json!({"content": "YWJj"});
        assert_eq!(extract_lyric_text(&root), "abc");
    }
}
