//! 封面字符画：**兜底**路径。
//!
//! # 为什么还需要字符
//!
//! 正常路径是 `ratatui-image`：探测终端支持哪种图形协议（kitty / iTerm2 / sixel），
//! 都不支持时它自己也会退到彩色半块。这里这套单色字符画只在**探测本身失败**
//! 时才用——比如 stdout 不是终端（重定向、某些 CI/容器），根本没得问。
//!
//! 思路和二维码 `qr_lines()` 完全一致：用 `▀▄█` 把上下两个像素压进一个字符
//! （上半格 / 下半格），这样每个像素占「1 列宽 × 半行高」，近似正方形。
//!
//! # 关于明度
//!
//! 只做黑白剪影，不上色：终端背景是深色，所以**亮像素填充、暗像素留空**，
//! 这样才看得见。真彩上色需要给每个字符配一对前景/背景色，字符数一多就是
//! 几百个 Span，对每帧重绘的 TUI 不划算——作为「正在放哪张专辑」的视觉锚点，
//! 剪影已经够用。

use image::DynamicImage;

/// 明度阈值。高于它算「亮」，填充字符。
const BRIGHT_THRESHOLD: f32 = 96.0;

/// 把图片渲染成可直接显示的行。
///
/// * `width` —— 目标**列数**
/// * `height` —— 目标**行数**（每个字符承载上下 2 个像素，因此采样 `height * 2` 行）
pub fn cover_lines(image: &DynamicImage, width: usize, height: usize) -> Vec<String> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let target_height = height * 2;
    // 目标比原图还大（或刚好相等）时不要重采样：滤波会在边界上插值出中间灰，
    // 把本该分明的明暗糊在一起。顺带也省掉一次无谓的重算。
    let rgb = if width as u32 >= image.width() && target_height as u32 >= image.height() {
        image.to_rgb8()
    } else {
        image
            .resize_exact(
                width as u32,
                target_height as u32,
                image::imageops::FilterType::Triangle,
            )
            .to_rgb8()
    };

    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        let upper_row = row * 2;
        let lower_row = upper_row + 1;

        let mut line = String::with_capacity(width);
        for column in 0..width {
            let upper = is_bright(&rgb, column, upper_row);
            let lower = is_bright(&rgb, column, lower_row);
            line.push(match (upper, lower) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        lines.push(line);
    }

    lines
}

/// 该像素是否够亮（够亮才填充字符）。越界当作暗。
fn is_bright(image: &image::RgbImage, x: usize, y: usize) -> bool {
    if x >= image.width() as usize || y >= image.height() as usize {
        return false;
    }
    let pixel = image.get_pixel(x as u32, y as u32);
    // 感知亮度：人眼对绿最敏感
    let luma =
        0.299 * f32::from(pixel[0]) + 0.587 * f32::from(pixel[1]) + 0.114 * f32::from(pixel[2]);
    luma >= BRIGHT_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, gray: u8) -> DynamicImage {
        let mut image = image::RgbImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                image.put_pixel(x, y, image::Rgb([gray, gray, gray]));
            }
        }
        DynamicImage::ImageRgb8(image)
    }

    #[test]
    fn bright_pixels_fill_dark_pixels_stay_empty() {
        // 终端背景是深色，所以只有亮部才该被画出来
        let bright = solid(4, 4, 255);
        assert_eq!(cover_lines(&bright, 2, 2), vec!["██", "██"]);

        let dark = solid(4, 4, 0);
        assert_eq!(cover_lines(&dark, 2, 2), vec!["  ", "  "]);
    }

    /// 一个字符承载上下两个像素：奇偶行错开明暗，才能看出半块是否用对。
    #[test]
    fn upper_and_lower_halves_use_different_glyphs() {
        // 偶数行亮、奇数行暗 → 每个字符都是「上半亮下半暗」→ '▀'
        let mut upper_bright = image::RgbImage::new(2, 4);
        for y in 0..4 {
            for x in 0..2 {
                let gray = if y % 2 == 0 { 255 } else { 0 };
                upper_bright.put_pixel(x, y, image::Rgb([gray, gray, gray]));
            }
        }
        let lines = cover_lines(&DynamicImage::ImageRgb8(upper_bright), 2, 2);
        assert_eq!(lines, vec!["▀▀", "▀▀"], "上半亮下半暗 → 上半块");

        // 反过来 → '▄'
        let mut lower_bright = image::RgbImage::new(2, 4);
        for y in 0..4 {
            for x in 0..2 {
                let gray = if y % 2 == 0 { 0 } else { 255 };
                lower_bright.put_pixel(x, y, image::Rgb([gray, gray, gray]));
            }
        }
        let lines = cover_lines(&DynamicImage::ImageRgb8(lower_bright), 2, 2);
        assert_eq!(lines, vec!["▄▄", "▄▄"], "上半暗下半亮 → 下半块");
    }

    #[test]
    fn zero_size_yields_nothing() {
        let image = solid(4, 4, 128);
        assert!(cover_lines(&image, 0, 4).is_empty());
        assert!(cover_lines(&image, 4, 0).is_empty());
    }
}
