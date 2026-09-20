//! KuGouMusicApi 客户端与接口封装。
//!
//! 分层：
//!
//! ```text
//! client.rs   —— 只管 HTTP：拼 URL、带 cookie、取响应体
//! model.rs    —— 领域模型 + 把酷狗原始 JSON 归一成领域模型
//! catalog.rs  —— 目录类接口：搜索 / 歌单 / 歌手 / 排行榜 / 播放直链
//! lyric.rs    —— 歌词：搜歌词拿 (id, accesskey) → 取正文 → 解析 LRC
//! cloud.rs    —— 登录态相关：设备指纹 / 云端歌单增删
//! ```
//!
//! 接口路径全部对照 <https://github.com/MakcRe/KuGouMusicApi> 的 `docs/README.md` 核对过。

mod catalog;
mod client;
/// 登录与云端歌单写操作。
pub mod cloud;
mod lyric;
/// 领域模型要对 crate 内其它层可见（`app` / `ui` 都要用 `Song` 等类型）。
pub mod model;

/// 客户端是这一层唯一对外暴露的类型。
///
/// 领域模型不在这里 re-export：各层直接从 `crate::api::model` 取，
/// 少一层转发，`use` 语句也更能说明「这个东西从哪来」。
pub use client::ApiClient;

use serde_json::Value;

/// 取出响应里的 `data` 段。
///
/// 少数接口把结果直接放在顶层，此时回落到 `root` 本身，让后续的候选键扫描
/// 仍然有机会命中，而不是直接判空。
pub(crate) fn data_of(root: &Value) -> &Value {
    root.get("data").unwrap_or(root)
}

/// 在 JSON 树里按 `key` 找第一个数组，深度限制 5 层。
fn find_array_by_key<'a>(value: &'a Value, key: &str, depth: usize) -> Option<&'a Vec<Value>> {
    if depth > 5 {
        return None;
    }
    match value {
        Value::Object(map) => {
            if let Some(found) = map.get(key).and_then(Value::as_array) {
                return Some(found);
            }
            map.values()
                .find_map(|child| find_array_by_key(child, key, depth + 1))
        }
        Value::Array(items) => items
            .iter()
            .find_map(|child| find_array_by_key(child, key, depth + 1)),
        _ => None,
    }
}

/// 递归收集所有「元素是对象」的数组。
fn collect_object_arrays<'a>(value: &'a Value, out: &mut Vec<&'a Vec<Value>>, depth: usize) {
    if depth > 5 {
        return;
    }
    match value {
        Value::Array(items) => {
            if items.iter().any(Value::is_object) {
                out.push(items);
            }
            for child in items {
                collect_object_arrays(child, out, depth + 1);
            }
        }
        Value::Object(map) => {
            for child in map.values() {
                collect_object_arrays(child, out, depth + 1);
            }
        }
        _ => {}
    }
}

/// 从响应里提取一组对象。
///
/// 策略分两级：
///
/// 1. 按 `preferred_keys` 依次找数组——覆盖该接口的已知键名；
/// 2. 全部落空时，扫描整棵 JSON 树，取第一个「能解析出非空结果」的对象数组。
///
/// 第二级是兜底：即使酷狗改了字段布局，界面也只是少了某些条目，而不是整页空白。
pub(crate) fn extract_list<T>(
    root: &Value,
    preferred_keys: &[&str],
    parse: impl Fn(&Value) -> Option<T>,
) -> Vec<T> {
    for key in preferred_keys {
        let Some(array) = find_array_by_key(root, key, 0) else {
            continue;
        };
        let parsed: Vec<T> = array.iter().filter_map(&parse).collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }

    let mut arrays = Vec::new();
    collect_object_arrays(root, &mut arrays, 0);
    for array in arrays {
        let parsed: Vec<T> = array.iter().filter_map(&parse).collect();
        if !parsed.is_empty() {
            return parsed;
        }
    }

    Vec::new()
}

/// 从响应里提取单个对象，用于详情类接口。
pub(crate) fn extract_first<T>(root: &Value, parse: impl Fn(&Value) -> Option<T>) -> Option<T> {
    // `data` 是数组时取第一个能解析的元素
    if let Some(items) = root.get("data").and_then(Value::as_array) {
        if let Some(found) = items.iter().find_map(&parse) {
            return Some(found);
        }
    }

    let mut arrays = Vec::new();
    collect_object_arrays(root, &mut arrays, 0);
    for array in arrays {
        if let Some(found) = array.iter().find_map(&parse) {
            return Some(found);
        }
    }

    // 最后把 `data` 本身当作对象试一次
    root.get("data").and_then(&parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_list_prefers_named_key() {
        let root = json!({
            "data": {
                "info": [{"name": "歌单A"}],
                "noise": [{"name": "不该被选中"}]
            }
        });
        let names: Vec<String> = extract_list(&root, &["info"], |value| {
            value.get("name")?.as_str().map(str::to_string)
        });
        assert_eq!(names, vec!["歌单A"]);
    }

    #[test]
    fn extract_list_falls_back_to_tree_scan() {
        // 键名全变了，兜底扫描仍应命中
        let root = json!({
            "data": {
                "unexpected_container": {
                    "deeply": {"nested": [{"name": "歌单B"}]}
                }
            }
        });
        let names: Vec<String> = extract_list(&root, &["info", "list"], |value| {
            value.get("name")?.as_str().map(str::to_string)
        });
        assert_eq!(names, vec!["歌单B"]);
    }

    #[test]
    fn extract_first_handles_array_payload() {
        let root = json!({"data": [{"id": 1, "name": "x"}, {"id": 2, "name": "y"}]});
        let first = extract_first(&root, |value| value.get("id")?.as_i64());
        assert_eq!(first, Some(1));
    }

    #[test]
    fn data_of_falls_back_to_root() {
        let root = json!({"lists": []});
        assert!(data_of(&root).get("lists").is_some());
    }
}
