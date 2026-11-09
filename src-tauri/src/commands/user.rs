//! ユーザープロフィール取得・フォロー操作。

use crate::api;
use crate::domain::{Account, ColumnKind, Note, User};
use crate::error::Result;
use crate::state::AppState;
use specta::Type;
use tauri::State;

/// プロフィールモーダル用のレスポンス。関係フラグ(`is_following` / `is_muted` / `is_blocking` /
/// `is_renote_muted`)は自分自身の場合 `None`。`profile_url` はリモートユーザーの元ページ
/// (http/https のみ。ローカルユーザーや不正な値では `None`)。
#[derive(Debug, Clone, serde::Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub user: User,
    pub is_following: Option<bool>,
    pub is_muted: Option<bool>,
    pub is_blocking: Option<bool>,
    pub is_renote_muted: Option<bool>,
    pub profile_url: Option<String>,
    pub is_self: bool,
}

/// 自分自身のプロフィールでは、関係フラグ(フォロー・ミュート)を出さない(純粋関数、テスト用に分離)。
pub(crate) fn relation_flag(is_self: bool, flag: Option<bool>) -> Option<bool> {
    if is_self {
        None
    } else {
        flag
    }
}

/// リモートユーザーの元ページの URL を選ぶ(純粋関数、テスト用に分離)。`url` → `uri` の順で、最初に
/// ホストを持つ http/https として解釈できるものを返す。ローカルユーザー(`host` が `None`)では `None`。
/// 値は外部サーバーが返すので信用せず、`javascript:` / `data:` などは読み飛ばす。
pub(crate) fn sanitize_profile_url(host: Option<&str>, url: Option<&str>, uri: Option<&str>) -> Option<String> {
    host?;
    [url, uri].into_iter().flatten().find_map(|candidate| {
        let parsed = url::Url::parse(candidate).ok()?;
        let is_web = matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some();
        is_web.then(|| parsed.to_string())
    })
}

/// フォロワー/フォロー中一覧の1件。`cursor` は次ページ取得時の `until_id` に使うFollowingレコード
/// 自身のID（`user.id` ではない）。Misskeyの `users/followers` / `users/following` はFollowingレコード
/// のIDでページングするため、ユーザーIDをそのままカーソルに使うと2ページ目以降がずれる。
#[derive(Debug, Clone, serde::Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FollowListEntry {
    pub user: User,
    pub cursor: String,
}

/// `account_id` に紐づくアカウント自身のプロフィールかどうかを判定する（純粋関数、テスト用に分離）。
pub(crate) fn is_self_user(accounts: &[Account], account_id: &str, user_id: &str) -> bool {
    accounts
        .iter()
        .find(|a| a.id == account_id)
        .is_some_and(|a| a.user_id == user_id)
}

/// ユーザープロフィール（bio/バナー/フォロー関係フラグ込み）を取得する。
#[tauri::command]
#[specta::specta]
pub async fn get_user_profile(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
) -> Result<UserProfile> {
    let client = state.client_for(&account_id)?;
    let raw = api::users::show(&client, &user_id).await?;
    let accounts = state.accounts.lock().unwrap().list();
    let is_self = is_self_user(&accounts, &account_id, &user_id);
    Ok(UserProfile {
        is_following: relation_flag(is_self, raw.is_following),
        is_muted: relation_flag(is_self, raw.is_muted),
        is_blocking: relation_flag(is_self, raw.is_blocking),
        is_renote_muted: relation_flag(is_self, raw.is_renote_muted),
        profile_url: sanitize_profile_url(raw.user.host.as_deref(), raw.url.as_deref(), raw.uri.as_deref()),
        user: raw.user.into(),
        is_self,
    })
}

/// フォローする。
#[tauri::command]
#[specta::specta]
pub async fn follow_user(state: State<'_, AppState>, account_id: String, user_id: String) -> Result<()> {
    let client = state.client_for(&account_id)?;
    api::users::follow(&client, &user_id).await
}

/// フォロー解除する。
#[tauri::command]
#[specta::specta]
pub async fn unfollow_user(state: State<'_, AppState>, account_id: String, user_id: String) -> Result<()> {
    let client = state.client_for(&account_id)?;
    api::users::unfollow(&client, &user_id).await
}

