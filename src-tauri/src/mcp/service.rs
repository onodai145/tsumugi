//! MCP のツールの中身(Issue #557)。rmcp に依存せず、`AppState` だけでテストできる。
//! 「公開してよいか」は、毎回、現在の設定から判定する(起動時の値をキャッシュしない)。

use crate::domain::Account;
use crate::mcp::dto::{from_note, NoteDto};
use crate::mcp::query::{build_get, build_search, SearchParams};
use crate::state::AppState;
use serde::Serialize;

#[derive(Debug, PartialEq, Eq)]
pub enum ToolError {
    /// 公開OFF・存在しないアカウント・スコープ外のノート。区別がつかない同一のエラー。
    NotFound,
    /// 入力が不正(クライアントへ返してよい一般的な文言)。
    Invalid(String),
    /// 内部エラー。詳細はログにだけ残す。
    Internal,
}

impl ToolError {
    pub fn message(&self) -> &str {
        match self {
            ToolError::NotFound => "not found",
            ToolError::Invalid(m) => m,
            ToolError::Internal => "internal error",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDto {
    pub id: String,
    pub acct: String,
    pub display_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsResult {
    pub accounts: Vec<AccountDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub notes: Vec<NoteDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

fn account_dto(a: &Account) -> AccountDto {
    AccountDto { id: a.id.clone(), acct: format!("@{}@{}", a.username, a.host), display_name: a.display_name.clone() }
}

/// 公開が許可されたアカウントの一覧(機能がOFFなら空)。
pub fn list_accounts(state: &AppState) -> AccountsResult {
    let mcp = state.settings.load_mcp();
    if !mcp.enabled {
        return AccountsResult { accounts: vec![] };
    }
    let accounts = state
        .accounts
        .lock()
        .unwrap()
        .list()
        .iter()
        .filter(|a| mcp.exposed_account_ids.contains(&a.id))
        .map(account_dto)
        .collect();
    AccountsResult { accounts }
}

/// 公開が許可されたアカウントを返す。機能OFF・公開OFF・存在しないは、すべて `NotFound`。
fn exposed_account(state: &AppState, account_id: &str) -> Result<Account, ToolError> {
    let mcp = state.settings.load_mcp();
    if !mcp.enabled || !mcp.exposed_account_ids.iter().any(|id| id == account_id) {
        return Err(ToolError::NotFound);
    }
    state.accounts.lock().unwrap().list().into_iter().find(|a| a.id == account_id).ok_or(ToolError::NotFound)
}

pub async fn search_notes(state: &AppState, account_id: &str, params: &SearchParams) -> Result<SearchResult, ToolError> {
    let account = exposed_account(state, account_id)?;
    let q = build_search(&account.id, &account.host, params).map_err(|e| ToolError::Invalid(e.0))?;
    let notes = state.cache.search_cache(&q.where_sql, q.until_id.as_deref(), q.limit).await.map_err(|e| {
        log::warn!("mcp search failed: {e}");
        ToolError::Internal
    })?;
    // 次ページがあり得るのは、ページが満杯のとき。カーソルは、DTOに変換する前の最後のIDにする。
    let next_cursor = if notes.len() as u32 >= q.limit { notes.last().map(|n| n.id.clone()) } else { None };
    let notes = notes.iter().filter_map(|n| from_note(n, &account.host)).collect();
    Ok(SearchResult { notes, next_cursor })
}

pub async fn get_note(state: &AppState, account_id: &str, note_id: &str) -> Result<NoteDto, ToolError> {
    let account = exposed_account(state, account_id)?;
    let q = build_get(&account.id, note_id).map_err(|_| ToolError::NotFound)?;
    let notes = state.cache.search_cache(&q.where_sql, None, q.limit).await.map_err(|e| {
        log::warn!("mcp get_note failed: {e}");
        ToolError::Internal
    })?;
    notes.first().and_then(|n| from_note(n, &account.host)).ok_or(ToolError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Visibility;
    use crate::mcp::testutil::{account, note, public_note, user};
    use crate::store::SettingsStore;

    /// a1(公開ON)・a2(公開OFF)・a3(存在しない)。a1 のキャッシュに公開/非公開のノートを入れる。
    async fn state() -> AppState {
        let state = AppState::new_for_test(SettingsStore::new_in_memory());
        state.accounts.lock().unwrap().upsert(account("a1", "one.example"));
        state.accounts.lock().unwrap().upsert(account("a2", "two.example"));
        state.settings.update_mcp(|m| {
            m.enabled = true;
            m.exposed_account_ids = vec!["a1".into()];
        }).unwrap();

        let alice = user("u1", "alice", None);
        let notes = vec![
            note("p", 500, Visibility::Public, false, "public rust 100%", alice.clone()),
            note("h", 400, Visibility::Home, false, "home rust", alice.clone()),
            note("f", 300, Visibility::Followers, false, "followers rust", alice.clone()),
            note("l", 200, Visibility::Public, true, "local only rust", alice.clone()),
            note("o", 450, Visibility::Public, false, "belongs to a2", alice),
        ];
        state.cache.cache_notes("col1", &notes).await.unwrap();
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        state.cache.record_note_accounts("a1", &ids(&["p", "h", "f", "l"])).await.unwrap();
        state.cache.record_note_accounts("a2", &ids(&["o"])).await.unwrap();
        state
    }

    fn ids(r: &SearchResult) -> Vec<&str> {
        r.notes.iter().map(|n| n.id.as_str()).collect()
    }

    #[tokio::test]
    async fn list_accounts_returns_only_exposed_accounts() {
        let s = state().await;
        let r = list_accounts(&s);
        assert_eq!(r.accounts.len(), 1);
        assert_eq!(r.accounts[0].id, "a1");
        assert_eq!(r.accounts[0].acct, "@me@one.example");
        assert_eq!(r.accounts[0].display_name, "Me");
    }

    #[tokio::test]
    async fn list_accounts_is_empty_when_the_feature_is_off() {
        let s = state().await;
        s.settings.update_mcp(|m| m.enabled = false).unwrap();
        assert!(list_accounts(&s).accounts.is_empty());
    }

    #[tokio::test]
    async fn search_returns_only_public_or_home_attributed_notes() {
        let s = state().await;
        let r = search_notes(&s, "a1", &SearchParams::default()).await.unwrap();
        assert_eq!(ids(&r), vec!["p", "h"]);
        assert_eq!(r.notes[0].url, "https://one.example/notes/p");
        assert_eq!(r.next_cursor, None, "limit に満たないので次ページは無い");
    }

    #[tokio::test]
    async fn unexposed_unknown_and_disabled_all_look_the_same() {
        let s = state().await;
        let p = SearchParams::default();
        // 公開OFFのアカウント、存在しないアカウントは同じ「見つからない」。
        assert_eq!(search_notes(&s, "a2", &p).await.err(), Some(ToolError::NotFound));
        assert_eq!(search_notes(&s, "a3", &p).await.err(), Some(ToolError::NotFound));
        assert_eq!(get_note(&s, "a2", "o").await.err(), Some(ToolError::NotFound));
        // 機能全体がOFF
        s.settings.update_mcp(|m| m.enabled = false).unwrap();
        assert_eq!(search_notes(&s, "a1", &p).await.err(), Some(ToolError::NotFound));
    }

    #[tokio::test]
    async fn turning_exposure_off_takes_effect_on_the_next_call() {
        let s = state().await;
        assert!(search_notes(&s, "a1", &SearchParams::default()).await.is_ok());
        s.settings.update_mcp(|m| m.exposed_account_ids.clear()).unwrap();
        assert_eq!(search_notes(&s, "a1", &SearchParams::default()).await.err(), Some(ToolError::NotFound));
    }

    #[tokio::test]
    async fn get_note_returns_only_in_scope_notes() {
        let s = state().await;
        assert_eq!(get_note(&s, "a1", "p").await.unwrap().id, "p");
        for out_of_scope in ["f", "l", "o", "nope"] {
            assert_eq!(get_note(&s, "a1", out_of_scope).await.err(), Some(ToolError::NotFound), "{out_of_scope}");
        }
    }

    #[tokio::test]
    async fn invalid_parameters_are_reported_as_invalid_not_not_found() {
        let s = state().await;
        let p = SearchParams { since: Some("yesterday".into()), ..Default::default() };
        assert!(matches!(search_notes(&s, "a1", &p).await.err(), Some(ToolError::Invalid(_))));
    }

    #[tokio::test]
    async fn percent_in_text_is_a_literal_and_never_widens_the_scope() {
        // `%` は実文字として扱われ、ワイルドカードにならない。本文に `%` を含む `p`(`100%`)だけが出て、
        // `h` などは出ない。他アカウント・範囲外のノートも混ざらない。
        let s = state().await;
        let p = SearchParams { text: Some("%".into()), ..Default::default() };
        assert_eq!(ids(&search_notes(&s, "a1", &p).await.unwrap()), vec!["p"]);
    }

    #[tokio::test]
    async fn an_account_with_no_attribution_yields_an_empty_list() {
        // 旧DB・埋め直し前: 帰属が無くてもエラーにせず、空で返す(fail-closed)。
        let s = AppState::new_for_test(SettingsStore::new_in_memory());
        s.accounts.lock().unwrap().upsert(account("a1", "one.example"));
        s.settings.update_mcp(|m| {
            m.enabled = true;
            m.exposed_account_ids = vec!["a1".into()];
        }).unwrap();
        s.cache.cache_notes("col1", &[public_note("p", 1, "hello")]).await.unwrap();
        let r = search_notes(&s, "a1", &SearchParams::default()).await.unwrap();
        assert!(r.notes.is_empty());
    }

    #[tokio::test]
    async fn the_accounts_own_host_in_the_user_filter_finds_local_users() {
        // a1 のホストは one.example。同じサーバーのユーザーはキャッシュ上ローカル(host なし)なので、
        // `@alice@one.example` でも見つかる(以前は 0 件になっていた)。別のホストには一致しない。
        let s = state().await;
        let by = |u: &str| SearchParams { user: Some(u.into()), ..Default::default() };
        assert_eq!(ids(&search_notes(&s, "a1", &by("@alice@one.example")).await.unwrap()), vec!["p", "h"]);
        assert_eq!(ids(&search_notes(&s, "a1", &by("@alice")).await.unwrap()), vec!["p", "h"]);
        assert!(search_notes(&s, "a1", &by("@alice@two.example")).await.unwrap().notes.is_empty());
    }

    #[tokio::test]
    async fn next_cursor_is_the_last_id_when_the_page_is_full() {
        let s = state().await;
        let r = search_notes(&s, "a1", &SearchParams { limit: Some(1), ..Default::default() }).await.unwrap();
        assert_eq!(ids(&r), vec!["p"]);
        assert_eq!(r.next_cursor.as_deref(), Some("p"));
        let r2 = search_notes(&s, "a1", &SearchParams { limit: Some(1), cursor: r.next_cursor, ..Default::default() }).await.unwrap();
        assert_eq!(ids(&r2), vec!["h"]);
    }
}
