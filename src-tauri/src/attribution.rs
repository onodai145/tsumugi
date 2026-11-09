//! ノートキャッシュのアカウント帰属のアプリ層ロジック(Issue #557)。
//! 設計: docs/superpowers/specs/2026-11-09-note-account-attribution-design.md

use crate::domain::{Column, ColumnKind, FilterQuery, Note};
use crate::filter::{ast, parser};
use crate::store::{NoteCacheStore, SettingsStore};

/// このカラムが `from cache` を含むか。含むカラムの REST 取得結果にはグローバルなキャッシュ由来の
/// ノートが混ざり、そのアカウントが取得した証拠にならない。ネットワークは使わない純粋な判定で、
/// 解析できない TQL は「含む」として扱う(埋め直しから除外=見えない側に倒す)。
pub(crate) fn column_includes_cache_source(column: &Column) -> bool {
    if !matches!(column.kind, ColumnKind::Tql) {
        return false;
    }
    let FilterQuery::Tql(text) = &column.filter else {
        return true;
    };
    match parser::parse(text) {
        Ok(q) => q.sources.iter().any(|s| matches!(s, ast::Source::Cache)),
        Err(_) => true,
    }
}

/// REST 取得・ギャップ埋めの結果を帰属させるアカウント。`from cache` を含むカラムでは None。
pub(crate) fn attribution_target(account_id: &str, use_cache: bool) -> Option<String> {
    (!use_cache).then(|| account_id.to_string())
}

/// 取得経路の帰属を記録する。失敗しても取得処理は止めない(未記録は外部に出ない側に倒れるだけ)。
pub(crate) async fn record(cache: &NoteCacheStore, account_id: &str, notes: &[Note]) {
    if notes.is_empty() {
        return;
    }
    let ids: Vec<String> = notes.iter().map(|n| n.id.clone()).collect();
    if let Err(e) = cache.record_note_accounts(account_id, &ids).await {
        log::warn!("failed to record the note attribution: {e}");
    }
}

/// アカウント削除時に、そのアカウントの帰属を消す。失敗してもアカウント削除は止めない
/// (残っても、そのアカウント ID が再び使われない限り外部には出ない)。
pub(crate) async fn forget(cache: &NoteCacheStore, account_id: &str) {
    if let Err(e) = cache.forget_account_notes(account_id).await {
        log::warn!("failed to forget the note attribution of the removed account: {e}");
    }
}

