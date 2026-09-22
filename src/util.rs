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

/// 本地时区的「今天」，格式 `2026-09-23`。
///
/// # 为什么必须是**本地**日期
///
/// 酷狗的领取接口要传「要领取的那一天」，传过去的日期就是领到的那天。UTC 日期
/// 在 UTC+8 的凌晨 0 点到 8 点之间还停在昨天，那时按 UTC 算就会去领一天已经过去的
/// VIP——白打一次接口，而且那天的权益也拿不回来。
///
/// 拿不到本地时区时返回 `None`（多线程环境下 `time` 可能拒绝推断偏移）。调用方
/// 应当**放弃领取并如实告知**，而不是退回 UTC 猜一个：猜错是白领，不领只是少一天。
pub fn today_local() -> Option<String> {
    let now = time::OffsetDateTime::now_local().ok()?;
    Some(format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    ))
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

    /// 日期格式必须正好是接口要的 `YYYY-MM-DD`：多一个空格、少一个前导零，
    /// 服务端都只当是「那一天不存在」，报错还看不出原因。
    #[test]
    fn today_local_is_a_plain_iso_date() {
        let Some(today) = today_local() else {
            // 多线程下 time 可能拒绝推断本地偏移，这时调用方会放弃领取。
            // 测试不能因此变成偶发失败。
            return;
        };
        assert_eq!(today.len(), 10, "应当是 YYYY-MM-DD：{today}");
        let parts: Vec<&str> = today.split('-').collect();
        assert_eq!(parts.len(), 3, "应当是三段：{today}");
        assert_eq!(parts[0].len(), 4, "年份四位：{today}");
        assert_eq!(parts[1].len(), 2, "月份两位（要补零）：{today}");
        assert_eq!(parts[2].len(), 2, "日期两位（要补零）：{today}");
        assert!(
            parts
                .iter()
                .all(|part| part.chars().all(|c| c.is_ascii_digit())),
            "只该有数字和短横线：{today}"
        );
    }
}
