//! 設定のエクスポート/インポート(Issue #544)。純粋ロジックは `store::settings_transfer`。
//! ファイルの読み書きはコマンドだけが行い、中身の処理は `*_bytes` 関数に分けてテストする。

use crate::commands::mute::{apply_mute_config, read_file_bytes, write_file_bytes};
use crate::error::Result;
use crate::session::AccountManager;
use crate::state::AppState;
use crate::store::settings_transfer::{build_export, parse_export, plan_import, ImportPreview, ImportResult};
use tauri::{AppHandle, State};

fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// 書き出すJSON(整形済み)。
pub(crate) fn export_settings_bytes(state: &AppState) -> Result<Vec<u8>> {
    let file = build_export(&state.settings.snapshot(), env!("CARGO_PKG_VERSION"), &now_rfc3339());
    Ok(serde_json::to_vec_pretty(&file)?)
}

/// 検証だけ行い、概要を返す(何も書き込まない)。
pub(crate) fn inspect_settings_bytes(state: &AppState, bytes: &[u8]) -> Result<ImportPreview> {
    let file = parse_export(bytes)?;
    Ok(plan_import(&state.settings.snapshot(), file)?.preview)
}

/// 検証し、保存し、実行中の状態を更新する。保存に失敗したら何も変わらない。
pub(crate) async fn import_settings_bytes(state: &AppState, bytes: &[u8]) -> Result<ImportResult> {
    let file = parse_export(bytes)?;
    let current = state.settings.snapshot();
    let plan = plan_import(&current, file)?;

    // 置き換え前のカラムのストリームは、保存に成功してから止める(失敗時に何も変えないため)。
    state.settings.replace(plan.data.clone())?;
    for column in &current.columns {
        state.connections.close(&column.id);
    }

    // 実行中のアカウント一覧を差し替える。選択中のアカウントは、残っていれば維持する。
    {
        let mut accounts = state.accounts.lock().unwrap();
        let previous_active = accounts.active_id().map(str::to_owned);
        let mut next = AccountManager::with_accounts(plan.data.accounts.clone());
        if let Some(id) = previous_active {
            let _ = next.set_active(&id);
        }
        *accounts = next;
    }
    state.refresh_fetch_allowlist(); // トークンなしのアカウントのホストは入らない
    // ミュート設定を反映し、カラムごとのbackfill境界を捨てる(カラムが置き換わったため)。
    apply_mute_config(state, plan.data.mute.clone()).await?;

    Ok(ImportResult {
        column_count: plan.data.columns.len() as i32,
        accounts_needing_login: plan.accounts_needing_login,
    })
}

/// トークンを持たないアカウントのID。
pub(crate) fn accounts_without_token_ids(state: &AppState) -> Vec<String> {
    state
        .accounts
        .lock()
        .unwrap()
        .list()
        .into_iter()
        .filter(|a| !state.has_token(&a.id))
        .map(|a| a.id)
        .collect()
}

