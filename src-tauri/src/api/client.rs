use crate::error::{Error, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

/// 1 インスタンス + 1 トークンに対する REST クライアント。
///
/// token は本 struct 内に保持し、リクエストボディの `i` フィールドとしてのみ送出する。
/// フロントには渡さない（`Debug` でも伏せる）。
#[derive(Clone)]
pub struct MisskeyClient {
    host: String,
    api_base: String,
    http: reqwest::Client,
    token: Option<String>,
}

impl std::fmt::Debug for MisskeyClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MisskeyClient")
            .field("host", &self.host)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl MisskeyClient {
    /// `host` は "misskey.io" のようなホスト名（スキーム無し）。
    pub fn new(http: reqwest::Client, host: impl Into<String>, token: Option<String>) -> Self {
        let host = host.into();
        let api_base = format!("https://{host}/api");
        Self {
            host,
            api_base,
            http,
            token,
        }
    }

    #[allow(dead_code)] // Phase 2: Streaming 接続で host を参照
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Misskey の任意エンドポイントを POST する。`body` は JSON オブジェクトに
    /// シリアライズできる必要がある（`i` を差し込むため）。
    pub async fn post<B, R>(&self, endpoint: &str, body: &B) -> Result<R>
    where
        B: Serialize,
        R: DeserializeOwned,
    {
        let value = self.build_body(body)?;
        let url = format!("{}/{}", self.api_base, endpoint.trim_start_matches('/'));
        let resp = self.http.post(&url).json(&value).send().await?;
        let status = resp.status();

        if status.is_success() {
            // 204 No Content 等、本文が空なら null として扱う
            let bytes = resp.bytes().await?;
            if bytes.is_empty() {
                return Ok(serde_json::from_value(Value::Null)?);
            }
            return Ok(serde_json::from_slice(&bytes)?);
        }

        Err(Self::map_error(endpoint, status, resp.text().await.ok()))
    }

    /// `body`（任意の Serialize）を JSON オブジェクト化し、token があれば `i` を差し込む。
    fn build_body<B: Serialize>(&self, body: &B) -> Result<Value> {
        let mut value = serde_json::to_value(body)?;
        match &mut value {
            Value::Object(map) => {
                if let Some(token) = &self.token {
                    map.insert("i".to_string(), json!(token));
                }
            }
            Value::Null => {
                // 引数なしのエンドポイント。token があれば {"i": ...} を作る
                let mut map = serde_json::Map::new();
                if let Some(token) = &self.token {
                    map.insert("i".to_string(), json!(token));
                }
                value = Value::Object(map);
            }
            _ => {
                return Err(Error::Invalid(
                    "request body must serialize to a JSON object".into(),
                ))
            }
        }
        Ok(value)
    }

    /// HTTP ステータス + Misskey エラーボディを型付き [`Error`] へ正規化する。
    fn map_error(endpoint: &str, status: reqwest::StatusCode, body: Option<String>) -> Error {
        // Misskey は { "error": { "code", "message", "id" } } を返す
        let (code, message) = body
            .as_deref()
            .and_then(|b| serde_json::from_str::<Value>(b).ok())
            .and_then(|v| v.get("error").cloned())
            .map(|e| {
                let code = e
                    .get("code")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();
                let msg = e
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_string();
                (code, msg)
            })
            .unwrap_or_default();

        let detail = format!("{endpoint}: {code} {message}").trim().to_string();
        match status.as_u16() {
            401 => Error::Unauthorized(detail),
            403 => Error::Forbidden(detail),
            404 => Error::NotFound(detail),
            429 => Error::RateLimited,
            _ => {
                // 認証系は Misskey ではしばしば 400 + CREDENTIAL_REQUIRED 等で返る
                if code.contains("CREDENTIAL") || code.contains("AUTHENTICATION") {
                    Error::Unauthorized(detail)
                } else {
                    Error::Api(detail)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Args {
        limit: u32,
    }

    fn client(token: Option<&str>) -> MisskeyClient {
        MisskeyClient::new(
            reqwest::Client::new(),
            "misskey.example",
            token.map(|t| t.to_string()),
        )
    }

    #[test]
    fn injects_token_into_object_body() {
        let c = client(Some("secret-token"));
        let v = c.build_body(&Args { limit: 10 }).unwrap();
        assert_eq!(v["limit"], 10);
        assert_eq!(v["i"], "secret-token");
    }

    #[test]
    fn no_token_leaves_body_untouched() {
        let c = client(None);
        let v = c.build_body(&Args { limit: 10 }).unwrap();
        assert_eq!(v["limit"], 10);
        assert!(v.get("i").is_none());
    }

    #[test]
    fn null_body_becomes_object_with_token() {
        let c = client(Some("t"));
        let v = c.build_body(&serde_json::Value::Null).unwrap();
        assert_eq!(v["i"], "t");
    }

    #[test]
    fn non_object_body_is_rejected() {
        let c = client(Some("t"));
        let err = c.build_body(&json!([1, 2, 3])).unwrap_err();
        assert!(matches!(err, Error::Invalid(_)));
    }

    #[test]
    fn debug_redacts_token() {
        let c = client(Some("super-secret"));
        let dbg = format!("{c:?}");
        assert!(!dbg.contains("super-secret"), "token leaked in Debug: {dbg}");
        assert!(dbg.contains("redacted"));
    }

    /// 実 Misskey に対する疎通確認（token 無しで /i → 認証エラーになること）。
    /// ネットワーク依存のため既定では実行しない: `cargo test -- --ignored real_misskey`
    #[ignore]
    #[tokio::test]
    async fn real_misskey_i_without_token_is_unauthorized() {
        let c = MisskeyClient::new(reqwest::Client::new(), "misskey.io", None);
        let res: Result<serde_json::Value> = c.post("i", &json!({})).await;
        match res {
            Err(Error::Unauthorized(_)) => {}
            other => panic!("expected Unauthorized, got {other:?}"),
        }
    }

    #[test]
    fn maps_status_to_typed_error() {
        let e = MisskeyClient::map_error(
            "i",
            reqwest::StatusCode::UNAUTHORIZED,
            Some(r#"{"error":{"code":"AUTHENTICATION_FAILED","message":"bad"}}"#.into()),
        );
        assert!(matches!(e, Error::Unauthorized(_)));

        let e = MisskeyClient::map_error("notes/create", reqwest::StatusCode::TOO_MANY_REQUESTS, None);
        assert!(matches!(e, Error::RateLimited));

        let e = MisskeyClient::map_error(
            "notes/show",
            reqwest::StatusCode::BAD_REQUEST,
            Some(r#"{"error":{"code":"NO_SUCH_NOTE","message":"nope"}}"#.into()),
        );
        assert!(matches!(e, Error::Api(_)));

        // 400 でも認証系コードなら Unauthorized に寄せる
        let e = MisskeyClient::map_error(
            "i",
            reqwest::StatusCode::BAD_REQUEST,
            Some(r#"{"error":{"code":"CREDENTIAL_REQUIRED","message":"x"}}"#.into()),
        );
        assert!(matches!(e, Error::Unauthorized(_)));
    }
}
