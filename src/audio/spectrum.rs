//! 频谱分析：把一段时域采样变成若干「频段能量」。
//!
//! # 为什么自己写 FFT
//!
//! 只需要一个功率是 2 的实序列正变换，radix-2 迭代实现不到 60 行。为这点东西
//! 拖进 `rustfft` 之类（连带它的一堆依赖与 SIMD 后端）不划算——依赖是要付体积
//! 和维护成本的。
//!
//! # 为什么要分频而不是直接画 bin
//!
//! FFT 出来的是**线性**频率：2048 点、44.1kHz 下每格约 21.5Hz，于是整个可闻
//! 低频段（20~200Hz）挤在最左边两三个格子里，右边一大半全是几乎不动的高频。
//! 人耳对频率的感知是**对数**的，所以按对数划分频段：低频窄、高频宽，每个频段
//! 对应一个柱子，看起来才是熟悉的频谱仪。

/// 一次分析取的采样点数。必须是 2 的幂。
///
/// 2048 在 44.1kHz 下约 46ms：短到能跟上鼓点，又长到有 ~21Hz 的频率分辨率。
pub const WINDOW_SIZE: usize = 2048;

/// 频段数。渲染时再按实际列数聚合，这里取足够细的分辨率即可。
pub const BAND_COUNT: usize = 64;

/// 最低/最高分析频率（Hz）。
///
/// 低于 40Hz 基本是 DC 与次声，画出来只会让最左边一根柱子常亮；高于 16kHz
/// 大多是噪声，且多数有损编码在这个区间已经砍掉了内容。
const FREQ_MIN: f32 = 40.0;
const FREQ_MAX: f32 = 16_000.0;

/// dB 显示范围。低于下限算静音，高于上限算顶格。
///
/// 音乐的动态范围比这宽得多，但终端只有十几行高，压到 60dB 窗口里才看得出起伏。
const DB_FLOOR: f32 = -62.0;
const DB_CEIL: f32 = -8.0;

/// 每个频段向高频递增的补偿（dB）。
///
/// 音乐的能量天然集中在低频，不做补偿的话右边大半根柱子永远趴着。按每八度约
/// +3dB 折算：`BAND_COUNT` 个频段铺满 log2(16000/40)≈8.6 个八度，约合每频段
/// 0.25dB。这是观感补偿，不是测量。
const TILT_PER_BAND: f32 = 0.25;

/// 原地 radix-2 FFT。`re` / `im` 长度相同且必须是 2 的幂。
///
/// 长度不合要求时直接返回（不 panic）：这是显示用的旁路数据，
/// 不该因为它把播放搞崩。
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    if n != im.len() || !n.is_power_of_two() {
        return;
    }

    // 位反转置换：把输入重排成迭代版 FFT 需要的顺序，省掉递归
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    let mut len = 2;
    while len <= n {
        let half = len / 2;
        // 旋转因子 w = e^(-2πi/len)，取负号对应正向变换
        let step = -2.0 * std::f32::consts::PI / len as f32;
        let mut start = 0;
        while start < n {
            for k in 0..half {
                let (sin, cos) = (step * k as f32).sin_cos();
                // w = cos + i·sin，即 e^(step·k·i)
                let a = start + k;
                let b = a + half;
                let tr = re[b] * cos - im[b] * sin;
                let ti = re[b] * sin + im[b] * cos;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
            start += len;
        }
        len <<= 1;
    }
}

