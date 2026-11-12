//! 構造化パラメータ → 検索クエリ(Issue #557)。
//!
//! TQL の AST を直接組み立てて `filter::sql::build_where` に渡し、その結果に**固定の**スコープ条件
//! (公開範囲・ローカル限定・アカウント帰属)を AND する。クライアントの入力は、AST の値か bind される
//! パラメータにしかならず、SQL 文字列には入らない。
//!
//! 注意: TQL の部分一致(`Contains`)は、`%` / `_` / `\` を実文字として扱い、大文字小文字を区別する
//! (方言ごとの関数で書くので、検索語のエスケープは要らない)。`text` の検索語はバインドされる。

use crate::filter::ast::{CompareOp, Expr, Field, Value};
use crate::filter::sql::{build_where, SqlCtx, SqlParam, SqlWhere};

pub const DEFAULT_LIMIT: u32 = 20;
pub const MAX_LIMIT: u32 = 50;
pub const MAX_TEXT_LEN: usize = 200;
pub const MAX_TERMS: usize = 8;
const MAX_ID_LEN: usize = 64;

/// 全クエリに必ず付く条件。`local_only` はPostgreSQLではBOOLEAN列のため `= 0` とは書かない。
/// 最後の `?` に、アカウントIDを(最後のパラメータとして)bindする。
pub const SCOPE_SQL: &str = "n.visibility IN ('public','home') AND NOT n.local_only AND n.id IN (SELECT note_id FROM note_account WHERE account_id = ?)";

#[derive(Debug, Default, Clone)]
pub struct SearchParams {
    pub text: Option<String>,
    pub user: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub has_media: Option<bool>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

pub struct Query {
    pub where_sql: SqlWhere,
    pub until_id: Option<String>,
    pub limit: u32,
}

/// 入力が不正なときのメッセージ(クライアントに返してよい一般的な文言)。
#[derive(Debug, PartialEq, Eq)]
pub struct QueryError(pub String);

fn invalid(msg: &str) -> QueryError {
    QueryError(msg.to_string())
}

fn with_scope(base: SqlWhere, account_id: &str) -> SqlWhere {
    base.and_raw(SCOPE_SQL, vec![SqlParam::Text(account_id.to_string())])
}

fn compare(field: Field, op: CompareOp, rhs: Value) -> Expr {
    Expr::Compare { lhs: Value::Field(field), op, rhs }
}

fn and_all(mut parts: Vec<Expr>) -> Option<Expr> {
    if parts.is_empty() {
        return None;
    }
    let first = parts.remove(0);
    Some(parts.into_iter().fold(first, |acc, e| Expr::And(Box::new(acc), Box::new(e))))
}

/// ISO 8601 の日時(`2026-11-01T00:00:00Z`)か、日付だけ(`2026-11-01`、UTC)。日付だけのときは、
/// 範囲の下限(`since`)ならその日の0時、上限(`until`)ならその日の最後の秒にする(その日全体を含む)。
fn parse_time(label: &str, s: &str, end_of_day: bool) -> Result<f64, QueryError> {
    let s = s.trim();
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(d.timestamp() as f64);
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let t = if end_of_day { d.and_hms_opt(23, 59, 59) } else { d.and_hms_opt(0, 0, 0) };
        if let Some(t) = t {
            return Ok(t.and_utc().timestamp() as f64);
        }
    }
    Err(QueryError(format!("{label} must be an ISO 8601 timestamp or date, e.g. 2026-11-01T00:00:00Z or 2026-11-01")))
}

fn is_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_ID_LEN && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// `@alice` / `alice` / `@alice@example.com` を、`@alice[@host]` に揃える。
/// キャッシュ上、そのアカウントと同じサーバーのユーザーはローカル(host なし)として `@alice` で
/// 保存されている。`account_host` と同じホストが指定されたら、ローカルユーザーとして `@alice` にする。
fn normalize_acct(raw: &str, account_host: &str) -> Result<String, QueryError> {
    let t = raw.trim();
    let t = t.strip_prefix('@').unwrap_or(t);
    let mut it = t.splitn(2, '@');
    let name = it.next().unwrap_or("");
    let host = it.next();
    let name_ok = !name.is_empty() && name.len() <= 128 && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    let host_ok = host.is_none_or(|h| !h.is_empty() && h.len() <= 253 && h.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':')));
    if !name_ok || !host_ok {
        return Err(invalid("user must look like @name or @name@host"));
    }
    Ok(match host {
        Some(h) if !h.eq_ignore_ascii_case(account_host) => format!("@{name}@{h}"),
        _ => format!("@{name}"),
    })
}

