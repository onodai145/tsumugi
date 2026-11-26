//! アカウント関連の Tauri command（認証・一覧・切替・削除・whoami）。
//! token は一切戻り値に含めない。

use crate::api::normalize::RawUser;
use crate::domain::{Account, User};
use crate::error::{Error, Result};
use crate::session::miauth::{build_miauth_url, check_miauth};
use crate::state::{AppState, PendingMiAuth};
use crate::store::settings_transfer::is_valid_account_host;
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
    let url = build_miauth_url(&host, &session_id)?;
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

    // 同一 host+user_id の既存アカウントがあれば再認証とみなし、id を維持する
    // （MiAuth はトークン発行後にスコープを追加できないため、権限不足時は再認証で
    // トークンだけ差し替える。id が変わるとカラム/グループの紐付けが失われる）。
    let existing_id = state
        .accounts
        .lock()
        .unwrap()
        .find_by_host_user(&host, &raw_user.id)
        .map(|a| a.id.clone());

    let account = build_account(existing_id, &host, &raw_user);
    persist_login(&state, &account, &token)?;
    state.accounts.lock().unwrap().upsert(account.clone());
    state.refresh_fetch_allowlist();
    state.pending.lock().unwrap().remove(&session_id);
    Ok(account)
}

/// token を keyring に、アカウントを設定に書く。設定の保存に失敗したら、keyring を書く前の状態に戻す
/// (残すと、どのアカウントにも結びつかない token ができる。Issue #696)。
fn persist_login(state: &AppState, account: &Account, token: &str) -> Result<()> {
    // 戻せるように、書く前に控える。読めなければ何も書かない。
    let previous = state.secrets.get(&account.id)?;
    // token は keyring のみに保存（Account/戻り値には含めない）
    state.secrets.set(&account.id, token)?;
    // 再起動で復元できるよう永続化
    state.settings.upsert_account(account).inspect_err(|_| {
        let restored = match &previous {
            Some(previous) => state.secrets.set(&account.id, previous),
            None => state.secrets.delete(&account.id),
        };
        if let Err(e) = restored {
            log::warn!("failed to restore the token after the login failed: {e}");
        }
    })
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
    remove_account_core(&state, &account_id).await
}

