//! 設定のエクスポート/インポート(Issue #544)。純粋ロジックは `store::settings_transfer`。
//! ファイルの読み書きはコマンドだけが行い、中身の処理は `*_bytes` 関数に分けてテストする。

use crate::commands::mute::{apply_mute_config, read_file_bytes, write_file_bytes};
use crate::error::{Error, Result};
use crate::session::AccountManager;
use crate::state::AppState;
use crate::store::settings_transfer::{
    build_export, parse_export, plan_import, ExportFile, ImportPlan, ImportPreview, ImportResult,
};
use crate::store::settings::SettingsData;
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

/// `AppState::has_token` の、取り込み用の呼び出し。keyring の読み取り失敗は、「トークンなし」にせず
/// 取り込みを拒否するエラーにする。原因(keyring側のメッセージ)はログにだけ残す。
fn has_token_or_reject(state: &AppState, account_id: &str) -> Result<bool> {
    state.has_token(account_id).map_err(|e| {
        log::warn!("failed to read a token from the secret store while importing settings: {e}");
        Error::Secret("キーリングを読み取れないため取り込めません".into())
    })
}

/// `plan_import` に、この端末のkeyringとの照合を足したもの。ファイルにだけあるアカウントのIDが、
/// 孤立して残っているトークン(settings.jsonに無いID)と一致すると、そのトークンが許可リスト
/// 入りのホストに送られかねない。ファイルは信頼しないので、衝突したらファイルごと拒否する。
/// keyring を読めなかったときも拒否する(fail-closed, Issue #552)。「トークンなし」として通すと、
/// 後から keyring が使えるようになったとき、孤立トークンと細工したファイルのホストが結びつきうる。
fn plan_checked(state: &AppState, current: &SettingsData, file: ExportFile) -> Result<ImportPlan> {
    let mut plan = plan_import(current, file)?;
    for id in &plan.accounts_needing_login {
        if has_token_or_reject(state, id)? {
            return Err(Error::Invalid("アカウントのIDがこの端末の保存済みトークンと衝突しています".into()));
        }
    }
    // 取り込み後にトークンを持たないアカウント全体。ファイルにだけあって新しく追加されるものに加え、
    // 端末側にすでにあるアカウント(ファイルと一致したものも、端末にだけ残るものも)のうち、トークンが無い
    // ものも含める(Issue #553)。
    // 確認ダイアログの件数と、取り込み結果(ImportResult)の両方に使う。
    let mut tokenless: Vec<String> = Vec::new();
    for account in &plan.data.accounts {
        if !has_token_or_reject(state, &account.id)? {
            tokenless.push(account.id.clone());
        }
    }
    plan.preview.accounts_needing_login = tokenless.len() as i32;
    plan.accounts_needing_login = tokenless;
    Ok(plan)
}

/// 検証だけ行い、概要を返す(何も書き込まない)。
pub(crate) fn inspect_settings_bytes(state: &AppState, bytes: &[u8]) -> Result<ImportPreview> {
    let file = parse_export(bytes)?;
    Ok(plan_checked(state, &state.settings.snapshot(), file)?.preview)
}

