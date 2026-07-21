//! MiAuth フロー。インスタンスURLとトークンを取得する。
//! 参考: Misskey Hub「MiAuth」。GET `/miauth/<session>?name=..&permission=..` を
//! ブラウザで開いて認可 → POST `/api/miauth/<session>/check` で token+user を得る。

use crate::api::normalize::RawUser;
use crate::error::{Error, Result};
use serde::Deserialize;

/// MVP で要求する権限スコープ（過不足なく最小限に寄せる）。
pub const PERMISSIONS: &[&str] = &[
    "read:account",
    "read:notifications",
    "read:following",
    "read:mutes",
    "read:blocks",
    "read:channels",
    "write:notes",
    "write:reactions",
    "write:following",
    "write:votes",
    "write:drive",
    "read:drive",
    "write:favorites",
    "write:account",
];

pub const APP_NAME: &str = "tsumugi";

/// 認可URL（ブラウザで開く）を組み立てる。callback は使わず、ユーザが認可後に
/// アプリへ戻り `check` を叩く運用（デスクトップのローカルサーバ不要）。
pub fn build_miauth_url(host: &str, session_id: &str) -> String {
    let permission = PERMISSIONS.join(",");
    let mut url = url::Url::parse(&format!("https://{host}/miauth/{session_id}"))
        .expect("host/session are validated upstream");
    url.query_pairs_mut()
        .append_pair("name", APP_NAME)
        .append_pair("permission", &permission);
    url.to_string()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckResponse {
    ok: bool,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    user: Option<RawUser>,
}

/// 認可済みかを確認し、成功していれば (token, user) を返す。
/// まだ認可されていない場合は `ok=false` が返るため [`Error::Unauthorized`] にする。
pub async fn check_miauth(
    http: &reqwest::Client,
    host: &str,
    session_id: &str,
) -> Result<(String, RawUser)> {
    let url = format!("https://{host}/api/miauth/{session_id}/check");
    let resp = http.post(&url).json(&serde_json::json!({})).send().await?;
    if !resp.status().is_success() {
        return Err(Error::Api(format!(
            "miauth check: HTTP {}",
            resp.status().as_u16()
        )));
    }
    let body: CheckResponse = resp.json().await.map_err(Error::from)?;
    match (body.ok, body.token, body.user) {
        (true, Some(token), Some(user)) => Ok((token, user)),
        (true, _, _) => Err(Error::Api(
            "miauth check ok but token/user missing".into(),
        )),
        (false, _, _) => Err(Error::Unauthorized(
            "authorization not completed in browser".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_url_with_name_and_permissions() {
        let u = build_miauth_url("misskey.io", "11111111-2222-3333-4444-555555555555");
        assert!(u.starts_with("https://misskey.io/miauth/11111111-2222-3333-4444-555555555555?"));
        assert!(u.contains("name=tsumugi"));
        // permission はカンマが URL エンコードされる（%2C）
        assert!(u.contains("write%3Anotes") || u.contains("write:notes"));
        assert!(u.contains("read%3Aaccount") || u.contains("read:account"));
    }

    #[test]
    fn check_response_deserializes() {
        let r: CheckResponse = serde_json::from_str(
            r#"{"ok":true,"token":"abc","user":{"id":"u1","username":"me"}}"#,
        )
        .unwrap();
        assert!(r.ok);
        assert_eq!(r.token.as_deref(), Some("abc"));
        assert_eq!(r.user.unwrap().username, "me");
    }
}
