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
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::{AsyncSeekExt, AsyncWriteExt, SeekFrom};

use crate::error::{AppError, Result};
use crate::logger::tlog;

/// 走分块并发的门槛。小文件并发反而慢（每次分块都要建一次连接），不值得。
const PARALLEL_MIN_BYTES: u64 = 512 * 1024;
/// 最多分几块。再多 CDN 也未必给更快，还容易触发限流。
const PARALLEL_MAX_CHUNKS: u64 = 4;
/// 单块大小的下限，避免把一首歌切成几十个碎片。
const MIN_CHUNK_BYTES: u64 = 256 * 1024;

/// 一段 Range 请求要下载的范围（闭区间，`end` 是最后一个字节的下标）。
#[derive(Debug, Clone, Copy)]
struct ChunkRange {
    start: u64,
    end: u64,
}

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
        // 先问一次 HEAD：拿到文件长度和「是否支持 Range」。
        // 支持并发就并发——一首 8 MB 的歌单连接爬要十几秒，分 4 块通常能砍到
        // 三分之一；不支持（或文件太小）就老老实实单连接，别为省几秒把
        // 兼容性搞坏。
        if let Some(total) = self.probe_parallel(url).await {
            let chunks = Self::split_into_chunks(total);
            if chunks.len() > 1 {
                match self
                    .fetch_parallel(url, target, total, chunks, progress)
                    .await
                {
                    Ok(bytes) => return Ok(bytes),
                    Err(error) => {
                        // 并发失败（比如 CDN 中途变卦）就退回单连接，
                        // 不要让用户因为一次优化尝试而播不了歌。
                        tlog!(
                            crate::logger::LEVEL_WARN,
                            "分块下载失败，退回单连接：{}",
                            error.user_hint()
                        );
                    }
                }
            }
        }

        self.fetch_sequential(url, target, progress).await
    }

    /// 问服务端「这个文件多大、支不支持分段下载」。两者都满足才返回长度。
    async fn probe_parallel(&self, url: &str) -> Option<u64> {
        let response = self.http.head(url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        let accepts_ranges = response
            .headers()
            .get(reqwest::header::ACCEPT_RANGES)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("bytes"));
        if !accepts_ranges {
            return None;
        }
        let total = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())?;
        (total >= PARALLEL_MIN_BYTES).then_some(total)
    }

    /// 把 `total` 字节切成若干块。块数受 [`PARALLEL_MAX_CHUNKS`] 和
    /// [`MIN_CHUNK_BYTES`] 双重约束，避免「4 块每块才 8 KB」这种极端。
    fn split_into_chunks(total: u64) -> Vec<ChunkRange> {
        let mut count = (total / MIN_CHUNK_BYTES).clamp(1, PARALLEL_MAX_CHUNKS);
        if count <= 1 {
            return vec![ChunkRange {
                start: 0,
                end: total.saturating_sub(1),
            }];
        }
        // 让每块不小于 MIN_CHUNK_BYTES，最后一块吃掉余数
        count = count.min(PARALLEL_MAX_CHUNKS);
        let size = total / count;
        (0..count)
            .map(|index| {
                let start = index * size;
                let end = if index == count - 1 {
                    total - 1
                } else {
                    start + size - 1
                };
                ChunkRange { start, end }
            })
            .collect()
    }

    /// 分块并发下载：每块一个 Range 请求，各自 seek 到自己的偏移写入同一个临时文件。
    async fn fetch_parallel(
        &self,
        url: &str,
        target: &Path,
        total: u64,
        chunks: Vec<ChunkRange>,
        progress: ProgressFn<'_>,
    ) -> Result<u64> {
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| AppError::io_at(parent.display().to_string(), error))?;
        }
        let temp_path = temp_path_for(target);
        // 预分配：让各块能并发 seek 到任意偏移而不互相踩踏
        let file = tokio::fs::File::create(&temp_path)
            .await
            .map_err(|error| AppError::io_at(temp_path.display().to_string(), error))?;
        file.set_len(total)
            .await
            .map_err(|error| AppError::io_at(temp_path.display().to_string(), error))?;
        drop(file);

        let received = Arc::new(AtomicU64::new(0));
        let mut handles = Vec::with_capacity(chunks.len());
        for range in chunks {
            let url = url.to_string();
            let http = self.http.clone();
            let path = temp_path.clone();
            let received = received.clone();
            handles.push(tokio::spawn(async move {
                let mut file = tokio::fs::File::options()
                    .write(true)
                    .open(&path)
                    .await
                    .map_err(|error| AppError::io_at(path.display().to_string(), error))?;
                file.seek(SeekFrom::Start(range.start))
                    .await
                    .map_err(|error| AppError::io_at(path.display().to_string(), error))?;

                let mut response = http
                    .get(&url)
                    .header(
                        reqwest::header::RANGE,
                        format!("bytes={}-{}", range.start, range.end),
                    )
                    .send()
                    .await?;
                let status = response.status();
                if !status.is_success() {
                    return Err(AppError::HttpStatus {
                        path: url.to_string(),
                        status: status.as_u16(),
                    });
                }

                let mut written = 0u64;
                while let Some(chunk) = response.chunk().await? {
                    file.write_all(&chunk)
                        .await
                        .map_err(|error| AppError::io_at(path.display().to_string(), error))?;
                    written += chunk.len() as u64;
                    received.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                }
                file.flush()
                    .await
                    .map_err(|error| AppError::io_at(path.display().to_string(), error))?;
                Ok::<u64, AppError>(written)
            }));
        }

        // 一边等一边汇报进度：各块的字节数汇总到 `received`
        let mut done = false;
        while !done {
            done = true;
            for handle in handles.iter_mut() {
                if !handle.is_finished() {
                    done = false;
                    break;
                }
            }
            progress(received.load(Ordering::Relaxed), Some(total));
            if !done {
                tokio::time::sleep(std::time::Duration::from_millis(120)).await;
            }
        }

        let mut total_written = 0u64;
        for handle in handles {
            match handle.await {
                Ok(Ok(bytes)) => total_written += bytes,
                Ok(Err(error)) => {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return Err(error);
                }
                Err(error) => {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return Err(AppError::Audio(format!("下载任务异常：{error}")));
                }
            }
        }

        if total_written == 0 {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(AppError::Audio(format!(
                "从 {url} 下载到的内容为空，可能该歌曲需要 VIP 或已下架"
            )));
        }

        tokio::fs::rename(&temp_path, target)
            .await
            .map_err(|error| AppError::io_at(target.display().to_string(), error))?;
        progress(total_written, Some(total));
        Ok(total_written)
    }

    /// 单连接顺序下载（原来的实现）。分块不可用时退回这条路径。
    async fn fetch_sequential(
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

    /// 把一个小文件整个读进内存（封面图用，不落盘）。
    ///
    /// 封面每张几十 KB，没必要为它建一套磁盘缓存；而且它和音频缓存的回收策略
    /// 也不一样（按修改时间删旧的，会把正在看的封面删掉）。
    pub async fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.http.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(AppError::HttpStatus {
                path: url.to_string(),
                status: status.as_u16(),
            });
        }
        let bytes = response.bytes().await.map_err(AppError::Http)?;
        Ok(bytes.to_vec())
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

    /// 小文件不分块——一次分块要建一次连接，小文件并发反而是负优化。
    #[test]
    fn small_file_is_never_split() {
        let chunks = Downloader::split_into_chunks(100 * 1024);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].start, 0);
        assert_eq!(chunks[0].end, 100 * 1024 - 1);
    }

    /// 大文件分块：各块首尾相接、整首歌一个字节都不能漏或多。
    #[test]
    fn chunks_cover_the_file_exactly_once() {
        let total = 8 * 1024 * 1024;
        let chunks = Downloader::split_into_chunks(total);
        assert!(chunks.len() > 1, "8 MB 应当分块");
        assert!(chunks.len() <= PARALLEL_MAX_CHUNKS as usize);

        // 首尾相接
        assert_eq!(chunks[0].start, 0);
        for pair in chunks.windows(2) {
            assert_eq!(pair[0].end + 1, pair[1].start, "块之间不能有缝");
        }
        // 最后一块正好到文件末尾
        assert_eq!(chunks.last().unwrap().end, total - 1);
        // 每块都不小于下限
        for range in &chunks {
            assert!(
                range.end - range.start + 1 >= MIN_CHUNK_BYTES,
                "块太小：{range:?}"
            );
        }
    }
}
