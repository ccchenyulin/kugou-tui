//! 播放电平采集。
//!
//! # 为什么是真的读采样
//!
//! 侧边栏那个小跳动条的数据来自这里。做法是把音频流包一层，采样原样透传给播放器，
//! 顺带把峰值记下来——所以它是**真的跟着音乐走**的：静音会掉到底，鼓点会顶到头。
//!
//! 拿随机数画动画当然更省事，但那样做出来的是装饰品，和音量、和音乐都没关系，
//! 属于自欺欺人。这里不这么做。

use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use rodio::Source;
use rodio::source::SeekError;

/// 保留多少个电平格子。侧边栏宽度有限，28 格刚好铺满一行。
pub const LEVEL_BUCKETS: usize = 28;

/// 每 N 个采样聚合成一个格子。
///
/// 44100Hz 立体声下约 15ms 一格：视觉上跟手，又不会被单个采样的抖动带偏。
const SAMPLES_PER_BUCKET: usize = 1300;

/// 播放电平：环形缓冲，存最近若干格子的峰值（0..=1000 的整数，避免浮点原子量）。
///
/// 用原子量而不是互斥锁：音频线程每隔一小段就写一次，主线程每帧读一次，
/// 无锁能保证任何一方都不会被对方卡住。
#[derive(Debug, Clone)]
pub struct AudioLevels {
    buckets: Arc<Vec<AtomicU32>>,
    cursor: Arc<AtomicUsize>,
}

impl Default for AudioLevels {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioLevels {
    pub fn new() -> Self {
        let mut buckets = Vec::with_capacity(LEVEL_BUCKETS);
        buckets.resize_with(LEVEL_BUCKETS, || AtomicU32::new(0));
        Self {
            buckets: Arc::new(buckets),
            cursor: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// 写入一格峰值（入参 0.0 ~ 1.0）。
    fn push(&self, level: f32) {
        let value = (level.clamp(0.0, 1.0) * 1000.0) as u32;
        let index = self.cursor.fetch_add(1, Ordering::Relaxed) % LEVEL_BUCKETS;
        self.buckets[index].store(value, Ordering::Relaxed);
    }

    /// 按时间顺序取出全部格子（最旧 → 最新），归一化到 0.0 ~ 1.0。
    pub fn snapshot(&self) -> Vec<f32> {
        let cursor = self.cursor.load(Ordering::Relaxed);
        (0..LEVEL_BUCKETS)
            .map(|offset| {
                let index = (cursor + offset) % LEVEL_BUCKETS;
                self.buckets[index].load(Ordering::Relaxed) as f32 / 1000.0
            })
            .collect()
    }

    /// 清零。停止播放时调用，否则会留一段不动的柱子，看着像卡住了。
    pub fn clear(&self) {
        for bucket in self.buckets.iter() {
            bucket.store(0, Ordering::Relaxed);
        }
    }
}

/// 把音源包起来：采样透传，同时把峰值记进 [`AudioLevels`]。
pub struct LevelMeter<S> {
    inner: S,
    levels: AudioLevels,
    peak: f32,
    counted: usize,
}

impl<S> LevelMeter<S> {
    pub fn new(inner: S, levels: AudioLevels) -> Self {
        Self {
            inner,
            levels,
            peak: 0.0,
            counted: 0,
        }
    }
}

impl<S> Iterator for LevelMeter<S>
where
    S: Source,
{
    type Item = S::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;

        let amplitude = sample.abs();
        if amplitude > self.peak {
            self.peak = amplitude;
        }

        self.counted += 1;
        if self.counted >= SAMPLES_PER_BUCKET {
            self.levels.push(self.peak);
            self.peak = 0.0;
            self.counted = 0;
        }

        Some(sample)
    }
}

impl<S> Source for LevelMeter<S>
where
    S: Source,
{
    // rodio 0.22 起叫 current_span_len（旧名是 current_frame_len）
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> NonZero<u16> {
        self.inner.channels()
    }

    fn sample_rate(&self) -> NonZero<u32> {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    /// 必须转发，否则进度条会完全失效。
    ///
    /// `Source::try_seek` 的**默认实现直接返回 `NotSupported`**。这个包装器如果漏掉它，
    /// rodio 就会认为整条音频不可跳转——点进度条、方向键 seek 全部变成静默失败，
    /// 而播放本身完全正常，极难察觉是这个包装层吞掉的。
    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(position)
    }
}
