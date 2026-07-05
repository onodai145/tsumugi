//! アカウント関連の Tauri command（認証・一覧・切替・削除・whoami）。
//! token は一切戻り値に含めない。

use crate::api::normalize::RawUser;
use crate::domain::{Account, User};
use crate::error::{Error, Result};
use crate::session::miauth::{build_miauth_url, check_miauth};
use crate::state::{AppState, PendingMiAuth};
use serde::Serialize;
use specta::Type;
use tauri::State;

/// `start_miauth` の戻り値。フロントは `url` をブラウザで開く。
#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MiAuthSession {
    pub url: String,
    pub session_id: String,
}

/// MiAuth を開始し、認可URLと session_id を返す。
#[tauri::command]
#[specta::specta]
pub async fn start_miauth(state: State<'_, AppState>, host: String) -> Result<MiAuthSession> {
    let host = normalize_host(&host)?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let url = build_miauth_url(&host, &session_id);
    state
        .pending
        .lock()
        .unwrap()
        .insert(session_id.clone(), PendingMiAuth { host });
    Ok(MiAuthSession { url, session_id })
}

/// ブラウザでの認可完了後に呼ぶ。token を keyring に保存し、Account を返す。
#[tauri::command]
#[specta::specta]
pub async fn complete_miauth(state: State<'_, AppState>, session_id: String) -> Result<Account> {
    // await をまたいで guard を保持しないよう host を取り出してから解錠
    let host = {
        let pending = state.pending.lock().unwrap();
        pending
            .get(&session_id)
            .map(|p| p.host.clone())
            .ok_or_else(|| Error::Invalid(format!("unknown miauth session: {session_id}")))?
    };

    let (token, raw_user) = check_miauth(&state.http, &host, &session_id).await?;

    let account = build_account(&host, &raw_user);
    // token は keyring のみに保存（Account/戻り値には含めない）
    state.secrets.set(&account.id, &token)?;
    state.accounts.lock().unwrap().upsert(account.clone());
    state.pending.lock().unwrap().remove(&session_id);
    Ok(account)
}

/// 登録済みアカウント一覧。
#[tauri::command]
#[specta::specta]
pub async fn list_accounts(state: State<'_, AppState>) -> Result<Vec<Account>> {
    Ok(state.accounts.lock().unwrap().list())
}

/// 既定アカウントを切り替える。
#[tauri::command]
#[specta::specta]
pub async fn switch_account(state: State<'_, AppState>, account_id: String) -> Result<()> {
    state.accounts.lock().unwrap().set_active(&account_id)
}

/// アカウント削除（token も keyring から消す）。
#[tauri::command]
#[specta::specta]
pub async fn remove_account(state: State<'_, AppState>, account_id: String) -> Result<()> {
    state.accounts.lock().unwrap().remove(&account_id)?;
    state.secrets.delete(&account_id)?;
    Ok(())
}

/// ログアウト（削除と同義: token 破棄 + アカウント除去）。
#[tauri::command]
#[specta::specta]
pub async fn logout(state: State<'_, AppState>, account_id: String) -> Result<()> {
    remove_account(state, account_id).await
}

/// 指定アカウントで `/i` を叩き、自分の User を返す。
#[tauri::command]
#[specta::specta]
pub async fn whoami(state: State<'_, AppState>, account_id: String) -> Result<User> {
    let client = state.client_for(&account_id)?;
    let raw: RawUser = client.post("i", &serde_json::json!({})).await?;
    Ok(raw.into())
}

/// "https://misskey.io/" や "@x@misskey.io" 混じりでもホスト名へ寄せる。
fn normalize_host(input: &str) -> Result<String> {
    let s = input.trim();
    let s = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
    let s = s.split('/').next().unwrap_or(s);
    let s = s.rsplit('@').next().unwrap_or(s); // "@user@host" -> "host"
    if s.is_empty() || !s.contains('.') {
        return Err(Error::Invalid(format!("invalid host: {input}")));
    }
    Ok(s.to_string())
}

fn build_account(host: &str, raw: &RawUser) -> Account {
    Account {
        id: uuid::Uuid::new_v4().to_string(),
        host: host.to_string(),
        username: raw.username.clone(),
        user_id: raw.id.clone(),
        display_name: raw.name.clone().unwrap_or_else(|| raw.username.clone()),
        avatar_url: raw.avatar_url.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_host_variants() {
        assert_eq!(normalize_host("misskey.io").unwrap(), "misskey.io");
        assert_eq!(normalize_host("https://misskey.io/").unwrap(), "misskey.io");
        assert_eq!(normalize_host("  http://example.tld/foo ").unwrap(), "example.tld");
        assert_eq!(normalize_host("@alice@mi.example.com").unwrap(), "mi.example.com");
        assert!(normalize_host("").is_err());
        assert!(normalize_host("localhost").is_err());
    }

    #[test]
    fn build_account_uses_name_then_username() {
        let raw: RawUser =
            serde_json::from_str(r#"{"id":"u1","username":"alice","name":"Alice A"}"#).unwrap();
        let a = build_account("misskey.io", &raw);
        assert_eq!(a.display_name, "Alice A");
        assert_eq!(a.user_id, "u1");
        assert_eq!(a.host, "misskey.io");
        assert!(!a.id.is_empty());

        let raw2: RawUser =
            serde_json::from_str(r#"{"id":"u2","username":"bob"}"#).unwrap();
        let a2 = build_account("misskey.io", &raw2);
        assert_eq!(a2.display_name, "bob"); // name 無し → username
    }
}
