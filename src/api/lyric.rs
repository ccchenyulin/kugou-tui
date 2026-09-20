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
    ///
    /// # 为什么要试多个候选
    ///
    /// 同一个 hash 酷狗往往提供多个 KRC 变体：**有的只带罗马音，有的带真正的中文译文**。
    /// 只取第一个的话，日语歌很容易拿到罗马音版——界面就显示成一串拼音，等于没有翻译。
    ///
    /// 所以这里遍历候选，优先采用「[language:] 里有 CJK 译文」的那个；
    /// 都没有才退回第一个能解析出内容的。
    pub async fn fetch_lyric(&self, song: &Song) -> Result<Lyric> {
        let candidates = self.find_lyric_candidates(song).await?;
        if candidates.is_empty() {
            return Err(AppError::NotFound(format!("未找到《{}》的歌词", song.name)));
        }

        let mut fallback: Option<Lyric> = None;

        for (lyric_id, access_key) in candidates {
            let query = [
                ("id", lyric_id),
                ("accesskey", access_key),
                // 必须是 krc：翻译与音译只在 KRC 的 [language:] 标签里，lrc 没有。
                ("fmt", "krc".to_string()),
                ("decode", "true".to_string()),
                ("charset", "utf8".to_string()),
            ];

            // 该接口在 `decode=true` 下返回 JSON；偶尔直接吐纯文本，两种都接住。
            let body = match self.get_text("/lyric", &query).await {
                Ok(body) => body,
                Err(_) => continue, // 这个候选取不到，试下一个
            };
            let text = match serde_json::from_str::<Value>(&body) {
                Ok(root) => {
                    if crate::api::model::check_error_code("/lyric", &root).is_err() {
                        continue;
                    }
                    extract_lyric_text(&root)
                }
                Err(_) => body,
            };

            let mut lyric = parse_lrc(&text);
            if lyric.is_empty() {
                continue;
            }
            attach_translations(&mut lyric, &text);

            // 命中「有 CJK 译文」的候选，直接用它
            if translation_block_has_cjk(&text) {
                return Ok(lyric);
            }
            // 否则留作兜底（只留第一个，避免覆盖成更差的）
            if fallback.is_none() {
                fallback = Some(lyric);
            }
        }

        fallback.ok_or_else(|| AppError::NotFound(format!("《{}》的歌词为空", song.name)))
    }

    /// 第一步：按 hash 找到歌词候选，拿到若干 `(id, accesskey)`。
    ///
    /// `man=yes` 才会返回多个版本。上限 6 个：再往后质量通常更差，
    /// 而每多一个候选就多一次请求。
    async fn find_lyric_candidates(&self, song: &Song) -> Result<Vec<(String, String)>> {
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
                    ("man", "yes".to_string()),
                ],
            )
            .await?;

        let candidates = crate::api::extract_list(&root, &["candidates"], |value| {
            let id = pick_string(value, &["id", "lyric_id"])?;
            let access_key = pick_string(value, &["accesskey", "access_key"])?;
            Some((id, access_key))
        });

        Ok(candidates.into_iter().take(6).collect())
    }
}