/// 既存の `column_note` から、`from cache` を含まないカラムの分を帰属として複写する(冪等)。
/// 起動時と、キャッシュのバックエンドを切り替えた直後に呼ぶ。
pub(crate) async fn backfill_all(settings: &SettingsStore, cache: &NoteCacheStore) {
    let columns = match settings.load_columns() {
        Ok(columns) => columns,
        Err(e) => {
            log::warn!("skipping the note attribution backfill: failed to load the columns: {e}");
            return;
        }
    };
    for column in columns.iter().filter(|c| !column_includes_cache_source(c)) {
        if let Err(e) = cache.backfill_note_accounts(&column.account_id, &column.id).await {
            log::warn!("failed to backfill the note attribution of a column: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SqliteBackend;

    fn column(id: &str, account_id: &str, kind: ColumnKind, filter: FilterQuery) -> Column {
        Column {
            id: id.into(),
            account_id: account_id.into(),
            kind,
            order: 0,
            filter,
            notify_sound: false,
            notify_desktop: false,
            notify_sound_choice: String::new(),
            group_id: "g1".into(),
            title: None,
        }
    }

    fn tql(text: &str) -> Column {
        column("c", "acc1", ColumnKind::Tql, FilterQuery::Tql(text.into()))
    }

    fn mem_cache() -> NoteCacheStore {
        NoteCacheStore::new(SqliteBackend::new(crate::store::db::open_cache_in_memory().unwrap()))
    }

    fn note(id: &str, created_at: i64) -> Note {
        Note {
            id: id.into(),
            created_at,
            text: None,
            cw: None,
            visibility: crate::domain::Visibility::Public,
            local_only: false,
            user: crate::domain::User {
                id: "author".into(),
                username: "alice".into(),
                host: None,
                name: None,
                avatar_url: None,
                is_bot: false,
                is_cat: false,
                followers_count: 0,
                following_count: 0,
                notes_count: 0,
                emojis: Default::default(),
                bio: None,
                banner_url: None,
                avatar_blurhash: None,
                instance: None,
            },
            reply_id: None,
            reply_user_id: None,
            reply: None,
            renote_id: None,
            renote: None,
            files: vec![],
            poll: None,
            tags: vec![],
            mentions: vec![],
            emojis: Default::default(),
            channel_id: None,
            via: None,
            lang: None,
            reactions: Default::default(),
            reaction_count: 0,
            renote_count: 0,
            reply_count: 0,
            my_reaction: None,
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }

    #[test]
    fn a_plain_column_does_not_include_the_cache_source() {
        assert!(!column_includes_cache_source(&column("c", "acc1", ColumnKind::Home, FilterQuery::Keywords(vec![]))));
    }

    #[test]
    fn a_tql_column_includes_the_cache_source_only_when_from_lists_cache() {
        assert!(!column_includes_cache_source(&tql("from home where reply")));
        assert!(column_includes_cache_source(&tql("from cache where reply")));
        assert!(column_includes_cache_source(&tql("from home, cache where reply")));
    }

    /// 解析できない TQL は、from cache を含むものとして扱う(埋め直しから除外=見えない側に倒す)。
    #[test]
    fn an_unparsable_tql_column_is_treated_as_including_the_cache_source() {
        assert!(column_includes_cache_source(&tql("from")));
        assert!(column_includes_cache_source(&column("c", "acc1", ColumnKind::Tql, FilterQuery::Keywords(vec![]))));
    }

    #[test]
    fn attribution_target_is_none_when_the_column_uses_the_cache() {
        assert_eq!(attribution_target("acc1", false).as_deref(), Some("acc1"));
        assert_eq!(attribution_target("acc1", true), None);
    }

    #[tokio::test]
    async fn record_attributes_the_notes_to_the_account() {
        let cache = mem_cache();
        cache.cache_notes("col1", &[note("n1", 1), note("n2", 2)]).await.unwrap();

        record(&cache, "acc1", &[note("n1", 1)]).await;

        assert!(cache.account_has_note("acc1", "n1").await.unwrap());
        assert!(!cache.account_has_note("acc1", "n2").await.unwrap());
    }

    #[tokio::test]
    async fn record_with_no_notes_is_a_noop() {
        let cache = mem_cache();
        record(&cache, "acc1", &[]).await;
    }

    /// 記録先が壊れていても、panic せず呼び出し元へエラーを返さない(取得処理を止めない)。
    #[tokio::test]
    async fn record_swallows_backend_errors() {
        let backend = SqliteBackend::new(crate::store::db::open_cache_in_memory().unwrap());
        let conn = std::sync::Arc::clone(backend.conn());
        let cache = NoteCacheStore::new(backend);
        cache.cache_notes("col1", &[note("n1", 1)]).await.unwrap();
        conn.lock().unwrap().execute("DROP TABLE note_account", []).unwrap();

        record(&cache, "acc1", &[note("n1", 1)]).await; // ここで戻ってくること自体がテスト
    }

    #[tokio::test]
    async fn forget_removes_only_the_given_accounts_attribution() {
        let cache = mem_cache();
        cache.cache_notes("col1", &[note("n1", 1)]).await.unwrap();
        record(&cache, "acc1", &[note("n1", 1)]).await;
        record(&cache, "acc2", &[note("n1", 1)]).await;

        forget(&cache, "acc1").await;

        assert!(!cache.account_has_note("acc1", "n1").await.unwrap());
        assert!(cache.account_has_note("acc2", "n1").await.unwrap());
    }

    /// 記録先が壊れていても、アカウント削除を止めない。
    #[tokio::test]
    async fn forget_swallows_backend_errors() {
        let backend = SqliteBackend::new(crate::store::db::open_cache_in_memory().unwrap());
        let conn = std::sync::Arc::clone(backend.conn());
        let cache = NoteCacheStore::new(backend);
        conn.lock().unwrap().execute("DROP TABLE note_account", []).unwrap();

        forget(&cache, "acc1").await; // ここで戻ってくること自体がテスト
    }

    #[tokio::test]
    async fn backfill_all_copies_plain_columns_and_skips_columns_that_use_the_cache() {
        let settings = SettingsStore::new_in_memory();
        settings.upsert_column(&column("plain", "acc1", ColumnKind::Home, FilterQuery::Keywords(vec![]))).unwrap();
        let mut mixed = tql("from home, cache where reply");
        mixed.id = "mixed".into();
        settings.upsert_column(&mixed).unwrap();
        let cache = mem_cache();
        cache.cache_notes("plain", &[note("n1", 1)]).await.unwrap();
        cache.cache_notes("mixed", &[note("n2", 2)]).await.unwrap();

        backfill_all(&settings, &cache).await;

        assert!(cache.account_has_note("acc1", "n1").await.unwrap());
        assert!(!cache.account_has_note("acc1", "n2").await.unwrap()); // from cache を含むカラムは除外
    }
}
