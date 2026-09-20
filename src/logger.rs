//! 极简文件日志。
//!
//! 刻意不引入 `tracing` + `tracing-subscriber`：一个 TUI 播放器只需要把异常落到
//! 文件里，完整的订阅者体系会额外带来数百 KB 二进制体积和启动期开销，与「低资源
//! 占用」的目标相冲突。日志写入失败一律静默吞掉——日志本身不该拖垮主流程。

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

pub const LEVEL_ERROR: &str = "ERROR";
pub const LEVEL_WARN: &str = "WARN";
pub const LEVEL_INFO: &str = "INFO";
pub const LEVEL_DEBUG: &str = "DEBUG";

static SINK: OnceLock<Mutex<File>> = OnceLock::new();
static DEBUG_ENABLED: OnceLock<bool> = OnceLock::new();

/// DEBUG 级别默认不输出——按键、位置同步这类日志每帧都会产生，默认打开会把
/// 日志淹掉。排查输入问题时用 `KUGOU_TUI_DEBUG=1 kugou-tui` 临时开启。
fn debug_enabled() -> bool {
    *DEBUG_ENABLED.get_or_init(|| {
        std::env::var("KUGOU_TUI_DEBUG")
            .map(|value| !value.is_empty() && value != "0")
            .unwrap_or(false)
    })
}

/// 打开（不存在则创建）日志文件。
///
/// 未调用本函数时所有 `tlog!` 调用都是空操作，因此日志是可选的。
pub fn init(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let _ = SINK.set(Mutex::new(file));
    Ok(())
}

/// 写入一行日志。
pub fn write(level: &str, message: &str) {
    if level == LEVEL_DEBUG && !debug_enabled() {
        return;
    }
    let Some(sink) = SINK.get() else {
        return;
    };
    let Ok(mut file) = sink.lock() else {
        return;
    };
    let _ = writeln!(file, "{} [{}] {}", timestamp_now(), level, message);
}

/// 形如 `2026-09-20 03:44:15Z` 的 UTC 时间戳。
fn timestamp_now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default();
    let (year, month, day, hour, minute, second) = civil_from_unix(seconds);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}Z")
}

/// 把 UNIX 时间戳换算成 UTC 日历时间。
///
/// 使用 Howard Hinnant 的 `civil_from_days` 算法，从而不依赖 `chrono` / `time`。
fn civil_from_unix(seconds: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let remainder = seconds.rem_euclid(86_400);

    // 以 0000-03-01 为原点，让闰年落在周期末尾，简化后续运算
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097); // 400 年 = 146097 天
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;

    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let march_index = (5 * day_of_year + 2) / 153;

    let day = day_of_year - (153 * march_index + 2) / 5 + 1;
    let month = if march_index < 10 {
        march_index + 3
    } else {
        march_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    (
        year,
        month as u32,
        day as u32,
        (remainder / 3_600) as u32,
        ((remainder % 3_600) / 60) as u32,
        (remainder % 60) as u32,
    )
}

/// 统一的日志宏，避免到处写 `crate::logger::write(...)`。
///
/// 用 `pub(crate) use` 而不是 `#[macro_export]`：后者会把宏暴露到 crate 根、
/// 污染公开 API，而这里只需要内部使用。代价是每个用到的模块要显式
/// `use crate::logger::tlog;`——这反而让依赖关系一目了然。
macro_rules! tlog {
    ($level:expr, $($arg:tt)*) => {
        $crate::logger::write($level, &format!($($arg)*))
    };
}

pub(crate) use tlog;

#[cfg(test)]
mod tests {
    use super::civil_from_unix;

    #[test]
    fn converts_epoch_to_calendar_date() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
        // 2026-09-20T03:44:15Z（用 `date -u -d @1789875855` 核对过）
        assert_eq!(civil_from_unix(1_789_875_855), (2026, 9, 20, 3, 44, 15));
        // 闰日
        assert_eq!(civil_from_unix(1_709_164_800), (2024, 2, 29, 0, 0, 0));
    }
}