/// 検証し、保存し、実行中の状態を更新する。保存に失敗したら何も変わらない。
pub(crate) async fn import_settings_bytes(state: &AppState, bytes: &[u8]) -> Result<ImportResult> {
    let file = parse_export(bytes)?;
    let current = state.settings.snapshot();
    let plan = plan_checked(state, &current, file)?;

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
/// keyring を読めなかったアカウントがあれば、「トークンなし」にはせずエラーにする。フロントは、この結果で
/// 通信のスキップと再認証の案内を決める。読み取りの失敗(keyring のロック、D-Bus の不通など)を「トークンなし」に
/// すると、障害のたびに全アカウントが再認証へ誘導され、通信もスキップされてしまう(Issue #553)。
pub(crate) fn accounts_without_token_ids(state: &AppState) -> Result<Vec<String>> {
    // keyring(D-Bus)の問い合わせ中にアカウント一覧のロックを持たない。
    let accounts = state.accounts.lock().unwrap().list();
    let mut ids = Vec::new();
    for account in accounts {
        if state.secrets.get(&account.id)?.is_none() {
            ids.push(account.id);
        }
    }
    Ok(ids)
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
    accounts_without_token_ids(&state)
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

    /// 端末側にあるが、トークンを持たないアカウント(以前のインポートの取り込み直後など)。
    fn state_with_tokenless_local_account() -> AppState {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.accounts.lock().unwrap().upsert(account("local1", "misskey.test", "u1"));
        state.settings.upsert_account(&account("local1", "misskey.test", "u1")).unwrap();
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
        assert_eq!(accounts_without_token_ids(&state).unwrap(), vec!["file2".to_string()]);
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
    async fn a_new_account_colliding_with_an_orphaned_keyring_token_is_rejected() {
        let state = state_with_local_account();
        // settings.json に無いIDのトークンがkeyringに残っている状態(孤立エントリ)。
        state.secrets.set("ghost", "tok").unwrap();
        let bytes = file_bytes(data_with(vec![account("ghost", "127.0.0.1", "u9")], vec![]));

        let err = import_settings_bytes(&state, &bytes).await.err().expect("拒否されるはず").to_string();
        assert!(err.contains("保存済みトークンと衝突"), "{err}");
        let err = inspect_settings_bytes(&state, &bytes).err().expect("プレビューも同じ結果").to_string();
        assert!(err.contains("保存済みトークンと衝突"), "{err}");

        assert_eq!(state.settings.load_columns().unwrap()[0].id, "old-c");
        assert_eq!(state.settings.snapshot().accounts.len(), 1);
        assert_eq!(state.accounts.lock().unwrap().list().len(), 1);
        assert!(!state.fetch_allowlist.contains_host("127.0.0.1"));
    }

    #[tokio::test]
    async fn a_rejected_file_changes_nothing() {
        let state = state_with_local_account();
        let bytes = file_bytes(data_with(vec![], vec![column("c1", "ghost-account")]));

        assert!(import_settings_bytes(&state, &bytes).await.is_err());

        assert_eq!(state.settings.load_columns().unwrap()[0].id, "old-c");
        assert_eq!(state.accounts.lock().unwrap().list().len(), 1);
    }

    /// 読み出しがいつも失敗するシークレットストア(keyring/Secret Service の障害を模す)。
    struct FailingStore;
    impl crate::session::SecretStore for FailingStore {
        fn set(&self, _: &str, _: &str) -> Result<()> {
            Err(Error::Secret("locked".into()))
        }
        fn get(&self, _: &str) -> Result<Option<String>> {
            Err(Error::Secret("locked".into()))
        }
        fn delete(&self, _: &str) -> Result<()> {
            Err(Error::Secret("locked".into()))
        }
    }

    #[tokio::test]
    async fn import_is_rejected_and_changes_nothing_when_the_secret_store_cannot_be_read() {
        // 孤立トークンとのID衝突の検査は、keyring を読めないときに素通りさせない(fail-closed, Issue #552)。
        // 読めないのを「トークンなし」にすると、後でkeyringが使えるようになったとき、孤立トークンと
        // 細工したファイルのホストが結びつきうる。
        let mut state = state_with_local_account();
        state.secrets = Box::new(FailingStore);
        let bytes = file_bytes(data_with(vec![account("ghost", "127.0.0.1", "u9")], vec![column("c1", "ghost")]));

        let err = import_settings_bytes(&state, &bytes).await.err().expect("拒否されるはず");
        assert!(matches!(err, Error::Secret(_)), "{err}");
        assert!(err.to_string().contains("キーリングを読み取れない"), "{err}");
        let err = inspect_settings_bytes(&state, &bytes).err().expect("プレビューも同じ結果");
        assert!(matches!(err, Error::Secret(_)), "{err}");

        assert_eq!(state.settings.load_columns().unwrap()[0].id, "old-c");
        assert_eq!(state.settings.snapshot().accounts.len(), 1);
        assert_eq!(state.accounts.lock().unwrap().list().len(), 1);
        assert!(!state.fetch_allowlist.contains_host("127.0.0.1"));
    }

    #[test]
    fn accounts_without_token_fails_when_the_secret_store_cannot_be_read() {
        // keyring を読めないのを「トークンなし」にすると、障害のたびに全アカウントが再認証へ誘導され、
        // 通信もスキップされる。読み取りの失敗は、トークンなしとは区別してエラーにする(Issue #553)。
        let mut state = state_with_local_account();
        state.secrets = Box::new(FailingStore);

        let err = accounts_without_token_ids(&state).err().expect("keyringを読めなければ失敗する");

        assert!(matches!(err, Error::Secret(_)), "{err}");
    }

    #[test]
    fn inspect_counts_a_matched_local_account_that_has_no_token() {
        let state = state_with_tokenless_local_account();
        // ファイル側の file1 は、端末側の local1(host+userIdが同じ)に付け替わる。
        let bytes = file_bytes(data_with(vec![account("file1", "misskey.test", "u1")], vec![]));

        let preview = inspect_settings_bytes(&state, &bytes).unwrap();

        assert_eq!(preview.accounts_needing_login, 1, "端末側でトークンが無いアカウントも数える");
    }

    #[test]
    fn inspect_counts_a_local_only_account_that_has_no_token() {
        let state = state_with_tokenless_local_account(); // local1(misskey.test / u1)
        // ファイルには別ホストのアカウントだけがある。local1 はファイルに無いまま、端末に残る。
        let bytes = file_bytes(data_with(vec![account("file1", "other.example", "u9")], vec![]));

        let preview = inspect_settings_bytes(&state, &bytes).unwrap();

        assert_eq!(
            preview.accounts_needing_login, 2,
            "端末にだけ残るトークンなしのアカウント(local1)と、新しく追加されるアカウント(file1)の両方を数える"
        );
    }

    #[test]
    fn inspect_does_not_count_a_matched_local_account_that_has_a_token() {
        let state = state_with_local_account(); // local1 はトークンあり
        let bytes = file_bytes(data_with(vec![account("file1", "misskey.test", "u1")], vec![]));

        let preview = inspect_settings_bytes(&state, &bytes).unwrap();

        assert_eq!(preview.accounts_needing_login, 0);
    }

    #[tokio::test]
    async fn import_reports_every_account_left_without_a_token() {
        let state = state_with_tokenless_local_account();
        let bytes = file_bytes(data_with(
            vec![account("file1", "misskey.test", "u1"), account("file2", "other.example", "u9")],
            vec![],
        ));

        let preview = inspect_settings_bytes(&state, &bytes).unwrap();
        let result = import_settings_bytes(&state, &bytes).await.unwrap();

        assert_eq!(preview.accounts_needing_login, 2);
        let mut ids = result.accounts_needing_login.clone();
        ids.sort();
        assert_eq!(ids, vec!["file2".to_string(), "local1".to_string()]);
        let mut actual = accounts_without_token_ids(&state).unwrap();
        actual.sort();
        assert_eq!(actual, ids, "取り込み結果は、実際にトークンの無いアカウントと一致する");
    }
}
