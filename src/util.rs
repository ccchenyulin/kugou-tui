//! 无第三方依赖的小工具。
//!
//! 这里只放两件事：
//!
//! 1. **伪随机数**。只为了「随机播放」这一个用途，不值得为此引入 `rand` +
//!    `getrandom` 两条依赖链（release 构建下约几十 KB）。用 xorshift64* 足够，
//!    且不涉及任何密码学用途。
//! 2. **时间戳**。避免为了一个 `now()` 引入 `chrono`。

use std::cell::Cell;
use std::time::{SystemTime, UNIX_EPOCH};

thread_local! {
    /// xorshift64* 的状态。初值由时钟纳秒与栈地址混合，避免多线程撞种子。
    static RNG_STATE: Cell<u64> = const { Cell::new(0) };
}

/// 当前 UNIX 时间戳（毫秒）。用于给请求加时间戳以绕开服务端缓存。
pub fn now_unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default()
}

/// 生成一个 64 位伪随机数。
pub fn random_u64() -> u64 {
    RNG_STATE.with(|state| {
        let mut value = state.get();
        if value == 0 {
            value = seed();
            // xorshift 的状态不能为 0
            if value == 0 {
                value = 0x9E37_79B9_7F4A_7C15;
            }
        }
        // xorshift64*
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        state.set(value);
        value.wrapping_mul(0x2545_F491_4F6C_DD1D)
    })
}

/// 生成 `[0, bound)` 内的伪随机数。`bound <= 1` 时返回 0。
///
/// 用 Lemire 的乘法取模法代替整数取模，避免低位偏置。
pub fn random_below(bound: usize) -> usize {
    if bound <= 1 {
        return 0;
    }
    let bound = bound as u64;
    let product = u128::from(random_u64()) * u128::from(bound);
    (product >> 64) as usize
}

fn seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or_default();
    // 与栈上局部变量的地址、进程 id 混合，让不同线程拿到不同种子
    let local = 0u8;
    let address = std::ptr::from_ref(&local) as u64;
    nanos ^ address.rotate_left(17) ^ u64::from(std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_below_stays_in_range() {
        for _ in 0..1_000 {
            assert!(random_below(7) < 7);
        }
        assert_eq!(random_below(0), 0);
        assert_eq!(random_below(1), 0);
    }

    #[test]
    fn successive_random_values_differ() {
        assert_ne!(random_u64(), random_u64());
    }

    #[test]
    fn timestamp_is_plausible() {
        // 2020-01-01 之后的毫秒时间戳
        assert!(now_unix_millis() > 1_577_836_800_000);
    }
}