/// `remove_account` の本体。`State` を引数に取らない形にして、テストできるようにしている。
///
/// エラーを返すなら何も変えない。確定点(`SettingsStore::delete_account`)の後は、`?` で抜けずに
/// 後始末を最後まで行う(Issue #694)。
async fn remove_account_core(state: &AppState, account_id: &str) -> Result<()> {
    if state.accounts.lock().unwrap().get(account_id).is_none() {
        return Err(Error::Invalid(format!("unknown account: {account_id}")));
    }
    // 確定後は設定から引けないので、先に控える。
    let column_ids: Vec<String> = state
        .settings
        .load_columns()?
        .into_iter()
        .filter(|c| c.account_id == account_id)
        .map(|c| c.id)
        .collect();

    {
        // トークンが一時的に消えている間は、スケジューラに読ませない。この中で `await` しない。
        let _removing = state.token_removal.write().unwrap();
        // トークンは確定点より前に消す。後だと、失敗したときに、画面から消し直せないトークンが残る。
        let token = state.secrets.get(account_id)?;
        state.secrets.delete(account_id)?;

        // 確定点: アカウントとカラムを永続層から削除する。
        if let Err(e) = state.settings.delete_account(account_id) {
            if let Some(token) = &token {
                if let Err(restore) = state.secrets.set(account_id, token) {
                    log::warn!("failed to restore the token after the account removal failed: {restore}");
                }
            }
            return Err(e);
        }
    }

    for column_id in &column_ids {
        state.connections.close(column_id);
    }
    // 存在は先頭で確かめた。同時に走った削除が先に消していても、失敗にはしない。
    let _ = state.accounts.lock().unwrap().remove(account_id);
    state.refresh_fetch_allowlist();
    state.forget_server_version(account_id);
    state.forget_translator_available(account_id);
    state.forget_server_mute_sync_lock(account_id);
    // そのアカウントのノート帰属も消す(Issue #557)。
    crate::attribution::forget(&state.cache, account_id).await;
    // そのアカウントのクライアント側予約(Issue #60 B)も消す。送信先が無くなるため。
    if let Err(e) = state.scheduled_posts.remove_account(account_id) {
        log::warn!("failed to remove the scheduled posts of the removed account: {e}");
    }
    // そのアカウントのカラムが全て消えたことで空になったグループと、ペイン分割ツリー上の
    // 対応するLeafを掃除する(Issue #31)。
    if let Err(e) = state.settings.delete_empty_groups() {
        log::warn!("failed to remove the empty groups after removing an account: {e}");
    }
    // 実行中の取得が上の `forget` の後に完了して再記録した分も消す(競合の窓を狭める。完全には塞げない)。
    crate::attribution::forget(&state.cache, account_id).await;
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

/// 接続先インスタンスの `/api/meta` を取得し、Account.instance を更新して返す
/// （Instance Ticker用、Issue #103）。boot時にフロントから全アカウント分呼ばれる想定。
#[tauri::command]
#[specta::specta]
pub async fn refresh_instance_meta(state: State<'_, AppState>, account_id: String) -> Result<Account> {
    let client = state.client_for(&account_id)?;
    let info = crate::api::meta::fetch_meta(&client).await?;
    let account = state.accounts.lock().unwrap().update_instance(&account_id, Some(info))?;
    state.settings.upsert_account(&account)?;
    Ok(account)
}

/// "https://misskey.io/" や "@x@misskey.io" 混じりでもホスト名へ寄せる。
fn normalize_host(input: &str) -> Result<String> {
    let s = input.trim();
    let s = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
    let s = s.split('/').next().unwrap_or(s);
    let s = s.rsplit('@').next().unwrap_or(s); // "@user@host" -> "host"
    // 厳密な検証は設定インポートと共通(Issue #610)。ポート付きは通す。`.` を要求するのは、
    // `localhost` などの単一ラベルと IPv6 リテラルを弾くため(net_guard はこれに依存する)。
    let name = s.split(':').next().unwrap_or(s);
    if !name.contains('.') || !is_valid_account_host(s) {
        return Err(Error::Invalid(format!("invalid host: {input}")));
    }
    Ok(s.to_string())
}

fn build_account(existing_id: Option<String>, host: &str, raw: &RawUser) -> Account {
    Account {
        id: existing_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        host: host.to_string(),
        username: raw.username.clone(),
        user_id: raw.id.clone(),
        display_name: raw.name.clone().unwrap_or_else(|| raw.username.clone()),
        avatar_url: raw.avatar_url.clone(),
        instance: None,
        is_cat: raw.is_cat,
        avatar_blurhash: raw.avatar_blurhash.clone(),
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
    fn normalize_host_accepts_port() {
        assert_eq!(normalize_host("https://mi.example.com:8443/").unwrap(), "mi.example.com:8443");
    }

    #[test]
    fn normalize_host_rejects_unparsable_hosts() {
        // Issue #610: `.` を含むだけの入力を通すと、後段の `Url::parse` が失敗する。
        for bad in ["mis key.io", "misskey.io:abc", "misskey.io|x", "misskey.io:0", "misskey.io:99999", "a..b", ".misskey.io", "misskey.io."] {
            assert!(normalize_host(bad).is_err(), "{bad} が通ってしまった");
        }
    }

    #[test]
    fn build_account_uses_name_then_username() {
        let raw: RawUser =
            serde_json::from_str(r#"{"id":"u1","username":"alice","name":"Alice A"}"#).unwrap();
        let a = build_account(None, "misskey.io", &raw);
        assert_eq!(a.display_name, "Alice A");
        assert_eq!(a.user_id, "u1");
        assert_eq!(a.host, "misskey.io");
        assert!(!a.id.is_empty());

        let raw2: RawUser =
            serde_json::from_str(r#"{"id":"u2","username":"bob"}"#).unwrap();
        let a2 = build_account(None, "misskey.io", &raw2);
        assert_eq!(a2.display_name, "bob"); // name 無し → username
    }

    #[test]
    fn build_account_reuses_existing_id_when_given() {
        let raw: RawUser =
            serde_json::from_str(r#"{"id":"u1","username":"alice","name":"Alice A"}"#).unwrap();
        let a = build_account(Some("existing-id".into()), "misskey.io", &raw);
        assert_eq!(a.id, "existing-id");
    }

    #[test]
    fn build_account_carries_is_cat_and_avatar_blurhash() {
        let raw: RawUser = serde_json::from_str(
            r#"{"id":"u1","username":"alice","isCat":true,"avatarBlurhash":"LEHV6nWB2yk8pyo0adR*.7kCMdnj"}"#,
        )
        .unwrap();
        let a = build_account(None, "misskey.io", &raw);
        assert!(a.is_cat);
        assert_eq!(a.avatar_blurhash.as_deref(), Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj"));
    }
}

#[cfg(test)]
mod removal_tests {
    use super::*;
    use crate::domain::{Column, ColumnGroup, ColumnKind, FilterQuery, LocalScheduleStatus};
    use crate::session::{MemoryStore, SecretStore};
    use crate::store::scheduled_post::test_input;
    use crate::store::testutil::BrokenDisk;
    use crate::store::{ScheduledPostStore, SettingsStore};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    type Hook = (&'static str, Box<dyn FnOnce() + Send>);

    /// 立てた操作だけが失敗する。`on_get` は、そのアカウントのトークンを次に読むときに 1 回だけ呼ぶ。
    /// `after_delete` は、そのアカウントのトークンを次に消した直後に 1 回だけ呼ぶ。
    #[derive(Default)]
    struct SecretFaults {
        get: AtomicBool,
        set: AtomicBool,
        delete: AtomicBool,
        on_get: Mutex<Option<Hook>>,
        after_delete: Mutex<Option<Hook>>,
    }

    struct FaultySecrets {
        inner: MemoryStore,
        faults: Arc<SecretFaults>,
    }

    fn fault(flag: &AtomicBool) -> Result<()> {
        if flag.load(Ordering::SeqCst) {
            return Err(Error::Secret("injected failure".into()));
        }
        Ok(())
    }

    fn run_hook(slot: &Mutex<Option<Hook>>, account_id: &str) {
        let hook = slot.lock().unwrap().take_if(|(id, _)| *id == account_id);
        if let Some((_, run)) = hook {
            run();
        }
    }

    impl SecretStore for FaultySecrets {
        fn set(&self, account_id: &str, token: &str) -> Result<()> {
            fault(&self.faults.set)?;
            self.inner.set(account_id, token)
        }

        fn get(&self, account_id: &str) -> Result<Option<String>> {
            fault(&self.faults.get)?;
            run_hook(&self.faults.on_get, account_id);
            self.inner.get(account_id)
        }

        fn delete(&self, account_id: &str) -> Result<()> {
            fault(&self.faults.delete)?;
            self.inner.delete(account_id)?;
            run_hook(&self.faults.after_delete, account_id);
            Ok(())
        }
    }

    /// アカウント `a1` と `a2`。それぞれ、グループ1つ、通知カラム1つ、予約1件、ストリーム1本、トークンを持つ。
    struct Fixture {
        state: AppState,
        faults: Arc<SecretFaults>,
        settings_disk: Arc<BrokenDisk>,
        scheduled_disk: BrokenDisk,
        _app: tauri::App<tauri::test::MockRuntime>,
    }

    fn account(id: &str, host: &str) -> Account {
        Account {
            id: id.into(),
            host: host.into(),
            username: id.into(),
            user_id: format!("u-{id}"),
            display_name: id.into(),
            avatar_url: None,
            instance: None,
            is_cat: false,
            avatar_blurhash: None,
        }
    }

    fn column(id: &str, account_id: &str, group_id: &str) -> Column {
        Column {
            id: id.into(),
            account_id: account_id.into(),
            kind: ColumnKind::Notifications,
            order: 0,
            filter: FilterQuery::Keywords(vec![]),
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            speech: Default::default(),
            group_id: group_id.into(),
            title: None,
        }
    }

    fn fixture() -> Fixture {
        // 接続先は、すぐに拒否されるローカルのポート。
        const ACCOUNTS: [(&str, &str, &str, &str); 2] = [
            ("a1", "127.0.0.1:1", "c1", "g1"),
            ("a2", "127.0.0.1:2", "c2", "g2"),
        ];
        let settings_disk = Arc::new(BrokenDisk::new());
        let scheduled_disk = BrokenDisk::new();
        let settings = SettingsStore::new(
            settings_disk.path("settings.json"),
            settings_disk.path("backgrounds"),
        )
        .unwrap();
        for (order, (id, host, column_id, group_id)) in ACCOUNTS.iter().enumerate() {
            settings.upsert_account(&account(id, host)).unwrap();
            settings
                .upsert_group(&ColumnGroup { id: (*group_id).into(), order: order as i32, width: 320, auto: false })
                .unwrap();
            settings.upsert_column(&column(column_id, id, group_id)).unwrap();
        }

        let scheduled =
            ScheduledPostStore::new(scheduled_disk.path("scheduled_posts.json")).unwrap();
        let mut state = AppState::new_for_test(settings).with_scheduled_posts(scheduled);
        let faults = Arc::new(SecretFaults::default());
        state.secrets = Box::new(FaultySecrets { inner: MemoryStore::default(), faults: faults.clone() });

        let app = tauri::test::mock_app();
        for (id, host, column_id, _) in ACCOUNTS {
            state.secrets.set(id, "token").unwrap();
            state.scheduled_posts.add(id, 4_000_000_000, test_input("later"), 0).unwrap();
            state.connections.open_notifications(
                app.handle().clone(),
                column_id.into(),
                id.into(),
                host.into(),
                "token".into(),
                None,
            );
        }
        assert_eq!(state.connections.open_count(), 2);

        Fixture { state, faults, settings_disk, scheduled_disk, _app: app }
    }

    fn has_account(f: &Fixture, id: &str) -> (bool, bool) {
        (
            f.state.accounts.lock().unwrap().get(id).is_some(),
            f.state.settings.load_accounts().unwrap().iter().any(|a| a.id == id),
        )
    }

    fn column_ids(f: &Fixture) -> Vec<String> {
        let mut ids: Vec<String> =
            f.state.settings.load_columns().unwrap().into_iter().map(|c| c.id).collect();
        ids.sort();
        ids
    }

    /// トークン以外が、削除前のまま。
    fn assert_untouched(f: &Fixture) {
        assert_eq!(has_account(f, "a1"), (true, true), "(メモリ, 設定)");
        assert_eq!(column_ids(f), ["c1", "c2"]);
        assert_eq!(f.state.scheduled_posts.list_for_account("a1").len(), 1);
        assert_eq!(f.state.connections.open_count(), 2);
    }

    fn token(f: &Fixture, id: &str) -> Option<String> {
        f.state.secrets.get(id).unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn removes_everything_of_that_account_only() {
        let f = fixture();

        remove_account_core(&f.state, "a1").await.unwrap();

        assert_eq!(has_account(&f, "a1"), (false, false));
        assert_eq!(column_ids(&f), ["c2"]);
        let groups: Vec<String> =
            f.state.settings.load_groups().unwrap().into_iter().map(|g| g.id).collect();
        assert_eq!(groups, ["g2"]);
        assert_eq!(token(&f, "a1"), None);
        assert!(f.state.scheduled_posts.list_for_account("a1").is_empty());
        assert_eq!(f.state.connections.open_count(), 1);

        assert_eq!(has_account(&f, "a2"), (true, true));
        assert_eq!(token(&f, "a2").as_deref(), Some("token"));
        assert_eq!(f.state.scheduled_posts.list_for_account("a2").len(), 1);
        // 残った 1 本が a2 のものなら、a2 のカラムを閉じると 0 本になる。
        f.state.connections.close("c2");
        assert_eq!(f.state.connections.open_count(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn keyring_delete_failure_changes_nothing() {
        let f = fixture();
        f.faults.delete.store(true, Ordering::SeqCst);

        let err = remove_account_core(&f.state, "a1").await.unwrap_err();

        assert!(matches!(err, Error::Secret(_)), "{err}");
        assert_untouched(&f);
        assert_eq!(token(&f, "a1").as_deref(), Some("token"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn keyring_read_failure_changes_nothing() {
        let f = fixture();
        f.faults.get.store(true, Ordering::SeqCst);

        let err = remove_account_core(&f.state, "a1").await.unwrap_err();

        assert!(matches!(err, Error::Secret(_)), "{err}");
        assert_untouched(&f);
        f.faults.get.store(false, Ordering::SeqCst);
        assert_eq!(token(&f, "a1").as_deref(), Some("token"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn settings_save_failure_restores_the_token_and_a_retry_succeeds() {
        let f = fixture();
        f.settings_disk.break_dir();

        assert!(remove_account_core(&f.state, "a1").await.is_err());

        assert_untouched(&f);
        assert_eq!(token(&f, "a1").as_deref(), Some("token"));

        f.settings_disk.repair_dir();
        remove_account_core(&f.state, "a1").await.unwrap();
        assert_eq!(has_account(&f, "a1"), (false, false));
        assert_eq!(token(&f, "a1"), None);
        assert_eq!(f.state.connections.open_count(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_token_restore_still_reports_the_settings_error() {
        let f = fixture();
        f.settings_disk.break_dir();
        f.faults.set.store(true, Ordering::SeqCst);

        let err = remove_account_core(&f.state, "a1").await.unwrap_err();

        assert!(!matches!(err, Error::Secret(_)), "{err}");
        assert_untouched(&f);
        assert_eq!(token(&f, "a1"), None, "書き戻せないので、要ログインの状態で残る");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_account_without_a_token_gets_none_on_failure_and_can_be_removed() {
        let f = fixture();
        f.state.secrets.delete("a1").unwrap();
        f.settings_disk.break_dir();

        assert!(remove_account_core(&f.state, "a1").await.is_err());
        assert_untouched(&f);
        assert_eq!(token(&f, "a1"), None);

        f.settings_disk.repair_dir();
        remove_account_core(&f.state, "a1").await.unwrap();
        assert_eq!(has_account(&f, "a1"), (false, false));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_scheduled_post_save_failure_after_the_commit_still_succeeds() {
        let f = fixture();
        f.scheduled_disk.break_dir();

        remove_account_core(&f.state, "a1").await.unwrap();

        assert_eq!(has_account(&f, "a1"), (false, false));
        assert_eq!(column_ids(&f), ["c2"]);
        assert_eq!(token(&f, "a1"), None);
        assert_eq!(f.state.connections.open_count(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_empty_group_cleanup_failure_after_the_commit_still_succeeds() {
        let f = fixture();
        let disk = f.settings_disk.clone();
        // 確定後の `refresh_fetch_allowlist` が、残るアカウント(a2)のトークンを読むところで、設定の保存先を壊す。
        *f.faults.on_get.lock().unwrap() = Some(("a2", Box::new(move || disk.break_dir())));

        remove_account_core(&f.state, "a1").await.unwrap();

        assert!(!f.settings_disk.path("settings.json").exists(), "確定の後で保存先が壊れた");
        assert_eq!(has_account(&f, "a1"), (false, false));
        assert!(f.state.scheduled_posts.list_for_account("a1").is_empty());
        assert_eq!(f.state.connections.open_count(), 1);
    }

    /// 削除の途中(トークンを消してから、確定に失敗して書き戻すまで)に予約の時刻が来ても、`Failed` にしない。
    #[tokio::test(flavor = "multi_thread")]
    async fn a_post_due_during_a_failing_removal_is_not_marked_failed() {
        let f = fixture();
        let entry = f.state.scheduled_posts.add("a1", 1_000, test_input("due"), 0).unwrap();
        f.settings_disk.break_dir();
        let (deleted_tx, deleted_rx) = mpsc::channel();
        let (scheduled_tx, scheduled_rx) = mpsc::channel::<()>();
        *f.faults.after_delete.lock().unwrap() = Some((
            "a1",
            Box::new(move || {
                deleted_tx.send(()).unwrap();
                // 排他が無ければ、この間にスケジューラの処理が終わる。
                let _ = scheduled_rx.recv_timeout(Duration::from_millis(500));
            }),
        ));

        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| {
            std::thread::scope(|s| {
                let removal = s.spawn(|| handle.block_on(remove_account_core(&f.state, "a1")));
                deleted_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                handle.block_on(crate::scheduler::process_due(&f.state, 1_000));
                let _ = scheduled_tx.send(());
                assert!(removal.join().unwrap().is_err());
            })
        });

        // 戻ったトークンで送り、接続できないので再試行待ちになる。
        let after = f.state.scheduled_posts.get(&entry.id).unwrap();
        assert_eq!((after.status, after.attempts), (LocalScheduleStatus::Pending, 1), "{:?}", after.error);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_unknown_account_is_rejected() {
        let f = fixture();

        let err = remove_account_core(&f.state, "nope").await.unwrap_err();

        assert!(matches!(err, Error::Invalid(_)), "{err}");
        assert_untouched(&f);
    }

    // ---- ログインの保存の失敗(Issue #696) ----

    #[tokio::test(flavor = "multi_thread")]
    async fn a_saved_login_writes_the_token_and_the_account() {
        let f = fixture();

        persist_login(&f.state, &account("a3", "127.0.0.1:3"), "new-token").unwrap();

        assert_eq!(token(&f, "a3").as_deref(), Some("new-token"));
        assert!(f.state.settings.load_accounts().unwrap().iter().any(|a| a.id == "a3"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_login_that_cannot_be_saved_leaves_no_token_for_a_new_account() {
        let f = fixture();
        f.settings_disk.break_dir();

        assert!(persist_login(&f.state, &account("a3", "127.0.0.1:3"), "new-token").is_err());

        assert_eq!(token(&f, "a3"), None);
        assert_eq!(has_account(&f, "a3"), (false, false));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_reauthentication_that_cannot_be_saved_keeps_the_previous_token() {
        let f = fixture();
        f.settings_disk.break_dir();

        assert!(persist_login(&f.state, &account("a1", "127.0.0.1:1"), "new-token").is_err());

        assert_eq!(token(&f, "a1").as_deref(), Some("token"));
        assert_eq!(has_account(&f, "a1"), (true, true));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_login_writes_nothing_when_the_previous_token_cannot_be_read() {
        let f = fixture();
        f.faults.get.store(true, Ordering::SeqCst);

        let result = persist_login(&f.state, &account("a1", "127.0.0.1:1"), "new-token");

        f.faults.get.store(false, Ordering::SeqCst);
        assert!(matches!(result, Err(Error::Secret(_))), "{result:?}");
        assert_eq!(token(&f, "a1").as_deref(), Some("token"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_cleanup_still_reports_the_save_error() {
        let f = fixture();
        f.settings_disk.break_dir();
        f.faults.delete.store(true, Ordering::SeqCst);

        let result = persist_login(&f.state, &account("a3", "127.0.0.1:3"), "new-token");

        assert!(result.is_err() && !matches!(result, Err(Error::Secret(_))), "{result:?}");
    }
}
