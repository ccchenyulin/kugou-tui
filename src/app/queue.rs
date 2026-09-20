//! 播放队列与播放模式。
//!
//! 队列是「当前播放上下文」的唯一真相来源：用户从搜索结果点歌时，会把整个搜索结果
//! 灌进队列并定位到选中项，这样才能自然地「下一首」。视图自身的列表只用于展示。

use serde::{Deserialize, Serialize};

use crate::api::model::Song;
use crate::util::random_below;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackMode {
    /// 顺序播放，播到末尾停止。
    #[default]
    Sequential,
    /// 列表循环。
    RepeatAll,
    /// 单曲循环。
    RepeatOne,
    /// 随机播放。
    Shuffle,
}

impl PlaybackMode {
    /// 按 `r` 键时的轮转顺序。
    pub fn next(self) -> Self {
        match self {
            Self::Sequential => Self::RepeatAll,
            Self::RepeatAll => Self::RepeatOne,
            Self::RepeatOne => Self::Shuffle,
            Self::Shuffle => Self::Sequential,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sequential => "顺序",
            Self::RepeatAll => "列表循环",
            Self::RepeatOne => "单曲循环",
            Self::Shuffle => "随机",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlayQueue {
    items: Vec<Song>,
    /// `None` 表示尚未开始播放。
    cursor: Option<usize>,
    mode: PlaybackMode,
}

impl PlayQueue {
    pub fn new(mode: PlaybackMode) -> Self {
        Self {
            items: Vec::new(),
            cursor: None,
            mode,
        }
    }

    pub fn items(&self) -> &[Song] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn mode(&self) -> PlaybackMode {
        self.mode
    }

    pub fn cycle_mode(&mut self) -> PlaybackMode {
        self.mode = self.mode.next();
        self.mode
    }

    pub fn current(&self) -> Option<&Song> {
        self.cursor.and_then(|index| self.items.get(index))
    }

    /// 用一批歌曲替换整个队列，并把游标定位到 `index`。
    ///
    /// 用户从任意列表点歌都走这条路径，保证「下一首」的语义符合直觉。
    pub fn replace_with(&mut self, songs: Vec<Song>, index: usize) -> Option<&Song> {
        if songs.is_empty() {
            self.items.clear();
            self.cursor = None;
            return None;
        }
        self.items = songs;
        self.cursor = Some(index.min(self.items.len() - 1));
        self.current()
    }

    /// 追加到队尾。若队列此前为空，游标指向新元素。
    pub fn append(&mut self, song: Song) -> usize {
        self.items.push(song);
        let index = self.items.len() - 1;
        if self.cursor.is_none() {
            self.cursor = Some(index);
        }
        index
    }

    /// 一次性追加多首到队尾。
    ///
    /// 预留容量后批量搬移。这是给「把整个歌单加入队列」用的：逐首 `append` 时每一首都可能触发
    /// Vec 扩容重分配；400 首实测差别很小，但 `reserve` 零成本，也把「批量」的意图写清楚。
    /// 返回队尾下标（供调用方提示）。
    pub fn append_all(&mut self, songs: Vec<Song>) -> usize {
        if songs.is_empty() {
            return self.items.len();
        }
        self.items.reserve(songs.len());
        self.items.extend(songs);
        if self.cursor.is_none() {
            self.cursor = Some(0);
        }
        self.items.len() - 1
    }

    /// 插入到当前曲目之后（`i` 键：下一首播放）。
    pub fn insert_next(&mut self, song: Song) -> usize {
        let position = match self.cursor {
            Some(index) => index + 1,
            None => 0,
        };
        let position = position.min(self.items.len());
        self.items.insert(position, song);
        if self.cursor.is_none() {
            self.cursor = Some(position);
        }
        position
    }

    /// 从队列中移除指定下标，并修正游标。
    pub fn remove(&mut self, index: usize) -> Option<Song> {
        if index >= self.items.len() {
            return None;
        }
        let removed = self.items.remove(index);
        self.cursor = match self.cursor {
            None => None,
            // 移除最后一项后队列为空，游标一并清掉
            Some(_) if self.items.is_empty() => None,
            Some(cursor) if cursor > index => Some(cursor - 1),
            Some(cursor) => Some(cursor.min(self.items.len() - 1)),
        };
        Some(removed)
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.cursor = None;
    }

    /// 直接定位到某首。
    pub fn jump_to(&mut self, index: usize) -> Option<&Song> {
        if index >= self.items.len() {
            return None;
        }
        self.cursor = Some(index);
        self.current()
    }

    /// 前进到下一首。
    ///
    /// * `triggered_by_user` —— 区分「用户按了 n」与「当前曲目自然播完」。
    ///   单曲循环只在自然播完时原地循环，用户按 `n` 仍然应该换歌。
    ///
    /// 返回 `None` 表示队列已走到尽头（顺序播放到底），调用方应停止播放。
    pub fn advance(&mut self, triggered_by_user: bool) -> Option<usize> {
        if self.items.is_empty() {
            self.cursor = None;
            return None;
        }
        let current = self.cursor.unwrap_or(0);
        let total = self.items.len();

        let next = match self.mode {
            PlaybackMode::RepeatOne if !triggered_by_user => current,
            PlaybackMode::Shuffle => {
                if total == 1 {
                    0
                } else {
                    // 重新抽一次以避免原地打转
                    let candidate = random_below(total);
                    if candidate == current {
                        (candidate + 1) % total
                    } else {
                        candidate
                    }
                }
            }
            _ => {
                let candidate = current + 1;
                if candidate >= total {
                    if self.mode == PlaybackMode::Sequential && !triggered_by_user {
                        return None;
                    }
                    0
                } else {
                    candidate
                }
            }
        };

        self.cursor = Some(next);
        Some(next)
    }

    /// 后退到上一首。
    pub fn retreat(&mut self) -> Option<usize> {
        if self.items.is_empty() {
            self.cursor = None;
            return None;
        }
        let current = self.cursor.unwrap_or(0);
        let total = self.items.len();

        let previous = match self.mode {
            PlaybackMode::Shuffle => random_below(total),
            _ => {
                if current == 0 {
                    total - 1
                } else {
                    current - 1
                }
            }
        };

        self.cursor = Some(previous);
        Some(previous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(hash: &str) -> Song {
        Song {
            name: hash.to_string(),
            hash: hash.to_string(),
            ..Song::default()
        }
    }

    fn queue_with(mode: PlaybackMode, count: usize) -> PlayQueue {
        let mut queue = PlayQueue::new(mode);
        let songs: Vec<Song> = (0..count).map(|index| song(&format!("h{index}"))).collect();
        queue.replace_with(songs, 0);
        queue
    }

    #[test]
    fn sequential_stops_at_end_when_finished_naturally() {
        let mut queue = queue_with(PlaybackMode::Sequential, 3);
        assert_eq!(queue.advance(false), Some(1));
        assert_eq!(queue.advance(false), Some(2));
        assert_eq!(queue.advance(false), None, "顺序播放到底应停止");
    }

    #[test]
    fn sequential_wraps_when_user_presses_next() {
        let mut queue = queue_with(PlaybackMode::Sequential, 2);
        assert_eq!(queue.advance(true), Some(1));
        assert_eq!(queue.advance(true), Some(0));
    }

    #[test]
    fn repeat_one_only_loops_on_natural_end() {
        let mut queue = queue_with(PlaybackMode::RepeatOne, 3);
        assert_eq!(queue.advance(false), Some(0), "自然播完应原地循环");
        assert_eq!(queue.advance(true), Some(1), "用户按下一首应换歌");
    }

    #[test]
    fn append_all_preserves_order() {
        let mut queue = PlayQueue::new(PlaybackMode::Sequential);
        let songs: Vec<Song> = (0..3).map(|index| song(&format!("h{index}"))).collect();
        queue.append_all(songs);
        let hashes: Vec<&str> = queue
            .items()
            .iter()
            .map(|song| song.hash.as_str())
            .collect();
        assert_eq!(hashes, vec!["h0", "h1", "h2"]);
        assert_eq!(queue.cursor, Some(0), "空队列加入后游标应指向第一首");
    }

    #[test]
    fn append_all_appends_after_existing_items() {
        let mut queue = queue_with(PlaybackMode::Sequential, 2);
        queue.append_all(vec![song("x"), song("y")]);
        assert_eq!(queue.len(), 4);
        assert_eq!(queue.items()[2].hash, "x");
        assert_eq!(queue.items()[3].hash, "y");
        assert_eq!(queue.cursor, Some(0), "原有游标不应被改动");
    }

    #[test]
    fn append_all_with_empty_input_is_noop() {
        let mut queue = queue_with(PlaybackMode::Sequential, 2);
        queue.append_all(Vec::new());
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn clear_resets_cursor_and_current() {
        let mut queue = queue_with(PlaybackMode::Sequential, 3);
        queue.jump_to(1);
        queue.clear();
        assert!(queue.is_empty());
        assert_eq!(queue.cursor, None);
        assert!(queue.current().is_none());
    }
    #[test]
    fn insert_next_places_after_cursor() {
        let mut queue = queue_with(PlaybackMode::Sequential, 3);
        queue.jump_to(1);
        queue.insert_next(song("inserted"));
        assert_eq!(queue.items()[2].hash, "inserted");
        assert_eq!(queue.current().map(|s| s.hash.as_str()), Some("h1"));
    }

    #[test]
    fn remove_keeps_cursor_pointing_at_same_song() {
        let mut queue = queue_with(PlaybackMode::Sequential, 3);
        queue.jump_to(2);
        queue.remove(0);
        assert_eq!(queue.current().map(|s| s.hash.as_str()), Some("h2"));
    }

    #[test]
    fn remove_last_item_clears_cursor() {
        let mut queue = queue_with(PlaybackMode::Sequential, 1);
        queue.remove(0);
        assert_eq!(queue.current().map(|s| s.hash.as_str()), None);
        assert!(queue.is_empty());
    }

    #[test]
    fn mode_cycles_through_all_variants() {
        let mut mode = PlaybackMode::Sequential;
        let mut seen = vec![mode];
        for _ in 0..3 {
            mode = mode.next();
            seen.push(mode);
        }
        assert_eq!(mode.next(), PlaybackMode::Sequential);
        assert_eq!(seen.len(), 4);
    }
}