/// `account_host` は、検索するアカウントのホスト(`user` の正規化に使う)。
pub fn build_search(account_id: &str, account_host: &str, p: &SearchParams) -> Result<Query, QueryError> {
    let mut parts: Vec<Expr> = Vec::new();

    if let Some(text) = p.text.as_deref() {
        let text = text.trim();
        if text.chars().count() > MAX_TEXT_LEN {
            return Err(QueryError(format!("text must be at most {MAX_TEXT_LEN} characters")));
        }
        let terms: Vec<&str> = text.split_whitespace().collect();
        if terms.len() > MAX_TERMS {
            return Err(QueryError(format!("text must have at most {MAX_TERMS} words")));
        }
        for t in terms {
            parts.push(compare(Field::Text, CompareOp::Contains, Value::Str(t.to_string())));
        }
    }
    if let Some(user) = p.user.as_deref() {
        parts.push(compare(Field::UserAcct, CompareOp::Eq, Value::Str(normalize_acct(user, account_host)?)));
    }
    if let Some(since) = p.since.as_deref() {
        parts.push(compare(Field::CreatedAt, CompareOp::Ge, Value::Num(parse_time("since", since, false)?)));
    }
    if let Some(until) = p.until.as_deref() {
        parts.push(compare(Field::CreatedAt, CompareOp::Le, Value::Num(parse_time("until", until, true)?)));
    }
    match p.has_media {
        Some(true) => parts.push(Expr::Bare(Value::Field(Field::HasFiles))),
        Some(false) => parts.push(Expr::Not(Box::new(Expr::Bare(Value::Field(Field::HasFiles))))),
        None => {}
    }

    // 自分視点の述語(mine / reacted など)は組み立てないので、`SqlCtx` は空でよい。
    let ctx = SqlCtx { my_ids: vec![], following_ids: None };
    let base = match and_all(parts) {
        Some(expr) => build_where(&expr, &ctx).map_err(|_| invalid("invalid search parameters"))?,
        None => SqlWhere::always_true(),
    };

    let until_id = match p.cursor.as_deref() {
        Some(c) if is_id(c) => Some(c.to_string()),
        Some(_) => return Err(invalid("cursor is invalid")),
        None => None,
    };
    Ok(Query {
        where_sql: with_scope(base, account_id),
        until_id,
        limit: p.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
    })
}

