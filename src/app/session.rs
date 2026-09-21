//! 退出时把「正在听什么」存下来，下次启动原样恢复。
//!
//! 关掉终端再打开，队列和进度都不该丢——这是播放器该有的基本行为。
//! 存在 `~/.cache/kugou-tui/session.json`，和配置文件分开：配置是用户手改的
//! 长期设置，会话是程序自己写的瞬时状态，混在一起会互相干扰（比如用户改了
//! 配置却被程序覆写）。

use serde::{Deserialize, Serialize};

use crate::api::model::Song;

/// 一个可恢复的播放会话。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Session {
    /// 队列里的全部歌曲，按顺序。
    pub queue: Vec<Song>,
    /// 队列游标：当前停在哪个位置。
    pub cursor: Option<usize>,
    /// 当前这首歌播到几毫秒。
    pub position_ms: u64,
}

impl Session {
    /// 会话文件的位置。
    pub fn path() -> std::path::PathBuf {
        crate::config::default_cache_dir().join("session.json")
    }

    /// 读上次会话。文件不存在、损坏、字段缺失都当作「没有上次」——会话丢了
    /// 只是少恢复一次，不该让程序起不来。
    pub fn load() -> Option<Self> {
        let text = std::fs::read_to_string(Self::path()).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// 写会话。失败只记日志：存不下会话不该影响退出。
    pub fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                crate::logger::tlog!(crate::logger::LEVEL_WARN, "创建会话目录失败：{error}");
                return;
            }
        }
        match serde_json::to_string(self) {
            Ok(text) => {
                if let Err(error) = std::fs::write(&path, text) {
                    crate::logger::tlog!(crate::logger::LEVEL_WARN, "保存会话失败：{error}");
                }
            }
            Err(error) => {
                crate::logger::tlog!(crate::logger::LEVEL_WARN, "序列化会话失败：{error}")
            }
        }
    }

    /// 这个会话有没有值得恢复的东西。
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