/// プロフィールモーダルに埋め込むノート一覧。既存の `ColumnKind::User` と同じ
/// `users/notes` エンドポイントを使う（カラムとして常設せず、都度取得する）。
#[tauri::command]
#[specta::specta]
pub async fn get_user_notes(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
    until_id: Option<String>,
) -> Result<Vec<Note>> {
    let client = state.client_for(&account_id)?;
    let kind = ColumnKind::User { user_id };
    let (endpoint, body) = kind.rest_request(20, until_id.as_deref()).expect("User kind always has rest_request");
    api::notes::fetch_notes(&client, endpoint, &body).await
}

/// フォロワー一覧（ページング）。
#[tauri::command]
#[specta::specta]
pub async fn get_user_followers(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
    until_id: Option<String>,
) -> Result<Vec<FollowListEntry>> {
    let client = state.client_for(&account_id)?;
    let entries = api::users::followers(&client, &user_id, until_id.as_deref()).await?;
    Ok(entries.into_iter().map(|(user, cursor)| FollowListEntry { user, cursor }).collect())
}

/// フォロー中一覧（ページング）。
#[tauri::command]
#[specta::specta]
pub async fn get_user_following(
    state: State<'_, AppState>,
    account_id: String,
    user_id: String,
    until_id: Option<String>,
) -> Result<Vec<FollowListEntry>> {
    let client = state.client_for(&account_id)?;
    let entries = api::users::following(&client, &user_id, until_id.as_deref()).await?;
    Ok(entries.into_iter().map(|(user, cursor)| FollowListEntry { user, cursor }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_account(id: &str, user_id: &str) -> Account {
        Account {
            id: id.to_string(),
            host: "misskey.example".to_string(),
            username: "alice".to_string(),
            user_id: user_id.to_string(),
            display_name: "Alice".to_string(),
            avatar_url: None,
            instance: None,
            is_cat: false,
            avatar_blurhash: None,
        }
    }

    #[test]
    fn is_self_user_true_when_user_id_matches_account() {
        let accounts = vec![make_account("acc1", "u1"), make_account("acc2", "u2")];
        assert!(is_self_user(&accounts, "acc1", "u1"));
    }

    #[test]
    fn is_self_user_false_when_user_id_differs() {
        let accounts = vec![make_account("acc1", "u1")];
        assert!(!is_self_user(&accounts, "acc1", "u2"));
    }

    #[test]
    fn is_self_user_false_when_account_id_unknown() {
        let accounts = vec![make_account("acc1", "u1")];
        assert!(!is_self_user(&accounts, "unknown", "u1"));
    }

    #[test]
    fn relation_flag_is_hidden_for_the_own_profile() {
        assert_eq!(relation_flag(true, Some(true)), None);
        assert_eq!(relation_flag(false, Some(true)), Some(true));
        assert_eq!(relation_flag(false, Some(false)), Some(false));
        assert_eq!(relation_flag(false, None), None);
    }

    #[test]
    fn profile_url_is_none_for_local_users() {
        assert_eq!(
            sanitize_profile_url(None, Some("https://misskey.example/@alice"), None),
            None
        );
    }

    #[test]
    fn profile_url_prefers_url_over_uri() {
        assert_eq!(
            sanitize_profile_url(
                Some("remote.example"),
                Some("https://remote.example/@alice"),
                Some("https://remote.example/users/alice")
            ),
            Some("https://remote.example/@alice".to_string())
        );
    }

    #[test]
    fn profile_url_falls_back_to_uri_when_url_is_missing_or_unsafe() {
        let uri = Some("https://remote.example/users/alice");
        assert_eq!(
            sanitize_profile_url(Some("remote.example"), None, uri),
            Some("https://remote.example/users/alice".to_string())
        );
        assert_eq!(
            sanitize_profile_url(Some("remote.example"), Some("javascript:alert(1)"), uri),
            Some("https://remote.example/users/alice".to_string())
        );
    }

    #[test]
    fn profile_url_rejects_non_http_schemes_and_garbage() {
        for bad in ["javascript:alert(1)", "data:text/html,<b>x</b>", "file:///etc/passwd", "", "not a url", "https://"] {
            assert_eq!(
                sanitize_profile_url(Some("remote.example"), Some(bad), Some(bad)),
                None,
                "{bad:?} は採用しない"
            );
        }
    }
}