/// `[language:]` 里是否存在**真正的 CJK 译文**块（而不是只有罗马音）。
///
/// 判定方式与桌面歌词脚本一致：逐块看有没有任一行含 CJK 字符。
/// 罗马音是纯拉丁，会被排除；中文译文含汉字，会命中。
fn translation_block_has_cjk(krc_text: &str) -> bool {
    let Some(payload) = extract_language_payload(krc_text) else {
        return false;
    };
    let Some(content) = payload.get("content").and_then(|value| value.as_array()) else {
        return false;
    };

    for block in content {
        let Some(items) = block.get("lyricContent").and_then(|value| value.as_array()) else {
            continue;
        };
        for entry in items {
            let Some(text) = flatten_lyric_content(entry) else {
                continue;
            };
            // CJK 统一表意文字 + 扩展 A 区
            if text.chars().any(|ch| matches!(ch, '\u{3400}'..='\u{9fff}')) {
                return true;
            }
        }
    }
    false
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

    // 各轨按 `type` 编号，但含义在不同歌里不固定（Bad Apple!! 的 type=1 是罗马音、
    // type=2 才是中文译文）。所以**不能**硬编码 type 取轨，要按「中文字符密度」动态挑——
    // 密度最高的是译文，最低的是音译/罗马音。
    //
    // `language` 字段同理不能用（实测两轨 language 都是 0），但跟我们的挑法无关。
    let mut tracks: Vec<(i64, String)> = Vec::new();
    for section in content {
        let Some(kind) = section.get("type").and_then(serde_json::Value::as_i64) else {
            continue;
        };
        let lines = section
            .get("lyricContent")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(flatten_lyric_content)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let joined: String = lines.join("\n");
        if !joined.trim().is_empty() {
            tracks.push((kind, joined));
        }
    }
    if tracks.is_empty() {
        return;
    }

    // 汉字（Han）占比。注意**汉字分不清中文和日文**——日文也用汉字。
    let hanzi_ratio = |text: &str| -> f64 {
        let (chars, hanzi) =
            text.chars()
                .filter(|ch| !ch.is_whitespace())
                .fold((0usize, 0usize), |(c, h), ch| {
                    (
                        c + 1,
                        h + if matches!(ch, '\u{4e00}'..='\u{9fff}') {
                            1
                        } else {
                            0
                        },
                    )
                });
        if chars == 0 {
            0.0
        } else {
            hanzi as f64 / chars as f64
        }
    };

    // 是否含假名（平假名 / 片假名）。有假名就说明这是**日文**，
    // 不能当中文译文——否则日文歌会把「日文原文」当成译文，显示出来跟原文重复。
    let has_kana =
        |text: &str| -> bool { text.chars().any(|ch| matches!(ch, '\u{3040}'..='\u{30ff}')) };

    // 判定「这是一条中文译文轨」：有汉字、且不含假名。
    // 密度阈值取 0.2：中文译文几乎全是汉字，日文原文因为夹杂大量假名通常低于此值，
    // 取宽松一点避免漏掉夹杂少量假名的译文（比如引用原句时）。
    let is_chinese_track = |text: &str| -> bool { !has_kana(text) && hanzi_ratio(text) >= 0.2 };

    // 排序：中文轨排最前（按汉字密度降序），其余（日文原文 / 罗马音）排后面。
    // 罗马音密度接近 0，自然落在最后，正好当音译。
    tracks.sort_by(|a, b| {
        let a_cn = is_chinese_track(&a.1);
        let b_cn = is_chinese_track(&b.1);
        match (a_cn, b_cn) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => hanzi_ratio(&b.1)
                .partial_cmp(&hanzi_ratio(&a.1))
                .unwrap_or(std::cmp::Ordering::Equal),
        }
    });

    // 有中文轨才把它当译文；否则留空，让界面回退显示音译（罗马音）——
    // 日语歌常常只有「日文原文 + 罗马音」两条轨，此时显示罗马音比重复原文有用得多，
    // 用户至少能跟着念。
    let chinese_count = tracks.iter().filter(|(_, t)| is_chinese_track(t)).count();

    // 译文位：只有存在中文轨时才填；没有就留空，界面会退回显示音译。
    // （用 first 是因为排序已把中文轨排到最前）
    let (translation_kind, translation_text) = if chinese_count > 0 {
        tracks.first().cloned().unwrap()
    } else {
        (i64::MAX, String::new())
    };

    // 音译位：取**汉字密度最低**的那条轨，也就是罗马音。
    //
    // 排序后中文在最前、其余按密度降序，所以最低密度的落在末尾。
    // 不能取第一条——没有中文轨时第一条是日文原文，而原文已经显示在上面了，
    // 再显示一遍毫无意义；罗马音至少能让人跟着念。
    let romanization = tracks
        .last()
        .cloned()
        .filter(|(kind, _)| *kind != translation_kind);

    // ---- 行号对齐（关键）----
    //
    // 各语言轨的 lyricContent 是按**原始 KRC 定时行**顺序排列的，而 parse_lrc 会
    // 跳过空内容的行（间奏之类的空行）。两边序号会错位：歌词第 5 行可能对应轨道的第 8 项。
    // 不对齐的话译文会取到空值，界面就退回去显示音译——这正是「翻译显示成音译」的根因。
    //
    // 所以这里复刻 parse_lrc 的筛选逻辑，算出每条保留下来的歌词行在定时行中的真实序号。
    let mut ordinals: Vec<usize> = Vec::new();
    let mut timed_seen = 0usize;
    for raw in krc_text.lines() {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        let (timestamps, remainder) = consume_time_tags(line);
        if timestamps.is_empty() {
            continue; // 元信息行（[id:] [ti:] [language:] 等）
        }
        let ordinal = timed_seen;
        timed_seen += 1;
        if !clean_krc_markup(remainder).is_empty() {
            ordinals.push(ordinal);
        }
    }

    // 预先切好行，避免在内层循环里反复 split（也顺带解决借用/move 的麻烦）
    let translation_lines: Vec<&str> = translation_text.lines().collect();
    // 先把音译轨的整段文本取出来（拥有所有权），再按行切片，
    // 否则引用的是闭包里的临时变量，编译不过
    let romanization_text: Option<String> = match romanization {
        Some((other_kind, other_text)) if other_kind != translation_kind => Some(other_text),
        _ => None,
    };
    let romanization_lines: Vec<&str> = romanization_text
        .as_deref()
        .map(|text| text.lines().collect())
        .unwrap_or_default();

    for (index, line) in lyric.lines.iter_mut().enumerate() {
        // 歌词行 → 它在定时行中的序号 → 再到轨道里取对应项
        let source = ordinals.get(index).copied().unwrap_or(index);
        if let Some(text) = translation_lines.get(source) {
            if !text.trim().is_empty() {
                line.translation = Some((*text).to_string());
            }
        }
        if let Some(text) = romanization_lines.get(source) {
            if !text.trim().is_empty() {
                line.romanization = Some((*text).to_string());
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
    /// 日语歌：只有「日文原文 + 罗马音」两条轨，没有中文译文时的行为。
    ///
    /// 锁住两点。
    ///
    /// 一是**不能把日文原文当成译文**：日文也用汉字，光看汉字密度会误判，
    /// 必须靠「含假名」把日文排除掉。
    ///
    /// 二是没有中文轨时译文留空、音译位放**罗马音**（密度最低那条），
    /// 而不是再显示一遍日文原文——原文已经在上面显示过了，重复毫无意义。
    #[test]
    fn japanese_song_without_chinese_uses_romaji() {
        use base64::Engine;

        let payload = r#"{"content":[
            {"language":0,"type":0,"lyricContent":[["","na ga re te ku to ki no na ka de de mo"]]},
            {"language":0,"type":1,"lyricContent":[["","流れてく時の中ででも"]]}
        ]}"#;
        let encoded = base64::engine::general_purpose::STANDARD.encode(payload);
        let krc = format!("[id:1]\n[language:{encoded}]\n[0,1000]流れてく時の中ででも\n");

        let mut lyric = parse_lrc(&krc);
        attach_translations(&mut lyric, &krc);

        assert_eq!(lyric.lines.len(), 1);
        // 含假名 → 不算中文译文，译文留空
        assert_eq!(
            lyric.lines[0].translation, None,
            "日文原文（含假名）不能被当成中文译文"
        );
        // 音译位应是罗马音，不是日文原文
        assert_eq!(
            lyric.lines[0].romanization.as_deref(),
            Some("na ga re te ku to ki no na ka de de mo"),
            "没有中文轨时，音译位应放罗马音"
        );
    }

    #[test]
    fn attaches_translation_and_romanization() {
        use base64::Engine;

        let payload = r#"{"content":[
            {"language":0,"type":1,"lyricContent":[["","no mi ko"]]},
            {"language":0,"type":2,"lyricContent":[["","就算身处流逝的时光里"]]}
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
            "中文密度高的轨应被选为译文（Bad Apple!! 是 type=2）"
        );
        assert_eq!(
            lyric.lines[0].romanization.as_deref(),
            Some("no mi ko"),
            "拉丁字符多的轨应被选为音译（Bad Apple!! 是 type=1）"
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
