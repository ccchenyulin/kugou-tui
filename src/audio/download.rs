//! 音频流下载。
//!
//! # 为什么先落盘再播，而不是边下边播
//!
//! rodio 的解码器要求 `Read + Seek`，也就是说必须能对整段音频做随机访问。
//! 「边下边播」要么把整首歌缓存在内存里（内存占用不可控），要么自己实现一个
//! 分块缓冲 + seek 的 `Read`（复杂度高、边界情况多）。
//!
//! 落盘下载换来的好处是实打实的：
//!
//! * 常驻内存只有解码缓冲（几百 KB），与歌曲码率无关；
//! * 拖进度条是真正的随机访问，没有「跳不过去」的区域；
//! * 重复播放零网络开销；
//! * 断点续传、失败重试都变成简单的文件操作。
//!
//! 代价是首播要等下载完成。为此这里把下载进度回报给界面，让用户看到明确反馈。

use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;

use crate::error::{AppError, Result};

/// 下载过程中的进度回调：`(已下载字节, 总字节)`。总字节在服务端不给
/// `Content-Length` 时为 `None`。
pub type ProgressFn<'a> = &'a (dyn Fn(u64, Option<u64>) + Send + Sync);

/// 音频下载器。
///
/// 与 [`crate::api::ApiClient`] 分开，因为直链指向 CDN 而不是 KuGouMusicApi，
/// 既不需要带 cookie，超时策略也不一样（CDN 大文件要宽松得多）。
#[derive(Debug, Clone)]
pub struct Downloader {
    http: reqwest::Client,
}

impl Downloader {
    pub fn new(proxy: Option<&str>) -> Result<Self> {
        let mut builder = reqwest::Client::builder()
            // 整首歌可能几十 MB，超时给足
            .timeout(std::time::Duration::from_secs(180))
            .connect_timeout(std::time::Duration::from_secs(8))
            .user_agent(concat!("kugou-tui/", env!("CARGO_PKG_VERSION")))
            .pool_max_idle_per_host(2);

        if let Some(proxy_url) = proxy.map(str::trim).filter(|url| !url.is_empty()) {
            let parsed = reqwest::Proxy::all(proxy_url)
                .map_err(|error| AppError::Config(format!("代理地址 {proxy_url} 无效：{error}")))?;
            builder = builder.proxy(parsed);
        }

        let http = builder
            .build()
            .map_err(|error| AppError::Config(format!("构造下载客户端失败：{error}")))?;

        Ok(Self { http })
    }

    /// 把 `url` 下载到 `target`。
    ///
    /// 先写同目录下的 `.part` 临时文件，全部写完再原子重命名。这样即使中途被杀
    /// 或断网，也不会留下一个「看起来完整、实际损坏」的缓存文件被后续播放命中。
    ///
    /// 返回写入的字节数。
    pub async fn fetch_to(
        &self,
        url: &str,
        target: &Path,
        progress: ProgressFn<'_>,
    ) -> Result<u64> {
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| AppError::io_at(parent.display().to_string(), error))?;
        }

        let mut response = self.http.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(AppError::HttpStatus {
                path: url.to_string(),
                status: status.as_u16(),
            });
        }

        let total_bytes = response.content_length();
        let temp_path = temp_path_for(target);
        let mut file = tokio::fs::File::create(&temp_path)
            .await
            .map_err(|error| AppError::io_at(temp_path.display().to_string(), error))?;

        let mut written: u64 = 0;
        let result = async {
            while let Some(chunk) = response.chunk().await? {
                file.write_all(&chunk)
                    .await
                    .map_err(|error| AppError::io_at(temp_path.display().to_string(), error))?;
                written += chunk.len() as u64;
                progress(written, total_bytes);
            }
            file.flush()
                .await
                .map_err(|error| AppError::io_at(temp_path.display().to_string(), error))?;
            file.sync_all()
                .await
                .map_err(|error| AppError::io_at(temp_path.display().to_string(), error))?;
            Ok::<u64, AppError>(written)
        }
        .await;

        drop(file);

        match result {
            Ok(bytes) if bytes > 0 => {
                tokio::fs::rename(&temp_path, target)
                    .await
                    .map_err(|error| AppError::io_at(target.display().to_string(), error))?;
                Ok(bytes)
            }
            Ok(_) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                Err(AppError::Audio(format!(
                    "从 {url} 下载到的内容为空，可能该歌曲需要 VIP 或已下架"
                )))
            }
            Err(error) => {
                // 清理半成品，避免污染缓存
                let _ = tokio::fs::remove_file(&temp_path).await;
                Err(error)
            }
        }
    }

    /// 根据直链推断音频容器扩展名。
    ///
    /// 酷狗的直链形如 `http://xxx/yyy.mp3?token=...`，扩展名在路径段里，
    /// 所以要先把 query 和 fragment 去掉再取后缀。
    pub fn extension_from_url(url: &str) -> &'static str {
        let without_fragment = url.split('#').next().unwrap_or(url);
        let without_query = without_fragment
            .split('?')
            .next()
            .unwrap_or(without_fragment);
        let extension = without_query
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();

        match extension.as_str() {
            "mp3" => "mp3",
            "flac" => "flac",
            "m4a" | "mp4" => "m4a",
            "aac" => "aac",
            "ogg" | "oga" => "ogg",
            "wav" => "wav",
            "ape" => "ape",
            // 推断不出来时按 mp3 存：rodio 走的是内容探测，扩展名只影响缓存查找
            _ => "mp3",
        }
    }
}

/// `song.mp3` → `song.mp3.part`
fn temp_path_for(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    target.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_extension_ignoring_query_string() {
        assert_eq!(
            Downloader::extension_from_url("http://cdn.kugou.com/a/b/abc.mp3?token=xyz&x=1"),
            "mp3"
        );
        assert_eq!(Downloader::extension_from_url("https://x/y.flac"), "flac");
        assert_eq!(
            Downloader::extension_from_url("https://x/y.m4a#frag"),
            "m4a"
        );
    }

    #[test]
    fn falls_back_to_mp3_for_unknown_extension() {
        assert_eq!(Downloader::extension_from_url("http://x/stream"), "mp3");
        assert_eq!(Downloader::extension_from_url("http://x/y.weird"), "mp3");
    }

    #[test]
    fn builds_part_file_beside_target() {
        let path = temp_path_for(Path::new("/tmp/cache/abc-128.mp3"));
        assert_eq!(path, PathBuf::from("/tmp/cache/abc-128.mp3.part"));
    }
}
