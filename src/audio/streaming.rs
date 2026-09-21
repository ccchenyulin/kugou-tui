//! 流式音频缓冲：边下边播的底座。
//!
//! # 为什么需要它
//!
//! `rodio::Decoder<R>` 要求 `R: Read + Seek`（见 docs.rs，rodio 0.22 没有
//! `new_stream`）。网络流只能 `Read` 不能 `Seek`，所以不能直接喂给 rodio。
//!
//! 这里在内存里攒一个**会增长**的字节缓冲区，并且让它实现 `Seek`：
//! 读指针还没下载到的地方就**阻塞等待**后台下载线程补数据。Decoder 因此
//! 以为自己在读一个"很大的本地文件"，实际数据还在路上。
//!
//! # 阻塞的代价
//!
//! `read()` 阻塞时 rodio 的播放线程就停在那儿——表现是**声音卡住**（不是结束）。
//! 所以每个等待都带超时：网络真的断了要能报错退出，不能永远卡死。

use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// 等数据的最长时间。超过就当作下载失败，让上层报错而不是无限卡住。
const WAIT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Default)]
struct Inner {
    /// 已下载到的字节。只会追加，不会缩短。
    data: Vec<u8>,
    /// 服务端给的总长度（`Content-Length`），拿不到就是 None。
    total: Option<u64>,
    /// 下载是否结束（成功或失败都算结束——失败时上层会把播放器停掉）。
    done: bool,
    /// 下载线程填进来的错误信息。
    error: Option<String>,
}

/// 一边下载一边增长的音频缓冲。
///
/// 克隆出来的实例共享同一份数据：下载任务持有一份写，播放线程持有一份读。
#[derive(Clone, Debug)]
pub struct StreamingBuffer {
    inner: Arc<(Mutex<Inner>, Condvar)>,
    /// 读指针。每个克隆有自己的读位置（播放线程只需要一个读者）。
    pos: u64,
}

impl StreamingBuffer {
    pub fn new(total: Option<u64>) -> Self {
        Self {
            inner: Arc::new((
                Mutex::new(Inner {
                    total,
                    ..Default::default()
                }),
                Condvar::new(),
            )),
            pos: 0,
        }
    }

    /// 已经下载到的字节数。
    pub fn buffered_bytes(&self) -> u64 {
        let (lock, _cvar) = &*self.inner;
        let inner = lock.lock().unwrap_or_else(|e| e.into_inner());
        inner.data.len() as u64
    }

    /// 是否已下载完。
    pub fn is_complete(&self) -> bool {
        let (lock, _cvar) = &*self.inner;
        let inner = lock.lock().unwrap_or_else(|e| e.into_inner());
        inner.done
    }

    /// 追加一段下载到的数据，并唤醒正在等它的读线程。
    pub fn push(&self, chunk: &[u8]) {
        if chunk.is_empty() {
            return;
        }
        let (lock, cvar) = &*self.inner;
        let mut inner = lock.lock().unwrap_or_else(|e| e.into_inner());
        inner.data.extend_from_slice(chunk);
        drop(inner);
        cvar.notify_all();
    }

    /// 标记下载结束。`error` 非空表示失败——读线程据此报错，不再傻等。
    pub fn finish(&self, error: Option<String>) {
        let (lock, cvar) = &*self.inner;
        let mut inner = lock.lock().unwrap_or_else(|e| e.into_inner());
        inner.done = true;
        inner.error = error;
        drop(inner);
        cvar.notify_all();
    }

    /// 确保 `target` 位置之前的数据已经下载到。seek 用。
    ///
    /// 下载已经结束还够不着 → 返回 Err（比如用户拖到了还没下到的位置，且
    /// 文件比预期短）。
    fn ensure(&self, target: u64) -> std::io::Result<()> {
        let (lock, cvar) = &*self.inner;
        let mut inner = lock.lock().unwrap_or_else(|e| e.into_inner());

        while (inner.data.len() as u64) < target && !inner.done {
            let result = cvar
                .wait_timeout(inner, WAIT_TIMEOUT)
                .unwrap_or_else(|e| e.into_inner());
            inner = result.0;
            if result.1.timed_out() && (inner.data.len() as u64) < target {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "等待音频数据超时：想要 {} 字节，只有 {} 字节",
                        target,
                        inner.data.len()
                    ),
                ));
            }
        }

        if inner.error.is_some() {
            return Err(std::io::Error::other(
                inner.error.clone().unwrap_or_default(),
            ));
        }

        if inner.data.len() as u64 >= target {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                format!(
                    "音频数据不足：需要 {} 字节，实际 {} 字节",
                    target,
                    inner.data.len()
                ),
            ))
        }
    }
}

