//! 型付きエラー。ネットワーク/レート/権限/APIエラーを正規化し、
//! `serde` でフロントへ返せる形にする（token 等の機微情報は含めない）。

use serde::Serialize;
use specta::Type;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error, Serialize, Type)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum Error {
    /// ネットワーク到達不可・タイムアウト等
    #[error("network error: {0}")]
    Network(String),

    /// 認証失敗・トークン無効（HTTP 401 / Misskey の認証系エラー）
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// 権限不足（HTTP 403）
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// レート制限（HTTP 429）
    #[error("rate limited")]
    RateLimited,

    /// リソースが存在しない（HTTP 404 / Misskey NO_SUCH_*）
    #[error("not found: {0}")]
    NotFound(String),

    /// 上記以外の Misskey API エラー（endpoint と Misskey エラーコードを含める）
    #[error("api error: {0}")]
    Api(String),

    /// keyring 等シークレットストアの失敗
    #[error("secret store error: {0}")]
    Secret(String),

    /// 入力・状態の不整合（未知の session_id、未登録アカウント等）
    #[error("invalid: {0}")]
    Invalid(String),
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Error::Network(format!("timeout: {e}"))
        } else if e.is_connect() {
            Error::Network(format!("connect: {e}"))
        } else {
            Error::Network(e.to_string())
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Api(format!("json: {e}"))
    }
}
