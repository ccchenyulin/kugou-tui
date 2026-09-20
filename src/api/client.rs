//! HTTP 客户端。
//!
//! 只做三件事：拼 URL、带 cookie、把响应体取回来。所有接口语义都在
//! [`crate::api::catalog`] / [`crate::api::lyric`] / [`crate::api::cloud`] 里。
//!
//! # 关于 KuGouMusicApi 的缓存
//!
//! 该服务内置了 2 分钟响应缓存（相同 URL 只回源一次）。对搜索、榜单这类希望拿到
//! 最新数据的接口，需要在 query 里塞一个时间戳让 URL 唯一。这类请求走
//! [`ApiClient::get_json_uncached`]。

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::COOKIE;
use serde_json::Value;

use crate::api::model::check_error_code;
use crate::error::{AppError, Result};
use crate::logger::tlog;
use crate::util::now_unix_millis;

/// 单次请求的超时。取播放直链偶尔会慢，给到 15 秒。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
/// 连接池里每个 host 保留的空闲连接数。4 足够覆盖「搜索 + 歌词 + 取链」的并发。
const POOL_MAX_IDLE: usize = 4;

const USER_AGENT_VALUE: &str = concat!("kugou-tui/", env!("CARGO_PKG_VERSION"));

/// KuGouMusicApi 客户端。
///
/// 内部 `reqwest::Client` 自带连接池，克隆它不会复制连接池，因此可以放心地
/// 在每个异步任务里 clone 一份。
#[derive(Debug, Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base: Arc<str>,
    cookie: Option<Arc<str>>,
}

impl ApiClient {
    /// 构造客户端。`proxy` 形如 `http://127.0.0.1:7890`。
    pub fn new(base: &str, cookie: Option<String>, proxy: Option<&str>) -> Result<Self> {
        let mut builder = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(USER_AGENT_VALUE)
            .pool_max_idle_per_host(POOL_MAX_IDLE)
            .tcp_keepalive(Duration::from_secs(60));

        if let Some(proxy_url) = proxy.map(str::trim).filter(|url| !url.is_empty()) {
            let parsed = reqwest::Proxy::all(proxy_url)
                .map_err(|error| AppError::Config(format!("代理地址 {proxy_url} 无效：{error}")))?;
            builder = builder.proxy(parsed);
        }

        let http = builder
            .build()
            .map_err(|error| AppError::Config(format!("构造 HTTP 客户端失败：{error}")))?;

        Ok(Self {
            http,
            base: Arc::from(base.trim_end_matches('/')),
            cookie: cookie
                .filter(|value| !value.trim().is_empty())
                .map(Arc::from),
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// 更新 cookie（例如自动探测到 dfid 之后）。
    pub fn set_cookie(&mut self, cookie: Option<String>) {
        self.cookie = cookie
            .filter(|value| !value.trim().is_empty())
            .map(Arc::from);
    }

    /// 发送 GET 并返回 `(状态码, 响应体文本)`。
    async fn send(&self, path: &str, query: &[(&str, String)]) -> Result<(u16, String)> {
        let url = format!("{}{}", self.base, path);
        let mut request = self.http.get(&url).query(query);

        if let Some(cookie) = self.cookie.as_ref() {
            request = request.header(COOKIE, cookie.as_ref());
        }

        let response = request.send().await?;
        let status = response.status();
        let body = response.text().await?;
        Ok((status.as_u16(), body))
    }

    /// 发送 POST（JSON body）并解析响应。`/privilege/lite` 这种「按歌曲问『这账号
    /// 能听哪几档音质』」的查询都是 POST + JSON body——GET + query 装不下这种结构。
    ///
    /// 复用 `get_json` 的错误处理：先解析 JSON、再看业务错误码、最后才看 HTTP 状态码。
    pub async fn post_json(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let mut request = self.http.post(&url).json(body);

        if let Some(cookie) = self.cookie.as_ref() {
            request = request.header(COOKIE, cookie.as_ref());
        }

        let response = request.send().await?;
        let status = response.status().as_u16();
        let body = response.text().await?;

        match serde_json::from_str::<Value>(&body) {
            Ok(value) => {
                check_error_code(path, &value)?;
                if !(200..300).contains(&status) {
                    return Err(AppError::HttpStatus {
                        path: path.to_string(),
                        status,
                    });
                }
                Ok(value)
            }
            Err(error) => {
                tlog!(
                    crate::logger::LEVEL_WARN,
                    "接口 {path} 返回了非 JSON 内容（HTTP {status}，前 200 字节）：{}",
                    body.chars().take(200).collect::<String>()
                );
                if !(200..300).contains(&status) {
                    return Err(AppError::HttpStatus {
                        path: path.to_string(),
                        status,
                    });
                }
                Err(AppError::Json(error))
            }
        }
    }

    /// 发送 GET 并解析 JSON，同时校验业务错误码。
    ///
    /// # 为什么先解析、后判状态码
    ///
    /// KuGouMusicApi 在业务失败时会把 `error_code` 放进响应体，而 HTTP 状态码可能同时
    /// 是 `502` 这类网关码。实测 `/search` 未登录时就是「HTTP 502 + body 里
    /// `error_code: 152`」。如果因为状态码非 2xx 就提前返回，体里那个真正能解释原因
    /// 的业务码就被丢掉了，用户只会看到一句没用的「返回状态码 502」。
    ///
    /// 所以顺序是：能解析出 JSON 就先看 `error_code`，它才是权威；只有体不可解析时
    /// 才退回用 HTTP 状态码报错。
    pub async fn get_json(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let (status, body) = self.send(path, query).await?;

        match serde_json::from_str::<Value>(&body) {
            Ok(value) => {
                check_error_code(path, &value)?;
                if !(200..300).contains(&status) {
                    return Err(AppError::HttpStatus {
                        path: path.to_string(),
                        status,
                    });
                }
                Ok(value)
            }
            Err(error) => {
                tlog!(
                    crate::logger::LEVEL_WARN,
                    "接口 {path} 返回了非 JSON 内容（HTTP {status}，前 200 字节）：{}",
                    body.chars().take(200).collect::<String>()
                );
                if !(200..300).contains(&status) {
                    return Err(AppError::HttpStatus {
                        path: path.to_string(),
                        status,
                    });
                }
                Err(AppError::Json(error))
            }
        }
    }

    /// 同 [`Self::get_json`]，但在 query 里加时间戳绕开服务端 2 分钟缓存。
    pub async fn get_json_uncached(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let mut query = query.to_vec();
        query.push(("timestamp", now_unix_millis().to_string()));
        self.get_json(path, &query).await
    }

    /// 取原始文本响应。歌词接口在 `decode=true` 下偶尔直接返回 LRC 纯文本。
    pub async fn get_text(&self, path: &str, query: &[(&str, String)]) -> Result<String> {
        let (status, body) = self.send(path, query).await?;
        if !(200..300).contains(&status) {
            return Err(AppError::HttpStatus {
                path: path.to_string(),
                status,
            });
        }
        Ok(body)
    }
}