impl Read for StreamingBuffer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        // 先把读指针需要的数据等来
        match self.ensure(self.pos + 1) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                // 已经到文件末尾了，正常结束
                return Ok(0);
            }
            Err(error) => return Err(error),
        }

        let (lock, _cvar) = &*self.inner;
        let inner = lock.lock().unwrap_or_else(|e| e.into_inner());
        let start = self.pos as usize;
        let available = inner.data.len().saturating_sub(start);
        let n = buf.len().min(available);
        if n == 0 {
            return Ok(0);
        }
        buf[..n].copy_from_slice(&inner.data[start..start + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for StreamingBuffer {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let (lock, _cvar) = &*self.inner;
        let total_hint = {
            let inner = lock.lock().unwrap_or_else(|e| e.into_inner());
            inner.total
        };

        let target = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => {
                // 流式播放时文件还没下完，"末尾"只能用声明的总长度估算
                let total = total_hint.unwrap_or_else(|| {
                    self.ensure(u64::MAX).ok();
                    self.buffered_bytes()
                });
                total.saturating_add_signed(offset)
            }
            SeekFrom::Current(offset) => self.pos.saturating_add_signed(offset),
        };

        self.ensure(target)?;
        self.pos = target;
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom};
    use std::thread;

    #[test]
    fn reads_buffered_data() {
        let buffer = StreamingBuffer::new(Some(4));
        buffer.push(b"abcd");
        buffer.finish(None);

        let mut reader = buffer.clone();
        let mut out = [0u8; 4];
        assert_eq!(reader.read(&mut out).unwrap(), 4);
        assert_eq!(&out, b"abcd");
        // 读完就是 EOF
        assert_eq!(reader.read(&mut out).unwrap(), 0);
    }

    #[test]
    fn read_blocks_until_data_arrives() {
        let buffer = StreamingBuffer::new(None);
        let writer = buffer.clone();

        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            writer.push(b"late");
            writer.finish(None);
        });

        // 下载线程还没动，这里会阻塞等——正是边下边播要的行为
        let mut reader = buffer.clone();
        let mut out = [0u8; 4];
        assert_eq!(reader.read(&mut out).unwrap(), 4);
        assert_eq!(&out, b"late");
        handle.join().expect("下载线程正常结束");
    }

    #[test]
    fn seek_beyond_buffer_reports_error() {
        let buffer = StreamingBuffer::new(Some(100));
        buffer.push(b"ab");
        buffer.finish(None);

        let mut reader = buffer.clone();
        // 声明 100 字节但只下了 2 字节，拖到第 50 字节应当失败而不是假装成功
        assert!(reader.seek(SeekFrom::Start(50)).is_err());
    }

    #[test]
    fn seek_within_buffer_works() {
        let buffer = StreamingBuffer::new(Some(6));
        buffer.push(b"abcdef");
        buffer.finish(None);

        let mut reader = buffer.clone();
        reader.seek(SeekFrom::Start(3)).expect("前 3 字节已下载");
        let mut out = [0u8; 3];
        assert_eq!(reader.read(&mut out).unwrap(), 3);
        assert_eq!(&out, b"def");
    }

    #[test]
    fn download_error_surfaces_to_reader() {
        let buffer = StreamingBuffer::new(None);
        buffer.push(b"ab");
        buffer.finish(Some("连接被重置".to_string()));

        let mut reader = buffer.clone();
        // 只有 2 字节，读第 3 字节时应当把下载错误抛出来
        let mut out = [0u8; 8];
        reader.read(&mut out).expect_err("下载失败应当报错");
    }
}
