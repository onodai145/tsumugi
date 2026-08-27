//! ユーザープロフィール取得・フォロー操作。

use crate::api;
use crate::domain::{Account, ColumnKind, Note, User};
use crate::error::Result;
use crate::state::AppState;
use specta::Type;
use tauri::State;

/// プロフィールモーダル用のレスポンス。`is_following` は自分自身の場合 `None`。
#[derive(Debug, Clone, serde::Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub user: User,
    pub is_following: Option<bool>,
    pub is_self: bool,
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
        user: raw.user.into(),
        is_following: if is_self { None } else { raw.is_following },
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
    let kind = crate::domain::ColumnKind::User { user_id };
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
}