/// 把一段单声道采样（长度应为 [`WINDOW_SIZE`]）分析成 `bands` 个 0.0~1.0 的能量值。
///
/// 返回空向量表示这次没法算（采样不够或采样率未知），调用方保持上一帧即可。
pub fn analyze(samples: &[f32], sample_rate: u32, bands: usize) -> Vec<f32> {
    if samples.len() < 2 || bands == 0 || sample_rate == 0 {
        return Vec::new();
    }

    // 取不超过输入长度的最大 2 的幂：FFT 只认这个长度。
    // `next_power_of_two()` 对已经是 2 的幂的数返回自身，只有不是时才要折半。
    let capped = samples.len().min(WINDOW_SIZE);
    let n = if capped.is_power_of_two() {
        capped
    } else {
        capped.next_power_of_two() / 2
    };
    if n < 2 {
        return Vec::new();
    }
    // 用窗口**末尾**的采样：那是刚刚播过的声音，画出来才跟得上
    let offset = samples.len() - n;

    let mut re = Vec::with_capacity(n);
    let mut im = vec![0.0f32; n];
    let mut window_sum = 0.0f32;
    // Hann 窗：直接截断一段采样会在两端产生频谱泄漏，能量糊到整个频域上，
    // 表现为所有柱子一起微微发抖。加窗把两端平滑到 0 就没有这个问题。
    for i in 0..n {
        let window = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos());
        window_sum += window;
        re.push(samples[offset + i] * window);
    }

    fft(&mut re, &mut im);

    // 归一化：原始 DFT 幅度与点数成正比（不加这一步，幅度动辄几百，
    // 换算成 dB 之后每个频段都会顶格，画出来是一堵齐平的墙）。
    // 除以「窗函数之和 / 2」后，幅度 A 的纯音在它那一列得到的值就是 A。
    let norm = (window_sum / 2.0).max(1.0);

    // 正频率部分：bin i 对应 i * sample_rate / n 赫兹
    let bins = n / 2;
    let nyquist = sample_rate as f32 / 2.0;
    let freq_max = FREQ_MAX.min(nyquist);
    let magnitudes: Vec<f32> = (0..bins)
        .map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt() / norm)
        .collect();

    // 对数分频：第 k 段覆盖 [FREQ_MIN * ratio^k, FREQ_MIN * ratio^(k+1))
    let ratio = (freq_max / FREQ_MIN).powf(1.0 / bands as f32);
    let mut out = Vec::with_capacity(bands);
    for band in 0..bands {
        let low = FREQ_MIN * ratio.powi(band as i32);
        let high = FREQ_MIN * ratio.powi(band as i32 + 1);
        let low_bin = (low * n as f32 / sample_rate as f32).floor() as usize;
        let high_bin = (high * n as f32 / sample_rate as f32).ceil() as usize;

        // 一个频段里可能压着好几个 bin，也可能（低频段）一个 bin 都没有
        let peak = if low_bin >= high_bin {
            magnitudes
                .get(low_bin.min(bins - 1))
                .copied()
                .unwrap_or(0.0)
        } else {
            magnitudes[low_bin.min(bins)..high_bin.min(bins)]
                .iter()
                .fold(0.0f32, |max, &value| max.max(value))
        };

        // 幅度 → dB → 0..1，再叠一点高频补偿
        let db = 20.0 * peak.max(1e-9).log10() + TILT_PER_BAND * band as f32;
        let normalized = (db - DB_FLOOR) / (DB_CEIL - DB_FLOOR);
        out.push(normalized.clamp(0.0, 1.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成 `frequency` 赫兹的正弦，幅度 0.8。
    fn sine(frequency: f32, sample_rate: u32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| {
                0.8 * (2.0 * std::f32::consts::PI * frequency * i as f32 / sample_rate as f32).sin()
            })
            .collect()
    }

    #[test]
    fn fft_of_dc_lands_entirely_in_bin_zero() {
        // 常数信号只有直流分量：能量应当全部落在第 0 个 bin
        let n = 8;
        let mut re = vec![1.0f32; n];
        let mut im = vec![0.0f32; n];
        fft(&mut re, &mut im);

        assert!((re[0] - n as f32).abs() < 1e-4, "直流分量 = 所有采样之和");
        for i in 1..n {
            assert!(
                re[i].abs() < 1e-4 && im[i].abs() < 1e-4,
                "其余 bin 应当为零"
            );
        }
    }

    #[test]
    fn fft_puts_a_pure_tone_in_its_own_bin() {
        // 4Hz 的信号、8 点、采样率 32Hz → 正好落在 bin 1
        let n = 8;
        let sample_rate = 32;
        let mut re = sine(4.0, sample_rate, n);
        let mut im = vec![0.0f32; n];
        fft(&mut re, &mut im);

        // 幅度 A 的实正弦，能量平分到 bin 1 与 bin n-1，各占 A·n/2。
        // 单个 bin 上是纯虚数，所以看模长而不是实部。
        let magnitude = (re[1] * re[1] + im[1] * im[1]).sqrt();
        assert!(
            (magnitude - 0.8 * n as f32 / 2.0).abs() < 1e-3,
            "bin 1 应当是主峰，实际 {magnitude}"
        );
        let off_peak = (re[3] * re[3] + im[3] * im[3]).sqrt();
        assert!(off_peak < 1e-3, "bin 3 不该有能量，实际 {off_peak}");
    }

    #[test]
    fn fft_ignores_malformed_input() {
        // 长度不是 2 的幂时安静地什么都不做，而不是 panic
        let mut re = vec![1.0f32; 6];
        let mut im = vec![0.0f32; 6];
        fft(&mut re, &mut im);
        assert_eq!(re, vec![1.0f32; 6], "输入应原样保留");
    }

    #[test]
    fn a_low_tone_lights_the_left_bands_only() {
        let sample_rate = 44_100;
        let samples = sine(100.0, sample_rate, WINDOW_SIZE);
        let bands = analyze(&samples, sample_rate, BAND_COUNT);

        assert_eq!(bands.len(), BAND_COUNT);
        let peak_index = bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(index, _)| index)
            .expect("总该有一列最大");
        assert!(
            peak_index < BAND_COUNT / 4,
            "100Hz 是低频，峰值应当落在最左边四分之一，实际在第 {peak_index} 列"
        );
    }

    #[test]
    fn a_high_tone_lights_the_right_bands() {
        let sample_rate = 44_100;
        let samples = sine(8_000.0, sample_rate, WINDOW_SIZE);
        let bands = analyze(&samples, sample_rate, BAND_COUNT);

        let peak_index = bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(index, _)| index)
            .expect("总该有一列最大");
        assert!(
            peak_index > BAND_COUNT / 2,
            "8kHz 是高频，峰值应当落在右半边，实际在第 {peak_index} 列"
        );
    }

    #[test]
    fn silence_produces_nothing() {
        let bands = analyze(&vec![0.0f32; WINDOW_SIZE], 44_100, BAND_COUNT);
        assert!(bands.iter().all(|&value| value == 0.0), "静音应当是平的");
    }

    #[test]
    fn degenerate_inputs_yield_no_bands() {
        assert!(analyze(&[], 44_100, BAND_COUNT).is_empty());
        assert!(analyze(&[0.0; 64], 0, BAND_COUNT).is_empty(), "采样率未知");
        assert!(analyze(&[0.0; 64], 44_100, 0).is_empty());
    }
}
