//! kitty 终端图形协议：把封面按原始像素画出来。
//!
//! # 为什么需要它
//!
//! 半块字符（`▀▄█`）能把图显示出来，但每个字符只能承载 2 个像素、还得做黑白
//! 剪影，24 列的封面糊成一团——用户要的是「1:1 还原」。kitty 的图形协议可以直接
//! 在终端里放置真彩图片，由终端自己缩放合成，这才是封面该有的样子。
//!
//! # 协议要点
//!
//! 传输与显示合并在一条转义序列里：
//!
//! ```text
//! \x1b_Ga=T,f=100,c=<列>,r=<行>,m=<是否还有后续>;<base64 数据>\x1b\\
//! ```
//!
//! * `a=T` —— transmit and display（传完就画）
//! * `f=100` —— 数据是 PNG，终端负责解码
//! * `c` / `r` —— 画在多少个字符格子里（终端按这个缩放）
//! * 数据超过 4096 字节要分块，除最后一块外 `m=1`
//!
//! # 为什么每帧都要重发
//!
//! ratatui 用双缓冲做整屏差分重绘。图片是终端层面的浮层，一旦 ratatui 重写了
//! 图片所在的格子，图片就被擦掉了。所以每帧渲染完都要重新发一次——PNG 编码结果
//! 缓存在内存里，重发只是拼一次 base64，代价可以接受。

use std::sync::OnceLock;

/// 单条转义序列里能塞的 base64 长度。协议建议不超过 4096 字节。
const CHUNK: usize = 4096;

/// 终端是否支持 kitty 图形协议。
///
/// 用 `OnceLock` 缓存：环境变量在一次进程生命周期内不会变，而检测本身要读环境。
pub fn is_supported() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        // kitty 自己会设 KITTY_WINDOW_ID；其它实现了该协议的终端（如 ghostty、
        // WezTerm）通常也认 TERM 里的 kitty 字样。
        std::env::var_os("KITTY_WINDOW_ID").is_some()
            || std::env::var("TERM")
                .map(|term| term.contains("kitty"))
                .unwrap_or(false)
    })
}

/// 生成「把 PNG 画出来」的转义序列。
///
/// **只指定列数，不指定行数**，高度由终端按图片自身的宽高比推算。
///
/// 早先这里同时传了 `c` 和 `r`，封面会被压变形——终端会老老实实把图片
/// **拉伸**填满那个矩形，而字符格不是正方形（高约为宽的两倍），于是
/// 「列数 = 行数 × 2」的矩形在像素上并不是方的，方形封面就被拉窄了。
/// 交给终端自己算比例，怎么都不会变形。
pub fn display_png(png: &[u8], columns: u16, id: u32) -> String {
    let encoded = base64(png);
    let mut out = String::with_capacity(encoded.len() + 64);

    // 分块传输：最后一块 m=0，其余 m=1
    let chunks: Vec<&str> = encoded
        .as_bytes()
        .chunks(CHUNK)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
        .collect();

    for (index, chunk) in chunks.iter().enumerate() {
        let last = index + 1 == chunks.len();
        if index == 0 {
            // 只有第一块带控制参数
            out.push_str("\x1b_Ga=T,f=100,");
            out.push_str(&format!("c={columns},i={id},"));
        } else {
            out.push_str("\x1b_G");
        }
        out.push_str(&format!("m={};", u8::from(!last)));
        out.push_str(chunk);
        out.push_str("\x1b\\");
    }

    out
}

/// 删除指定 ID 的图片。
///
/// **必须每帧调用**：kitty 的图片是终端层面的浮层，显示在字符**之上**，
/// ratatui 的整屏重绘擦不掉它。只发不删的话，切到别的页面后旧封面会留在
/// 原地——切几次就叠出好几张。
///
/// 按 ID 删而不是删全部：终端里可能有别的程序也放了图，不该波及。
pub fn delete_image(id: u32) -> String {
    format!("\x1b_Ga=d,d=i,i={id}\x1b\\")
}

/// 标准 base64 编码。
///
/// 手写而不是引依赖：只用到一个方向、一份实现不到 30 行，不值得为它多一个
/// 包（以及随之而来的版本维护）。
fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map(u32::from);
        let b2 = chunk.get(2).copied().map(u32::from);

        let triple = (b0 << 16) | (b1.unwrap_or(0) << 8) | b2.unwrap_or(0);

        out.push(TABLE[(triple >> 18) as usize & 0x3F] as char);
        out.push(TABLE[(triple >> 12) as usize & 0x3F] as char);
        // 不足 3 字节时用 '=' 补齐
        out.push(if b1.is_some() {
            TABLE[(triple >> 6) as usize & 0x3F] as char
        } else {
            '='
        });
        out.push(if b2.is_some() {
            TABLE[triple as usize & 0x3F] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_examples() {
        // RFC 4648 的测试向量
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn display_sequence_carries_size_and_chunks() {
        let png = vec![0u8; 8];
        let seq = display_png(&png, 30, 1);
        assert!(
            seq.starts_with("\x1b_Ga=T,f=100,c=30,i=1,m=0;"),
            "首块要带控制参数与图片 ID"
        );
        assert!(seq.ends_with("\x1b\\"), "必须以 ST 结束");
    }

    /// 删除序列必须带上 ID：不带就会把终端里其它程序放的图一并删掉。
    #[test]
    fn delete_sequence_targets_specific_image() {
        let seq = delete_image(7);
        assert_eq!(seq, "\x1b_Ga=d,d=i,i=7\x1b\\");
    }

    #[test]
    fn large_payload_is_split_into_chunks() {
        // 超过一块的阈值，应该出现多条序列且中间块 m=1
        let png = vec![0u8; 8192];
        let seq = display_png(&png, 10, 1);
        assert!(seq.contains("m=1;"), "分块时中间块应为 m=1");
        assert_eq!(seq.matches("\x1b_G").count(), 3, "8192 字节 → 3 块 base64");
    }
}