pub fn build_get(account_id: &str, note_id: &str) -> Result<Query, QueryError> {
    if !is_id(note_id) {
        return Err(invalid("id is invalid"));
    }
    let base = SqlWhere::raw("n.id = ?", vec![SqlParam::Text(note_id.to_string())]);
    Ok(Query { where_sql: with_scope(base, account_id), until_id: None, limit: 1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Visibility;
    use crate::filter::sql::{Dialect, SqlParam};
    use crate::mcp::testutil::{note, public_note, user};
    use crate::store::NoteCacheStore;

    /// テストのアカウントのホスト。
    const HOST: &str = "one.example";

    fn texts(q: &Query) -> Vec<String> {
        q.where_sql
            .render(Dialect::Sqlite)
            .params
            .iter()
            .filter_map(|p| match p {
                SqlParam::Text(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_query_carries_the_scope_with_the_account_id_bound_last() {
        for q in [
            build_search("a1", HOST, &SearchParams::default()).unwrap(),
            build_search("a1", HOST, &SearchParams { text: Some("rust".into()), has_media: Some(true), ..Default::default() }).unwrap(),
            build_get("a1", "note1").unwrap(),
        ] {
            let sql = &q.where_sql.render(Dialect::Sqlite).sql;
            assert!(sql.contains(SCOPE_SQL), "スコープ条件が付いていない: {sql}");
            assert_eq!(texts(&q).last().map(String::as_str), Some("a1"), "最後のパラメータはアカウントID");
        }
    }

    #[test]
    fn text_words_become_bound_needles_and_never_enter_the_sql() {
        let p = SearchParams { text: Some("rust  '; DROP TABLE note;--".into()), ..Default::default() };
        let q = build_search("a1", HOST, &p).unwrap();
        for d in [Dialect::Sqlite, Dialect::Postgres, Dialect::MySql] {
            let sql = &q.where_sql.render(d).sql;
            assert!(!sql.contains("DROP"), "入力がSQLに入っている: {sql}");
            assert!(!sql.contains("rust"));
        }
        let t = texts(&q);
        assert_eq!(t[0], "rust");
        assert_eq!(t[1], "';");
        assert!(t.contains(&"DROP".to_string()));
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let bad = |p: SearchParams| build_search("a1", HOST, &p).is_err();
        assert!(bad(SearchParams { text: Some("a b c d e f g h i".into()), ..Default::default() }), "9語");
        assert!(bad(SearchParams { text: Some("x".repeat(201)), ..Default::default() }), "201字");
        assert!(bad(SearchParams { since: Some("yesterday".into()), ..Default::default() }));
        assert!(bad(SearchParams { until: Some("2026-13-40T00:00:00Z".into()), ..Default::default() }));
        assert!(bad(SearchParams { user: Some("@a b".into()), ..Default::default() }));
        assert!(bad(SearchParams { user: Some("@@host".into()), ..Default::default() }));
        assert!(bad(SearchParams { cursor: Some("x' OR 1=1".into()), ..Default::default() }));
        assert!(build_get("a1", "").is_err());
        assert!(build_get("a1", "n'; --").is_err());
    }

    #[test]
    fn blank_text_is_treated_as_absent() {
        let a = build_search("a1", HOST, &SearchParams { text: Some("   ".into()), ..Default::default() }).unwrap();
        let b = build_search("a1", HOST, &SearchParams::default()).unwrap();
        assert_eq!(a.where_sql.render(Dialect::Sqlite).sql, b.where_sql.render(Dialect::Sqlite).sql);
    }

    #[test]
    fn limit_defaults_to_20_and_is_clamped_to_1_through_50() {
        let lim = |l: Option<u32>| build_search("a1", HOST, &SearchParams { limit: l, ..Default::default() }).unwrap().limit;
        assert_eq!(lim(None), 20);
        assert_eq!(lim(Some(0)), 1);
        assert_eq!(lim(Some(10)), 10);
        assert_eq!(lim(Some(9999)), 50);
    }

    #[test]
    fn user_is_normalized_to_an_acct_with_a_leading_at() {
        let t = |u: &str| texts(&build_search("a1", HOST, &SearchParams { user: Some(u.into()), ..Default::default() }).unwrap());
        assert_eq!(t("@alice")[0], "@alice");
        assert_eq!(t("alice")[0], "@alice", "@無しでも受け付ける");
        assert_eq!(t("@alice@example.com")[0], "@alice@example.com");
    }

    #[test]
    fn since_and_until_become_epoch_second_bounds() {
        let q = build_search(
            "a1",
            HOST,
            &SearchParams { since: Some("2026-11-01T00:00:00Z".into()), until: Some("2026-11-02T09:00:00+09:00".into()), ..Default::default() },
        )
        .unwrap();
        let reals: Vec<f64> = q
            .where_sql
            .render(Dialect::Sqlite)
            .params
            .iter()
            .filter_map(|p| if let SqlParam::Real(x) = p { Some(*x) } else { None })
            .collect();
        // 2026-11-01T00:00:00Z と、2026-11-02T09:00:00+09:00(= 2026-11-02T00:00:00Z)のepoch秒。
        assert_eq!(reals, vec![1_793_491_200.0, 1_793_577_600.0]);
    }

    #[test]
    fn a_user_on_the_accounts_own_host_is_matched_as_a_local_user() {
        // キャッシュ上、アカウントと同じサーバーのユーザーはローカル(host なし)として `@name` で保存される。
        let t = |u: &str| texts(&build_search("a1", HOST, &SearchParams { user: Some(u.into()), ..Default::default() }).unwrap())[0].clone();
        assert_eq!(t("@alice@one.example"), "@alice");
        assert_eq!(t("@alice@ONE.Example"), "@alice", "ホストの大文字小文字は区別しない");
        assert_eq!(t("alice@one.example"), "@alice", "@無しでも同じ");
        assert_eq!(t("@alice@other.example"), "@alice@other.example", "別のホストはそのまま");
        assert_eq!(t("@alice"), "@alice");
    }

    #[tokio::test]
    async fn searching_with_the_accounts_own_host_finds_the_local_users_notes() {
        let cache = NoteCacheStore::new(crate::store::SqliteBackend::new(crate::store::db::open_cache_in_memory().unwrap()));
        let notes = vec![
            note("l1", 200, Visibility::Public, false, "from a local user", user("u1", "alice", None)),
            note("r1", 100, Visibility::Public, false, "from a remote user", user("u2", "bob", Some("remote.example"))),
        ];
        cache.cache_notes("col1", &notes).await.unwrap();
        cache.record_note_accounts("a1", &["l1".to_string(), "r1".to_string()]).await.unwrap();
        let by = |u: &str| build_search("a1", HOST, &SearchParams { user: Some(u.into()), ..Default::default() }).unwrap();
        assert_eq!(ids_of(&cache, &by("@alice@one.example")).await, vec!["l1"], "同じホストを付けても見つかる");
        assert_eq!(ids_of(&cache, &by("@alice")).await, vec!["l1"]);
        assert_eq!(ids_of(&cache, &by("@bob@remote.example")).await, vec!["r1"]);
    }

    #[test]
    fn date_only_since_and_until_cover_the_whole_day() {
        let reals = |since: Option<&str>, until: Option<&str>| -> Vec<f64> {
            build_search("a1", HOST, &SearchParams { since: since.map(Into::into), until: until.map(Into::into), ..Default::default() })
                .unwrap()
                .where_sql
                .render(Dialect::Sqlite)
                .params
                .iter()
                .filter_map(|p| if let SqlParam::Real(x) = p { Some(*x) } else { None })
                .collect()
        };
        // 2026-11-01 は 1793491200(UTC 0時)。since は0時、until はその日の最後の秒。
        assert_eq!(reals(Some("2026-11-01"), None), vec![1_793_491_200.0]);
        assert_eq!(reals(None, Some("2026-11-01")), vec![1_793_491_200.0 + 86_399.0]);
        assert!(build_search("a1", HOST, &SearchParams { since: Some("2026-13-01".into()), ..Default::default() }).is_err());
    }

    /// 実際のSQLiteで、スコープ条件が効くことを確かめる。
    async fn seeded() -> NoteCacheStore {
        let cache = NoteCacheStore::new(crate::store::SqliteBackend::new(crate::store::db::open_cache_in_memory().unwrap()));
        let alice = user("u1", "alice", None);
        let notes = vec![
            note("p", 500, Visibility::Public, false, "public rust", alice.clone()),
            note("h", 400, Visibility::Home, false, "home rust", alice.clone()),
            note("f", 300, Visibility::Followers, false, "followers rust", alice.clone()),
            note("s", 200, Visibility::Specified, false, "specified rust", alice.clone()),
            note("l", 100, Visibility::Public, true, "local only rust", alice.clone()),
            note("o", 450, Visibility::Public, false, "other account rust", alice.clone()),
            note("x", 420, Visibility::Public, false, "no attribution rust", alice),
        ];
        cache.cache_notes("col1", &notes).await.unwrap();
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        cache.record_note_accounts("a1", &ids(&["p", "h", "f", "s", "l"])).await.unwrap();
        cache.record_note_accounts("a2", &ids(&["o"])).await.unwrap();
        cache
    }

    async fn ids_of(cache: &NoteCacheStore, q: &Query) -> Vec<String> {
        cache.search_cache(&q.where_sql, q.until_id.as_deref(), q.limit).await.unwrap().into_iter().map(|n| n.id).collect()
    }

    #[tokio::test]
    async fn search_returns_only_attributed_public_or_home_notes_that_are_not_local_only() {
        let cache = seeded().await;
        let q = build_search("a1", HOST, &SearchParams::default()).unwrap();
        assert_eq!(ids_of(&cache, &q).await, vec!["p", "h"]);
    }

    #[tokio::test]
    async fn filters_narrow_within_the_scope_and_never_widen_it() {
        let cache = seeded().await;
        let q = build_search("a1", HOST, &SearchParams { text: Some("home".into()), ..Default::default() }).unwrap();
        assert_eq!(ids_of(&cache, &q).await, vec!["h"]);
        // 他アカウントのノートは、文字列が一致しても出ない。
        let q = build_search("a1", HOST, &SearchParams { text: Some("other account".into()), ..Default::default() }).unwrap();
        assert!(ids_of(&cache, &q).await.is_empty());
        // 帰属の無いノートも出ない。
        let q = build_search("a1", HOST, &SearchParams { text: Some("no attribution".into()), ..Default::default() }).unwrap();
        assert!(ids_of(&cache, &q).await.is_empty());
    }

    #[tokio::test]
    async fn percent_and_underscore_in_text_are_literal_and_stay_inside_the_scope() {
        let cache = NoteCacheStore::new(crate::store::SqliteBackend::new(crate::store::db::open_cache_in_memory().unwrap()));
        let notes = vec![
            public_note("n1", 400, "50% off"),
            public_note("n2", 300, "50 off"),
            public_note("n3", 200, "a_b"),
            public_note("n4", 100, "aXb"),
        ];
        cache.cache_notes("col1", &notes).await.unwrap();
        cache.record_note_accounts("a1", &notes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()).await.unwrap();

        for (text, expected) in [("%", vec!["n1"]), ("50%", vec!["n1"]), ("_", vec!["n3"]), ("a_b", vec!["n3"])] {
            let q = build_search("a1", HOST, &SearchParams { text: Some(text.into()), ..Default::default() }).unwrap();
            assert_eq!(ids_of(&cache, &q).await, expected, "text = {text:?}");
        }
        // スコープ条件のANDは外れない: 帰属の無いアカウントでは、何も出ない。
        let q = build_search("never-seen", HOST, &SearchParams { text: Some("%".into()), ..Default::default() }).unwrap();
        assert!(ids_of(&cache, &q).await.is_empty());
        // seeded() の範囲外のノートも、`%` では出ない。
        let seeded = seeded().await;
        let q = build_search("a1", HOST, &SearchParams { text: Some("%".into()), ..Default::default() }).unwrap();
        assert!(ids_of(&seeded, &q).await.is_empty());
    }

    #[tokio::test]
    async fn an_account_with_no_attribution_gets_an_empty_result_not_an_error() {
        let cache = seeded().await;
        let q = build_search("never-seen", HOST, &SearchParams::default()).unwrap();
        assert!(ids_of(&cache, &q).await.is_empty());
    }

    #[tokio::test]
    async fn cursor_pages_through_older_notes() {
        let cache = NoteCacheStore::new(crate::store::SqliteBackend::new(crate::store::db::open_cache_in_memory().unwrap()));
        let notes: Vec<_> = (1..=5).map(|i| public_note(&format!("n{i}"), i * 100, "hello")).collect();
        cache.cache_notes("col1", &notes).await.unwrap();
        cache.record_note_accounts("a1", &notes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()).await.unwrap();

        let first = build_search("a1", HOST, &SearchParams { limit: Some(2), ..Default::default() }).unwrap();
        assert_eq!(ids_of(&cache, &first).await, vec!["n5", "n4"]);
        let second = build_search("a1", HOST, &SearchParams { limit: Some(2), cursor: Some("n4".into()), ..Default::default() }).unwrap();
        assert_eq!(ids_of(&cache, &second).await, vec!["n3", "n2"]);
    }

    #[tokio::test]
    async fn get_returns_one_note_only_when_it_is_in_scope() {
        let cache = seeded().await;
        for (id, expected) in [("p", 1), ("h", 1), ("f", 0), ("s", 0), ("l", 0), ("o", 0), ("x", 0), ("nope", 0)] {
            let q = build_get("a1", id).unwrap();
            assert_eq!(ids_of(&cache, &q).await.len(), expected, "note {id}");
        }
    }
}
