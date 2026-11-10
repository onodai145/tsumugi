//! MCPサーバーの設定コマンド(Issue #557)。設定は即時反映し、サーバーを起動・停止する。
//! デスクトップ以外では「未対応」を返す(型とコマンドは全プラットフォームで登録し、バインディングを揃える)。

use crate::state::AppState;
use serde::Serialize;
use specta::Type;
use tauri::State;

#[derive(Debug, Serialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum McpRunState {
    Stopped,
    Running,
    Error { message: String },
}

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    pub port: u16,
    pub state: McpRunState,
    pub exposed_account_ids: Vec<String>,
}

#[cfg(not(desktop))]
const UNSUPPORTED: &str = "MCPサーバーはこのプラットフォームでは使えません";

#[cfg(desktop)]
fn run_state(s: &AppState) -> McpRunState {
    use crate::mcp::server::RunState;
    match s.mcp.status() {
        RunState::Stopped => McpRunState::Stopped,
        RunState::Running { .. } => McpRunState::Running,
        RunState::Error(message) => McpRunState::Error { message },
    }
}

#[cfg(not(desktop))]
fn run_state(_: &AppState) -> McpRunState {
    McpRunState::Stopped
}

pub(crate) fn status(s: &AppState) -> McpStatus {
    let m = s.settings.load_mcp();
    McpStatus { enabled: m.enabled, port: m.port, state: run_state(s), exposed_account_ids: m.exposed_account_ids }
}

/// 存在するアカウントだけ、公開の可否を切り替えられる。
pub(crate) fn set_exposed(s: &AppState, account_id: &str, exposed: bool) -> Result<(), String> {
    if !s.accounts.lock().unwrap().list().iter().any(|a| a.id == account_id) {
        return Err("アカウントが見つかりません".into());
    }
    s.settings
        .update_mcp(|m| {
            m.exposed_account_ids.retain(|id| id != account_id);
            if exposed {
                m.exposed_account_ids.push(account_id.to_string());
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub(crate) fn set_port_setting(s: &AppState, port: u16) -> Result<(), String> {
    crate::mcp::settings::validate_port(port)?;
    s.settings.update_mcp(|m| m.port = port).map(|_| ()).map_err(|e| e.to_string())
}

/// 設定画面に見せるトークン。無ければ作る。実行中のサーバーが読む共有値も揃える。
#[cfg(desktop)]
pub(crate) fn token_for_display(s: &AppState) -> Result<String, String> {
    let t = crate::mcp::token::load_or_create(s.secrets.as_ref()).map_err(|e| e.to_string())?;
    s.mcp.token.set(t.clone());
    Ok(t)
}

#[cfg(desktop)]
pub(crate) fn regenerate(s: &AppState) -> Result<String, String> {
    let t = crate::mcp::token::regenerate(s.secrets.as_ref()).map_err(|e| e.to_string())?;
    s.mcp.token.set(t.clone());
    Ok(t)
}

#[tauri::command]
#[specta::specta]
pub async fn get_mcp_status(state: State<'_, AppState>) -> Result<McpStatus, String> {
    Ok(status(&state))
}

#[tauri::command]
#[specta::specta]
pub async fn set_mcp_enabled(app: tauri::AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<McpStatus, String> {
    #[cfg(desktop)]
    {
        state.settings.update_mcp(|m| m.enabled = enabled).map_err(|e| e.to_string())?;
        if let Err(e) = crate::mcp::server::reconcile(&app).await {
            // keyring が使えない等。オンのままにせず、停止状態に戻してエラーを返す。
            let _ = state.settings.update_mcp(|m| m.enabled = false);
            let _ = crate::mcp::server::reconcile(&app).await;
            return Err(e);
        }
        Ok(status(&state))
    }
    #[cfg(not(desktop))]
    {
        let _ = (&app, &state, enabled);
        Err(UNSUPPORTED.into())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn set_mcp_port(app: tauri::AppHandle, state: State<'_, AppState>, port: u16) -> Result<McpStatus, String> {
    #[cfg(desktop)]
    {
        set_port_setting(&state, port)?;
        crate::mcp::server::reconcile(&app).await?;
        Ok(status(&state))
    }
    #[cfg(not(desktop))]
    {
        let _ = (&app, &state, port);
        Err(UNSUPPORTED.into())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn set_mcp_account_exposed(state: State<'_, AppState>, account_id: String, exposed: bool) -> Result<McpStatus, String> {
    #[cfg(desktop)]
    {
        set_exposed(&state, &account_id, exposed)?;
        Ok(status(&state))
    }
    #[cfg(not(desktop))]
    {
        let _ = (&state, &account_id, exposed);
        Err(UNSUPPORTED.into())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_mcp_token(state: State<'_, AppState>) -> Result<String, String> {
    #[cfg(desktop)]
    {
        token_for_display(&state)
    }
    #[cfg(not(desktop))]
    {
        let _ = &state;
        Err(UNSUPPORTED.into())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn regenerate_mcp_token(state: State<'_, AppState>) -> Result<String, String> {
    #[cfg(desktop)]
    {
        regenerate(&state)
    }
    #[cfg(not(desktop))]
    {
        let _ = &state;
        Err(UNSUPPORTED.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::testutil::account;
    use crate::store::SettingsStore;

    fn state() -> AppState {
        let s = AppState::new_for_test(SettingsStore::new_in_memory());
        s.accounts.lock().unwrap().upsert(account("a1", "one.example"));
        s
    }

    #[test]
    fn status_reflects_settings_and_run_state() {
        let s = state();
        s.settings.update_mcp(|m| m.exposed_account_ids = vec!["a1".into()]).unwrap();
        let st = status(&s);
        assert!(!st.enabled);
        assert_eq!(st.port, crate::mcp::settings::DEFAULT_PORT);
        assert_eq!(st.exposed_account_ids, vec!["a1".to_string()]);
        assert!(matches!(st.state, McpRunState::Stopped));
    }

    #[test]
    fn exposing_an_account_requires_it_to_exist() {
        let s = state();
        assert!(set_exposed(&s, "nope", true).is_err());
        assert!(s.settings.load_mcp().exposed_account_ids.is_empty());
        set_exposed(&s, "a1", true).unwrap();
        assert_eq!(s.settings.load_mcp().exposed_account_ids, vec!["a1".to_string()]);
        set_exposed(&s, "a1", true).unwrap();
        assert_eq!(s.settings.load_mcp().exposed_account_ids.len(), 1, "二重に入らない");
        set_exposed(&s, "a1", false).unwrap();
        assert!(s.settings.load_mcp().exposed_account_ids.is_empty());
    }

    #[test]
    fn port_below_1024_is_rejected_and_not_saved() {
        let s = state();
        assert!(set_port_setting(&s, 80).is_err());
        assert_eq!(s.settings.load_mcp().port, crate::mcp::settings::DEFAULT_PORT);
        set_port_setting(&s, 50000).unwrap();
        assert_eq!(s.settings.load_mcp().port, 50000);
    }

    #[cfg(desktop)]
    #[test]
    fn token_is_created_lazily_and_regenerate_changes_it_and_the_shared_value() {
        let s = state();
        let t1 = token_for_display(&s).unwrap();
        assert_eq!(token_for_display(&s).unwrap(), t1, "安定している");
        let t2 = regenerate(&s).unwrap();
        assert_ne!(t1, t2);
        assert_eq!(s.mcp.token.get().as_deref(), Some(t2.as_str()), "実行中のサーバーが読む値も更新される");
        assert_eq!(token_for_display(&s).unwrap(), t2);
    }
}
