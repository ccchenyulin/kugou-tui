//! 统一错误类型。
//!
//! 所有与外部世界的交互（HTTP、文件系统、音频设备）都必须转换成带上下文的
//! [`AppError`] 再向上传播。非测试代码里禁止出现裸 `unwrap()` / `expect()`。

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    /// 传输层失败：DNS 解析、连接被拒、TLS 握手、超时。
    #[error("网络请求失败：{0}")]
    Http(#[from] reqwest::Error),

    /// KuGouMusicApi 返回了非 2xx 状态码。
    #[error("接口 {path} 返回状态码 {status}")]
    HttpStatus { path: String, status: u16 },

    /// 业务层错误码，例如 `error_code: 152` 表示缺少认证信息。
    #[error("接口 {path} 返回错误：code={code} {message}")]
    Api {
        path: String,
        code: i64,
        message: String,
    },

    /// 响应体不是合法 JSON，或字段类型与预期不符。
    #[error("JSON 解析失败：{0}")]
    Json(#[from] serde_json::Error),

    /// 未附带路径的 IO 错误（多为 `?` 自动转换产生）。
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),

    /// 带路径的 IO 错误，排查问题时能直接定位到文件。
    #[error("文件 {path} 操作失败：{source}")]
    IoAt {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("配置错误：{0}")]
    Config(String),

    #[error("音频引擎错误：{0}")]
    Audio(String),

    #[error("未找到资源：{0}")]
    NotFound(String),
    #[error("{0}")]
    /// 其它内部错误（任务调度失败之类），保留上下文便于定位。
    Other(String),
}

impl AppError {
    /// 为 IO 错误补充出错的文件路径。
    pub fn io_at(path: impl Into<String>, source: std::io::Error) -> Self {
        Self::IoAt {
            path: path.into(),
            source,
        }
    }

    /// 是否是「页码越界」。
    ///
    /// 实测搜索接口 `page` 最多到 16 页，再往后返回 `error_code: 149`（Out Page Range）。
    /// 这是正常的"没更多了"，不是故障——翻页时应当据此停止，而不是当错误抛出。
    pub fn is_page_out_of_range(&self) -> bool {
        match self {
            Self::Api { code, .. } => *code == 149,
            _ => false,
        }
    }

    /// 是否属于「需要登录」类错误。UI 层据此弹出登录引导而不是普通报错。
    ///
    /// 已知码（都是实测出来的，上游不公开语义）：
    ///
    /// * `152` —— 搜索接口缺 cookie
    /// * `20005` / `40004` —— 登录态失效
    /// * `20028` —— 取播放直链时的「本次请求需要验证」
    /// * `20010` —— 请求里完全没有可用的认证信息（`/user/playlist` 实测）
    /// * `20017` —— token 本身无效或已过期：`/user/playlist` 只在带了 token 时才
    ///   返回它，不带 token 时返回 `20010`。服务端不附带任何错误描述，所以只认
    ///   这个码，界面才能提示「重新扫码」而不是甩一句 `code=20017`。
    pub fn is_auth_related(&self) -> bool {
        match self {
            Self::Api { code, .. } => matches!(code, 152 | 20005 | 20010 | 20017 | 20028 | 40004),
            _ => false,
        }
    }

    /// 是否是「曾经登录过、但现在失效了」（区别于「从来没登录」）。
    ///
    /// 两者的处置动作都是按 `L`，但说清楚「已失效」能省掉一次自查：
    /// 用户不用先怀疑是不是自己没扫过码。
    pub fn is_login_expired(&self) -> bool {
        match self {
            Self::Api { code, .. } => matches!(code, 20005 | 20017 | 40004),
            _ => false,
        }
    }

    /// 面向用户的一行提示。避免把 reqwest 的长串错误直接糊到状态栏上。
    pub fn user_hint(&self) -> String {
        if self.is_login_expired() {
            return "登录态已失效：按 L 重新扫码（酷狗会定期轮换 token）".to_string();
        }
        if self.is_auth_related() {
            return "需要登录：按 L 扫码，或配置 cookie（--cookie / 配置文件）".to_string();
        }
        match self {
            Self::Http(error) if error.is_timeout() => "请求超时，请检查网络或代理设置".to_string(),
            Self::Http(error) if error.is_connect() => {
                "无法连接 KuGouMusicApi 服务，请确认它已启动（默认 127.0.0.1:3000）".to_string()
            }
            other => other.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(code: i64) -> AppError {
        AppError::Api {
            path: "/user/playlist".to_string(),
            code,
            message: "服务端未提供错误描述".to_string(),
        }
    }

    /// `/user/playlist` 实测：不带认证信息返回 20010，带了失效 token 返回 20017。
    /// 两个都必须归入「需要登录」，否则界面只会甩一句 `code=20017`。
    #[test]
    fn auth_codes_are_recognized() {
        for code in [152, 20005, 20010, 20017, 20028, 40004] {
            assert!(api(code).is_auth_related(), "code={code} 应属于登录类");
        }
        assert!(!api(149).is_auth_related(), "页码越界不是登录问题");
        assert!(api(149).is_page_out_of_range());
    }

    #[test]
    fn expired_login_gets_a_clearer_hint() {
        let hint = api(20017).user_hint();
        assert!(hint.contains("失效"), "应说明是失效而不是没登录：{hint}");
        assert!(hint.contains("L"), "应提示重新扫码：{hint}");

        // 从来没登录过的（152 = 搜索缺 cookie）不该说「失效」
        assert!(api(152).user_hint().contains("需要登录"));
    }
}