/// 設定を `path` に書き出す。Androidでは `path` は `content://` URI でもよい。
#[tauri::command]
#[specta::specta]
pub async fn export_settings(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<()> {
    let bytes = export_settings_bytes(&state)?;
    write_file_bytes(&app, &path, &bytes).await
}

/// 設定ファイルを検証して概要を返す。何も書き込まない。
#[tauri::command]
#[specta::specta]
pub async fn inspect_settings_file(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<ImportPreview> {
    let bytes = read_file_bytes(&app, &path).await?;
    inspect_settings_bytes(&state, &bytes)
}

/// 設定ファイルを読み込み、現在の設定を置き換える。呼び出し元はこのあとWebViewをリロードする。
#[tauri::command]
#[specta::specta]
pub async fn import_settings(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<ImportResult> {
    let bytes = read_file_bytes(&app, &path).await?;
    import_settings_bytes(&state, &bytes).await
}

/// トークンを持たないアカウントのID一覧。UIの「要ログイン」表示に使う。
#[tauri::command]
#[specta::specta]
pub async fn accounts_without_token(state: State<'_, AppState>) -> Result<Vec<String>> {
    Ok(accounts_without_token_ids(&state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Account, Column, ColumnGroup, ColumnKind, FilterQuery};
    use crate::store::settings_transfer::{parse_export, ExportData, ExportFile, EXPORT_FORMAT, EXPORT_VERSION};
    use crate::store::SettingsStore;

    fn account(id: &str, host: &str, user_id: &str) -> Account {
        Account {
            id: id.into(),
            host: host.into(),
            username: "me".into(),
            user_id: user_id.into(),
            display_name: "Me".into(),
            avatar_url: None,
            instance: None,
            is_cat: false,
            avatar_blurhash: None,
        }
    }

    fn column(id: &str, account_id: &str) -> Column {
        Column {
            id: id.into(),
            account_id: account_id.into(),
            kind: ColumnKind::Home,
            order: 0,
            filter: FilterQuery::Keywords(vec![]),
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            group_id: "g1".into(),
            title: None,
        }
    }

    /// `register_test_account("local1")` と同じアカウント(host=misskey.test, user=u1)を、
    /// 設定ファイル側にも保存した状態にする。
    fn state_with_local_account() -> AppState {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.register_test_account("local1");
        state.settings.upsert_account(&account("local1", "misskey.test", "u1")).unwrap();
        state
            .settings
            .upsert_group(&ColumnGroup { id: "old-g".into(), order: 0, width: 300, auto: false })
            .unwrap();
        state.settings.upsert_column(&Column { group_id: "old-g".into(), ..column("old-c", "local1") }).unwrap();
        state
    }

    fn file_bytes(data: ExportData) -> Vec<u8> {
        serde_json::to_vec(&ExportFile {
            format: EXPORT_FORMAT.into(),
            version: EXPORT_VERSION,
            exported_at: "2026-11-02T00:00:00Z".into(),
            app_version: "0.0.0".into(),
            data,
        })
        .unwrap()
    }

    fn data_with(accounts: Vec<Account>, columns: Vec<Column>) -> ExportData {
        ExportData {
            accounts,
            groups: vec![ColumnGroup { id: "g1".into(), order: 0, width: 300, auto: false }],
            columns,
            mute: Default::default(),
            notify: Default::default(),
            ui: Default::default(),
            pane_layout: None,
        }
    }

    #[test]
    fn export_bytes_parse_back_and_contain_no_token() {
        let state = state_with_local_account();

        let bytes = export_settings_bytes(&state).unwrap();

        let file = parse_export(&bytes).unwrap();
        assert_eq!(file.data.accounts.len(), 1);
        assert_eq!(file.data.columns.len(), 1);
        assert!(!String::from_utf8(bytes).unwrap().contains("\"token\""), "トークンは含めない");
    }

    #[test]
    fn inspect_does_not_change_anything() {
        let state = state_with_local_account();
        let bytes = file_bytes(data_with(vec![account("f1", "other.example", "u9")], vec![column("c1", "f1")]));

        let preview = inspect_settings_bytes(&state, &bytes).unwrap();

        assert_eq!(preview.column_count, 1);
        assert_eq!(preview.accounts_needing_login, 1);
        assert_eq!(state.settings.load_columns().unwrap()[0].id, "old-c", "検証だけでは何も置き換わらない");
    }

    #[tokio::test]
    async fn import_replaces_settings_keeps_the_local_account_and_reports_new_ones() {
        let state = state_with_local_account();
        let bytes = file_bytes(data_with(
            vec![account("file1", "misskey.test", "u1"), account("file2", "other.example", "u9")],
            vec![column("c1", "file1"), column("c2", "file2")],
        ));

        let result = import_settings_bytes(&state, &bytes).await.unwrap();

        assert_eq!(result.column_count, 2);
        assert_eq!(result.accounts_needing_login, vec!["file2".to_string()]);
        let columns = state.settings.load_columns().unwrap();
        let by_id = |id: &str| columns.iter().find(|c| c.id == id).unwrap().account_id.clone();
        assert_eq!(by_id("c1"), "local1", "端末側のアカウントIDに付け替わる");
        assert_eq!(by_id("c2"), "file2");
        assert!(columns.iter().all(|c| c.id != "old-c"), "端末側のカラムは置き換わる");
        assert_eq!(state.accounts.lock().unwrap().list().len(), 2, "実行中のアカウント一覧も更新される");
        assert_eq!(accounts_without_token_ids(&state), vec!["file2".to_string()]);
    }

    #[tokio::test]
    async fn import_never_adds_hosts_of_token_less_accounts_to_the_fetch_allowlist() {
        let state = state_with_local_account();
        let bytes = file_bytes(data_with(
            vec![account("evil1", "127.0.0.1", "u8"), account("evil2", "internal.example", "u9")],
            vec![],
        ));

        import_settings_bytes(&state, &bytes).await.unwrap();

        assert!(!state.fetch_allowlist.contains_host("127.0.0.1"));
        assert!(!state.fetch_allowlist.contains_host("internal.example"));
        assert!(state.fetch_allowlist.contains_host("misskey.test"), "トークンのあるアカウントは残る");
    }

    #[tokio::test]
    async fn a_rejected_file_changes_nothing() {
        let state = state_with_local_account();
        let bytes = file_bytes(data_with(vec![], vec![column("c1", "ghost-account")]));

        assert!(import_settings_bytes(&state, &bytes).await.is_err());

        assert_eq!(state.settings.load_columns().unwrap()[0].id, "old-c");
        assert_eq!(state.accounts.lock().unwrap().list().len(), 1);
    }
}
