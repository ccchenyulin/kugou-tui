//! 边下边播的存储层。
//!
//! # 为什么需要它
//!
//! `rodio::Decoder<R>` 要求 `R: Read + Seek`（见 docs.rs，rodio 0.22 没有
//! `new_stream`）。网络流只能 `Read` 不能 `Seek`，所以不能直接喂给 rodio。
//!
//! 早期这里手写过一个 `StreamingBuffer`：把整首歌的字节攒在内存 `Vec<u8>`
//! 里并实现 `Seek`。能用，但有两个毛病：
//!
//! 1. **内存峰值 = 整首歌**。一首无损 20-40 MB 全堆在堆上，播完才释放。
//! 2. **下载完成要重播**。内存缓冲是一次性的，落盘后为了让解码器能用
//!    `Seek` 到任意位置，上层会再 `load` 一次文件——表现为「首播 1 秒后
//!    从头重来」。
//!
//! 现在改成「数据直接落盘，播放读的就是那个文件」：
//!
//! * 后台任务把响应的 chunk 写进缓存文件；
//! * [`CacheFileProvider`] 同时给出这个文件的读句柄与写句柄；
//! * `stream-download` 在字节还没到位时阻塞等待，`Read` 因此可以安全地
//!   跑在写指针前面；
//! * 内存里只有 `BufReader` 的一小段，与文件大小无关。
//!
//! 落盘的文件**就是**缓存条目本身，播完下次直接命中，不再有「内存 + 磁盘
//! 写两份」的浪费。

use std::fs::{File, OpenOptions};
use std::io;
use std::path::PathBuf;

use stream_download::storage::StorageProvider;

/// 把下载数据直接写进 `path` 的存储后端。
///
/// `into_reader_writer` 会**开两个句柄指向同一个文件**：写句柄追加数据，
/// 读句柄供解码器按需读取。二者各自维护位置，互不干扰——这正是
/// `stream-download` 要求的契约。
#[derive(Debug)]
pub struct CacheFileProvider {
    path: PathBuf,
}

impl CacheFileProvider {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl StorageProvider for CacheFileProvider {
    type Reader = File;
    type Writer = File;

    fn into_reader_writer(
        self,
        _content_length: Option<u64>,
    ) -> io::Result<(Self::Reader, Self::Writer)> {
        // 先建写句柄并截断：确保这是一个全新的空文件，旧的半截数据不会
        // 混进来。读句柄随后打开，拿到的是同一个 inode。
        let writer = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .read(true)
            .open(&self.path)?;
        let reader = File::open(&self.path)?;
        Ok((reader, writer))
    }
}
