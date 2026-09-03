//! note cacheのSqliteBackend(Issue #115 Phase 1)。既存の`note_cache.rs`のロジックを
//! `NoteCacheBackend`トレイトの非同期メソッドとして提供する。rusqliteの同期呼び出しを
//! `tauri::async_runtime::spawn_blocking`で包む(rusqliteとsqlxのSQLiteドライバは
//! `libsqlite3-sys`のバージョン要求が競合し共存できないため、Phase 1ではsqlxを使わない。
//! 設計書「DBアクセス手段」参照)。

use crate::domain::Note;
use crate::error::{Error, Result};
use crate::store::note_cache::NoteCacheBackend;
use rusqlite::{Connection, OptionalExtension};
use std::sync::{Arc, Mutex};

pub(crate) struct SqliteBackend {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteBackend {
    pub(crate) fn new(conn: Connection) -> Self {
        Self { conn: Arc::new(Mutex::new(conn)) }
    }

    #[cfg(test)]
    pub(crate) fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }
}

/// `tauri::async_runtime::spawn_blocking`のjoinエラー(`tauri::Error`)を`Error`へマッピングする
/// 共通ヘルパー。
fn map_join_error(e: tauri::Error) -> Error {
    Error::Db(format!("cache task join error: {e}"))
}

#[async_trait::async_trait]
impl NoteCacheBackend for SqliteBackend {
    async fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let column_id = column_id.to_string();
        let notes = notes.to_vec();
        tauri::async_runtime::spawn_blocking(move || -> Result<()> {
            if notes.is_empty() {
                return Ok(());
            }
            let mut guard = conn.lock().unwrap();
            let tx = guard.transaction()?;
            let now = crate::store::note_cache::now_epoch();
            for n in &notes {
                crate::store::note_cache::upsert_note(&tx, n)?;
                tx.execute(
                    "INSERT OR IGNORE INTO column_note (column_id, note_id, received_at, created_at) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![column_id, n.id, now, n.created_at],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
        .map_err(map_join_error)?
    }

    async fn cache_note(&self, column_id: &str, note: &Note) -> Result<()> {
        self.cache_notes(column_id, std::slice::from_ref(note)).await
    }

    async fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>> {
        let conn = Arc::clone(&self.conn);
        let column_id = column_id.to_string();
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<Note>> {
            let guard = conn.lock().unwrap();
            let mut stmt = guard.prepare(
                "SELECT n.id, n.payload FROM column_note cn
                 JOIN note n ON n.id = cn.note_id
                 WHERE cn.column_id = ?1
                 ORDER BY cn.created_at DESC, cn.note_id DESC
                 LIMIT ?2",
            )?;
            let rows: Vec<(String, String)> = stmt
                .query_map(rusqlite::params![column_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            drop(stmt);
            crate::store::note_cache::resolve_payload_rows(&guard, rows)
        })
        .await
        .map_err(map_join_error)?
    }

    async fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>> {
        let conn = Arc::clone(&self.conn);
        let column_id = column_id.to_string();
        let until_id = until_id.to_string();
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<Note>> {
            let guard = conn.lock().unwrap();
            let mut stmt = guard.prepare(
                "SELECT n.id, n.payload FROM column_note cn
                 JOIN note n ON n.id = cn.note_id
                 WHERE cn.column_id = ?1 AND cn.note_id < ?2
                 ORDER BY cn.note_id DESC
                 LIMIT ?3",
            )?;
            let rows: Vec<(String, String)> = stmt
                .query_map(rusqlite::params![column_id, until_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            drop(stmt);
            let mut out = crate::store::note_cache::resolve_payload_rows(&guard, rows)?;
            // 絞り込み・LIMITの選抜は境界比較と同じ id 基準で行い、表示順への並べ替えだけを
            // ここで行う。created_at で選抜すると、連合ノート(idとcreated_atの順序が食い違う)が
            // 範囲内にあるのに LIMIT の外へ弾かれて静かに欠落する(Issue #228)。
            out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
            Ok(out)
        })
        .await
        .map_err(map_join_error)?
    }

    async fn get_note(&self, note_id: &str) -> Result<Option<Note>> {
        let conn = Arc::clone(&self.conn);
        let note_id = note_id.to_string();
        tauri::async_runtime::spawn_blocking(move || -> Result<Option<Note>> {
            let guard = conn.lock().unwrap();
            let row: Option<(String, String)> = guard
                .query_row("SELECT id, payload FROM note WHERE id = ?1", rusqlite::params![note_id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?;
            Ok(match row {
                Some((id, payload)) => {
                    crate::store::note_cache::resolve_payload_rows(&guard, vec![(id, payload)])?.into_iter().next()
                }
                None => None,
            })
        })
        .await
        .map_err(map_join_error)?
    }

    async fn update_note(&self, note: &Note) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let note = note.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<()> {
            let guard = conn.lock().unwrap();
            crate::store::note_cache::upsert_note(&guard, &note)
        })
        .await
        .map_err(map_join_error)?
    }

    async fn clear_column_notes(&self, column_id: &str) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let column_id = column_id.to_string();
        tauri::async_runtime::spawn_blocking(move || -> Result<()> {
            let guard = conn.lock().unwrap();
            guard.execute("DELETE FROM column_note WHERE column_id = ?1", rusqlite::params![column_id])?;
            guard.execute(
                "DELETE FROM column_fetch_boundary WHERE column_id = ?1",
                rusqlite::params![column_id],
            )?;
            Ok(())
        })
        .await
        .map_err(map_join_error)?
    }

    async fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>> {
        let conn = Arc::clone(&self.conn);
        let column_id = column_id.to_string();
        tauri::async_runtime::spawn_blocking(move || -> Result<Option<String>> {
            let guard = conn.lock().unwrap();
            let v: Option<String> = guard
                .query_row(
                    "SELECT oldest_fetched_id FROM column_fetch_boundary WHERE column_id = ?1",
                    rusqlite::params![column_id],
                    |r| r.get(0),
                )
                .optional()?;
            Ok(v)
        })
        .await
        .map_err(map_join_error)?
    }

    async fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let column_id = column_id.to_string();
        let new_oldest_id = new_oldest_id.to_string();
        tauri::async_runtime::spawn_blocking(move || -> Result<()> {
            let guard = conn.lock().unwrap();
            guard.execute(
                "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES (?1, ?2)
                 ON CONFLICT(column_id) DO UPDATE SET oldest_fetched_id = excluded.oldest_fetched_id",
                rusqlite::params![column_id, new_oldest_id],
            )?;
            Ok(())
        })
        .await
        .map_err(map_join_error)?
    }

    async fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let column_id = column_id.to_string();
        let new_oldest_id = new_oldest_id.to_string();
        tauri::async_runtime::spawn_blocking(move || -> Result<()> {
            let guard = conn.lock().unwrap();
            guard.execute(
                "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES (?1, ?2)
                 ON CONFLICT(column_id) DO UPDATE SET
                    oldest_fetched_id = MIN(oldest_fetched_id, excluded.oldest_fetched_id)",
                rusqlite::params![column_id, new_oldest_id],
            )?;
            Ok(())
        })
        .await
        .map_err(map_join_error)?
    }

    async fn clear_all_fetch_boundaries(&self) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        tauri::async_runtime::spawn_blocking(move || -> Result<()> {
            let guard = conn.lock().unwrap();
            guard.execute("DELETE FROM column_fetch_boundary", [])?;
            Ok(())
        })
        .await
        .map_err(map_join_error)?
    }

    async fn note_count(&self) -> Result<i32> {
        let conn = Arc::clone(&self.conn);
        tauri::async_runtime::spawn_blocking(move || -> Result<i32> {
            let guard = conn.lock().unwrap();
            let count: i32 = guard.query_row("SELECT COUNT(*) FROM note", [], |r| r.get(0))?;
            Ok(count)
        })
        .await
        .map_err(map_join_error)?
    }

    async fn notes_since(&self, since_epoch_secs: i32) -> Result<i32> {
        let conn = Arc::clone(&self.conn);
        tauri::async_runtime::spawn_blocking(move || -> Result<i32> {
            let guard = conn.lock().unwrap();
            let count: i32 = guard.query_row(
                "SELECT COUNT(*) FROM note WHERE created_at >= ?1",
                rusqlite::params![since_epoch_secs],
                |r| r.get(0),
            )?;
            Ok(count)
        })
        .await
        .map_err(map_join_error)?
    }

    async fn prune(&self, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize> {
        let conn = Arc::clone(&self.conn);
        tauri::async_runtime::spawn_blocking(move || -> Result<usize> {
            let guard = conn.lock().unwrap();
            crate::store::note_cache::prune_sync(&guard, keep, max_age_days, max_size_mb)
        })
        .await
        .map_err(map_join_error)?
    }

    async fn search_cache(
        &self,
        where_sql: &crate::filter::sql::SqlWhere,
        until_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Note>> {
        let conn = Arc::clone(&self.conn);
        // SqlWhereはCloneをderiveしていないためフィールドごとに複製する(Global Constraints参照)。
        let where_sql = crate::filter::sql::SqlWhere { sql: where_sql.sql.clone(), params: where_sql.params.clone() };
        let until_id = until_id.map(|s| s.to_string());
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<Note>> {
            let guard = conn.lock().unwrap();
            crate::store::note_cache::search_cache_sync(&guard, &where_sql, until_id.as_deref(), limit)
        })
        .await
        .map_err(map_join_error)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DriveFile, User, Visibility};
    use rusqlite::params;
    use std::collections::HashMap;

    fn store() -> SqliteBackend {
        SqliteBackend::new(crate::store::db::open_cache_in_memory().unwrap())
    }

    fn note(id: &str, created_at: i64) -> Note {
        Note {
            id: id.into(),
            created_at,
            text: Some("hello https://example.com #rust".into()),
            cw: None,
            visibility: Visibility::Home,
            local_only: false,
            user: User {
                id: "u1".into(),
                username: "alice".into(),
                host: None,
                name: Some("Alice".into()),
                avatar_url: None,
                is_bot: false,
                is_cat: false,
                followers_count: 5,
                following_count: 3,
                notes_count: 42,
                emojis: std::collections::HashMap::new(),
                bio: None,
                banner_url: None,
                instance: None,
            },
            reply_id: None,
            renote_id: None,
            renote: None,
            files: vec![DriveFile {
                id: "f1".into(),
                mime_type: "image/png".into(),
                is_sensitive: false,
                url: "http://x/f1".into(),
                thumbnail_url: None,
                name: "f1.png".into(),
            }],
            poll: None,
            tags: vec!["rust".into()],
            mentions: vec![],
            emojis: std::collections::HashMap::new(),
            channel_id: None,
            via: None,
            lang: None,
            reactions: HashMap::from([("👍".into(), 3u32)]),
            reaction_count: 3,
            renote_count: 1,
            reply_count: 0,
            my_reaction: Some("👍".into()),
            is_renoted_by_me: false,
            is_favorited_by_me: false,
            is_pinned: false,
        }
    }

    /// `note.emojis` を配列形式(移行前の旧フォーマット)に差し替えた payload JSON を作る。
    fn payload_with_array_emojis(n: &Note) -> String {
        let mut v = serde_json::to_value(n).unwrap();
        v["emojis"] = serde_json::json!(["old_style_name"]);
        serde_json::to_string(&v).unwrap()
    }

    #[tokio::test]
    async fn cache_roundtrip_preserves_note_and_order() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 150)]).await.unwrap();
        let got = s.load_cached("col1", 10).await.unwrap();
        // created_at 降順
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2", "n3", "n1"]);
        // payload 復元が完全（reactions/files/tags）
        assert_eq!(got[0].reactions.get("👍"), Some(&3));
        assert_eq!(got[0].files[0].mime_type, "image/png");
        assert_eq!(got[0].tags, vec!["rust".to_string()]);
        assert_eq!(got[0].my_reaction.as_deref(), Some("👍"));
    }

    /// 8dc26912 で `Note.emojis` が `Vec<String>` → `HashMap<String,String>` に変わった
    /// (Issue #150)。それ以前に保存された配列形式の payload が1件混ざっていても、
    /// その行だけスキップして残りは正常に読めること（カラム全滅にしない）。
    #[tokio::test]
    async fn load_cached_skips_row_with_legacy_array_emojis_payload() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200)]).await.unwrap();
        {
            let conn = s.conn().lock().unwrap();
            let legacy_payload = payload_with_array_emojis(&note("n1", 100));
            conn.execute("UPDATE note SET payload = ?1 WHERE id = 'n1'", params![legacy_payload]).unwrap();
        }

        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2"]);
    }

    /// load_cached と同様、search_cache も壊れた行1件で全体を空にしない。
    #[tokio::test]
    async fn search_cache_skips_row_with_legacy_array_emojis_payload() {
        use crate::filter::{parser, sql};
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200)]).await.unwrap();
        {
            let conn = s.conn().lock().unwrap();
            let legacy_payload = payload_with_array_emojis(&note("n1", 100));
            conn.execute("UPDATE note SET payload = ?1 WHERE id = 'n1'", params![legacy_payload]).unwrap();
        }

        let ctx = sql::SqlCtx { my_ids: vec![], following_ids: None };
        let expr = parser::parse_predicate("has_files").unwrap();
        let w = sql::build_where(&expr, &ctx).unwrap();
        let got = s.search_cache(&w, None, 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2"]);
    }

    #[tokio::test]
    async fn get_note_returns_none_when_not_cached() {
        let s = store();
        assert!(s.get_note("missing").await.unwrap().is_none());
    }

    /// 旧形式(配列)の emojis payload は「読めないので未キャッシュ扱い」とし、
    /// Err で呼び出し元(react/unreact/noteUpdated反映)を永続的に沈黙させない(Issue #150)。
    #[tokio::test]
    async fn get_note_returns_none_for_row_with_legacy_array_emojis_payload() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100)]).await.unwrap();
        {
            let conn = s.conn().lock().unwrap();
            let legacy_payload = payload_with_array_emojis(&note("n1", 100));
            conn.execute("UPDATE note SET payload = ?1 WHERE id = 'n1'", params![legacy_payload]).unwrap();
        }

        assert!(s.get_note("n1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn update_note_persists_without_column_note_and_get_note_reflects_it() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100)]).await.unwrap();

        let mut n = s.get_note("n1").await.unwrap().unwrap();
        n.reactions.insert("😀".into(), 1);
        n.reaction_count += 1;
        n.my_reaction = Some("😀".into());
        s.update_note(&n).await.unwrap();

        // update_note は column_note に触れないので、既存の所属は変わらない
        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].reactions.get("😀"), Some(&1));
        assert_eq!(got[0].reaction_count, 4); // 元の3 + 1
        assert_eq!(got[0].my_reaction.as_deref(), Some("😀"));

        // get_note 単体でも同じ内容が読める
        let single = s.get_note("n1").await.unwrap().unwrap();
        assert_eq!(single.reactions.get("😀"), Some(&1));
    }

    #[tokio::test]
    async fn search_cache_applies_predicate_and_until_id_boundary() {
        use crate::filter::{parser, sql};
        let s = store();
        s.cache_notes("col1", &[note("a1", 300), note("a2", 200), note("a3", 100)]).await.unwrap();

        let ctx = sql::SqlCtx { my_ids: vec![], following_ids: None };

        // 述語(has_files)は全件trueなので until_id 境界のみで絞られる
        let expr = parser::parse_predicate("has_files").unwrap();
        let w = sql::build_where(&expr, &ctx).unwrap();
        let got = s.search_cache(&w, Some("a3"), 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["a1", "a2"]);

        // 述語が全件falseなら空
        let expr2 = parser::parse_predicate("cw").unwrap();
        let w2 = sql::build_where(&expr2, &ctx).unwrap();
        assert!(s.search_cache(&w2, None, 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn upsert_replaces_and_relations_not_duplicated() {
        let s = store();
        s.cache_note("col1", &note("n1", 100)).await.unwrap();
        s.cache_note("col1", &note("n1", 100)).await.unwrap(); // 再受信
        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.len(), 1); // 重複しない
        // 関連テーブルも重複していない
        let conn = s.conn().lock().unwrap();
        let rc: i64 =
            conn.query_row("SELECT COUNT(*) FROM note_reaction WHERE note_id='n1'", [], |r| r.get(0)).unwrap();
        assert_eq!(rc, 1);
    }

    #[tokio::test]
    async fn column_isolation_and_clear() {
        let s = store();
        s.cache_note("colA", &note("n1", 100)).await.unwrap();
        s.cache_note("colB", &note("n2", 100)).await.unwrap();
        assert_eq!(s.load_cached("colA", 10).await.unwrap().len(), 1);
        assert_eq!(s.load_cached("colB", 10).await.unwrap().len(), 1);
        s.clear_column_notes("colA").await.unwrap();
        assert_eq!(s.load_cached("colA", 10).await.unwrap().len(), 0);
        assert_eq!(s.load_cached("colB", 10).await.unwrap().len(), 1); // 他カラムは残る
    }

    #[tokio::test]
    async fn fetch_boundary_roundtrip() {
        let s = store();
        assert!(s.get_fetch_boundary("col1").await.unwrap().is_none());

        s.set_fetch_boundary("col1", "n100").await.unwrap();
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n100"));
    }

    #[tokio::test]
    async fn set_fetch_boundary_overwrites_unconditionally() {
        let s = store();
        s.set_fetch_boundary("col1", "n100").await.unwrap();
        s.set_fetch_boundary("col1", "n999").await.unwrap(); // より新しい値でも無条件に上書き
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n999"));
    }

    #[tokio::test]
    async fn extend_fetch_boundary_only_moves_older() {
        let s = store();
        s.set_fetch_boundary("col1", "n500").await.unwrap();

        // より古い値(n300)への延長は反映される
        s.extend_fetch_boundary("col1", "n300").await.unwrap();
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n300"));

        // より新しい値(n800)は無視される(単調性)
        s.extend_fetch_boundary("col1", "n800").await.unwrap();
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n300"));
    }

    #[tokio::test]
    async fn extend_fetch_boundary_sets_when_absent() {
        let s = store();
        assert!(s.get_fetch_boundary("col1").await.unwrap().is_none());
        s.extend_fetch_boundary("col1", "n300").await.unwrap();
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n300"));
    }

    #[tokio::test]
    async fn clear_column_notes_also_removes_boundary() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100)]).await.unwrap();
        s.set_fetch_boundary("col1", "n1").await.unwrap();

        s.clear_column_notes("col1").await.unwrap();

        assert!(s.get_fetch_boundary("col1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn load_cached_before_returns_notes_older_than_until_id_desc() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).await.unwrap();

        let got = s.load_cached_before("col1", "n3", 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2", "n1"]);
    }

    #[tokio::test]
    async fn load_cached_before_respects_limit_and_column_scope() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).await.unwrap();
        s.cache_notes("col2", &[note("m1", 250)]).await.unwrap();

        let got = s.load_cached_before("col1", "n3", 1).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2"]);

        // col2 のノートは混ざらない
        let got_all = s.load_cached_before("col1", "n3", 10).await.unwrap();
        assert!(got_all.iter().all(|n| n.id != "m1"));
    }

    /// 連合ノートは id(受信順) と created_at(発信元での投稿時刻) の順序が食い違いうる。
    /// LIMIT の選抜は必ず id 基準で行い、created_at が古いという理由だけで
    /// 範囲内のノートが脱落しないこと(Issue #228)。
    #[tokio::test]
    async fn load_cached_before_selects_by_id_not_created_at() {
        let s = store();
        // id順: n1 < n2 < n3、created_at順: n2(100) < n3(800) < n1(900)
        s.cache_notes("col1", &[note("n1", 900), note("n2", 100), note("n3", 800)]).await.unwrap();

        let got = s.load_cached_before("col1", "n9", 2).await.unwrap();
        // id の大きい方から2件(n3, n2)が選抜される。created_at で選ぶと n1, n3 になり n2 が欠落する。
        let ids: Vec<&str> = got.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["n3", "n2"], "id基準で選抜し created_at DESC で並べること");
    }

    #[tokio::test]
    async fn prune_removes_oldest_beyond_keep_and_related_rows() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).await.unwrap();
        let deleted = s.prune(2, 0, 0).await.unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(s.note_count().await.unwrap(), 2);
        // 最古(n1)が消え、残りは新しい2件
        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n3", "n2"]);
        // 関連テーブル・column_note も一緒に消えていること
        let conn = s.conn().lock().unwrap();
        let rc: i64 =
            conn.query_row("SELECT COUNT(*) FROM note_reaction WHERE note_id='n1'", [], |r| r.get(0)).unwrap();
        assert_eq!(rc, 0);
        let cn: i64 =
            conn.query_row("SELECT COUNT(*) FROM column_note WHERE note_id='n1'", [], |r| r.get(0)).unwrap();
        assert_eq!(cn, 0);
    }

    #[tokio::test]
    async fn prune_is_noop_when_under_or_unlimited() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200)]).await.unwrap();
        assert_eq!(s.prune(10, 0, 0).await.unwrap(), 0); // 上限未満
        assert_eq!(s.prune(0, 0, 0).await.unwrap(), 0); // 全て無制限
        assert_eq!(s.note_count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn prune_removes_notes_older_than_max_age_days() {
        let s = store();
        let now = crate::store::note_cache::now_epoch();
        let one_day = 86_400;
        s.cache_notes("col1", &[note("old", now - 40 * one_day), note("recent", now - one_day)]).await.unwrap();
        let deleted = s.prune(0, 30, 0).await.unwrap();
        assert_eq!(deleted, 1);
        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["recent"]);
    }

    #[tokio::test]
    async fn prune_shrinks_db_below_max_size_mb() {
        let s = store();
        // 十分な件数×サイズを入れて 1MB を確実に超えさせ、上限指定で
        // 実際に削除・縮小(incremental_vacuum)が働くことを確認する。
        let notes: Vec<Note> = (0..1000)
            .map(|i| {
                let mut n = note(&format!("n{i}"), 100 + i as i64);
                n.text = Some("x".repeat(2000));
                n
            })
            .collect();
        s.cache_notes("col1", &notes).await.unwrap();
        let before_count = s.note_count().await.unwrap();
        let before_size = {
            let conn = s.conn().lock().unwrap();
            db_size_bytes_for_test(&conn)
        };
        assert!(before_size > 1024 * 1024, "test setup should exceed 1MB, got {before_size}");

        let deleted = s.prune(0, 0, 1).await.unwrap();
        assert!(deleted > 0);
        assert!(s.note_count().await.unwrap() < before_count);
        let after_size = {
            let conn = s.conn().lock().unwrap();
            db_size_bytes_for_test(&conn)
        };
        assert!(after_size < before_size);
    }

    fn db_size_bytes_for_test(conn: &Connection) -> i64 {
        let page_count: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap();
        let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0)).unwrap();
        page_count * page_size
    }

    #[tokio::test]
    async fn prune_raises_boundary_to_surviving_oldest_note_after_keep_exceeded() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).await.unwrap();
        s.set_fetch_boundary("col1", "n1").await.unwrap(); // n1まで(=全件)取得済みと主張

        let deleted = s.prune(2, 0, 0).await.unwrap(); // 最古のn1が削除される
        assert_eq!(deleted, 1);

        // n1が消えたので、生存最古のn2まで境界を引き上げる
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n2"));
    }

    /// created_at は古いが id は生存最古より新しいノート(連合ノート)が prune で消えた場合、
    /// 生存最古ID だけでは境界を引き上げられない。削除されたノートの最大IDまで引き上げること(Issue #228)。
    #[tokio::test]
    async fn prune_raises_boundary_past_deleted_note_with_newer_id() {
        let s = store();
        // id順: n1 < n2 < n5、created_at順: n5(100) < n1(200) < n2(300)
        s.cache_notes("col1", &[note("n5", 100), note("n1", 200), note("n2", 300)]).await.unwrap();
        s.set_fetch_boundary("col1", "n1").await.unwrap();

        let deleted = s.prune(2, 0, 0).await.unwrap(); // created_at 最古の n5 が削除される
        assert_eq!(deleted, 1);

        // 生存最古IDは n1 のままなので、削除された n5 まで境界を引き上げる必要がある
        assert_eq!(s.get_fetch_boundary("col1").await.unwrap().as_deref(), Some("n5"));
    }

    #[tokio::test]
    async fn clear_all_fetch_boundaries_removes_every_column() {
        let s = store();
        s.set_fetch_boundary("col1", "n100").await.unwrap();
        s.set_fetch_boundary("col2", "n200").await.unwrap();

        s.clear_all_fetch_boundaries().await.unwrap();

        assert!(s.get_fetch_boundary("col1").await.unwrap().is_none());
        assert!(s.get_fetch_boundary("col2").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn prune_clears_boundary_when_column_fully_pruned() {
        let s = store();
        let now = crate::store::note_cache::now_epoch();
        let one_day = 86_400;
        s.cache_notes("col1", &[note("old", now - 40 * one_day)]).await.unwrap();
        s.set_fetch_boundary("col1", "old").await.unwrap();

        let deleted = s.prune(0, 30, 0).await.unwrap();
        assert_eq!(deleted, 1);

        // カラムのキャッシュが全滅したので境界は未確定に戻る
        assert!(s.get_fetch_boundary("col1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn prune_leaves_unaffected_columns_boundary_untouched() {
        let s = store();
        s.cache_notes("colA", &[note("a1", 50)]).await.unwrap();
        s.cache_notes("colB", &[note("b1", 100), note("b2", 200), note("b3", 300)]).await.unwrap();
        s.set_fetch_boundary("colA", "a1").await.unwrap();
        s.set_fetch_boundary("colB", "b1").await.unwrap();

        let deleted = s.prune(3, 0, 0).await.unwrap(); // 4件中keep=3 → 全体最古のa1のみ削除
        assert_eq!(deleted, 1);

        assert!(s.get_fetch_boundary("colA").await.unwrap().is_none());
        assert_eq!(s.get_fetch_boundary("colB").await.unwrap().as_deref(), Some("b1")); // 変わらない
    }

    /// 旧形式(userフルオブジェクト埋め込み)の行を素のSQLで作る(upsert_noteを経由しない=
    /// Issue #263 以前に保存された実データの形を再現する)。
    fn insert_legacy_row(conn: &Connection, note_id: &str, created_at: i64, user_json: serde_json::Value) {
        let mut n = note(note_id, created_at);
        let mut v = serde_json::to_value(&n).unwrap();
        v["user"] = user_json;
        let payload = serde_json::to_string(&v).unwrap();
        n.user.id = v["user"]["id"].as_str().unwrap_or("").to_string();
        conn.execute(
            "INSERT INTO note (
                id, created_at, text, text_length, cw, visibility, local_only, user_id,
                reply_id, reply_user_id, renote_id, channel_id, via, lang,
                files_count, has_poll, has_link, is_pinned,
                reaction_count, renote_count, reply_count, my_reaction,
                is_renoted_by_me, is_favorited_by_me, payload
            ) VALUES (?1, ?2, '', 0, NULL, 'home', 0, ?3, NULL, NULL, NULL, NULL, NULL, NULL,
                0, 0, 0, 0, 0, 0, 0, NULL, 0, 0, ?4)",
            params![note_id, created_at, n.user.id, payload],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO column_note (column_id, note_id, received_at, created_at) VALUES ('col1', ?1, 0, ?2)",
            params![note_id, created_at],
        )
        .unwrap();
    }

    #[tokio::test]
    async fn load_cached_self_heals_legacy_full_user_payload() {
        let s = store();
        {
            let conn = s.conn().lock().unwrap();
            insert_legacy_row(
                &conn,
                "n_legacy",
                100,
                serde_json::json!({
                    "id": "u_legacy", "username": "carol", "host": "remote.example",
                    "name": "Carol", "avatarUrl": null, "isBot": false, "isCat": false,
                    "followersCount": 0, "followingCount": 0, "notesCount": 0,
                    "emojis": {}, "bio": null, "bannerUrl": null,
                    "instance": { "name": "Remote", "iconUrl": "https://remote.example/icon.png", "themeColor": "#ff8800" }
                }),
            );
        }

        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].user.id, "u_legacy");
        let instance = got[0].user.instance.as_ref().expect("instance should be hydrated");
        assert_eq!(instance.name.as_deref(), Some("Remote"));

        // payload がスタブ形式へ書き戻されていること
        let conn = s.conn().lock().unwrap();
        let raw: String = conn.query_row("SELECT payload FROM note WHERE id = 'n_legacy'", [], |r| r.get(0)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["user"], serde_json::json!({ "id": "u_legacy" }));

        // user テーブルへ抽出されていること
        let name: String = conn.query_row("SELECT name FROM user WHERE id = 'u_legacy'", [], |r| r.get(0)).unwrap();
        assert_eq!(name, "Carol");
    }

    #[tokio::test]
    async fn load_cached_self_heals_legacy_renote_author_instance() {
        let s = store();
        {
            let conn = s.conn().lock().unwrap();
            // 本体(u_main)も renote元(u_renote_author)も旧形式(userフルオブジェクト埋め込み)。
            // renote元著者にはinstance情報が付いている。
            let mut n = note("n_with_renote", 200);
            let mut v = serde_json::to_value(&n).unwrap();
            v["user"] = serde_json::json!({
                "id": "u_main", "username": "mainuser", "host": null, "name": "Main User",
                "avatarUrl": null, "isBot": false, "isCat": false,
                "followersCount": 0, "followingCount": 0, "notesCount": 0,
                "emojis": {}, "bio": null, "bannerUrl": null, "instance": null
            });
            v["renote"] = serde_json::json!({
                "id": "n_renoted", "createdAt": 100, "text": "original", "cw": null,
                "visibility": "public", "localOnly": false,
                "user": {
                    "id": "u_renote_author", "username": "renoteauthor", "host": "remote.example",
                    "name": "Renote Author", "avatarUrl": null, "isBot": false, "isCat": false,
                    "followersCount": 0, "followingCount": 0, "notesCount": 0,
                    "emojis": {}, "bio": null, "bannerUrl": null,
                    "instance": { "name": "Remote", "iconUrl": "https://remote.example/icon.png", "themeColor": "#ff8800" }
                },
                "replyId": null, "renoteId": null, "renote": null, "files": [], "poll": null,
                "tags": [], "mentions": [], "emojis": {}, "channelId": null, "via": null, "lang": null,
                "reactions": {}, "reactionCount": 0, "renoteCount": 0, "replyCount": 0,
                "myReaction": null, "isRenotedByMe": false, "isFavoritedByMe": false, "isPinned": false
            });
            n.id = "n_with_renote".to_string();
            let payload = serde_json::to_string(&v).unwrap();
            conn.execute(
                "INSERT INTO note (
                    id, created_at, text, text_length, cw, visibility, local_only, user_id,
                    reply_id, reply_user_id, renote_id, channel_id, via, lang,
                    files_count, has_poll, has_link, is_pinned,
                    reaction_count, renote_count, reply_count, my_reaction,
                    is_renoted_by_me, is_favorited_by_me, payload
                ) VALUES ('n_with_renote', 200, '', 0, NULL, 'home', 0, 'u_main', NULL, NULL, 'n_renoted', NULL, NULL, NULL,
                    0, 0, 0, 0, 0, 0, 0, NULL, 0, 0, ?1)",
                params![payload],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO column_note (column_id, note_id, received_at, created_at) VALUES ('col1', 'n_with_renote', 0, 200)",
                [],
            )
            .unwrap();
        }

        let got = s.load_cached("col1", 10).await.unwrap();
        assert_eq!(got.len(), 1);
        let renote = got[0].renote.as_ref().expect("renote should be present");
        let instance = renote.user.instance.as_ref().expect("renote author instance should be hydrated");
        assert_eq!(instance.name.as_deref(), Some("Remote"));

        // 両方(本体+renote分)がuserテーブルへ抽出されていること
        let conn = s.conn().lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM user WHERE id IN ('u_main','u_renote_author')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);

        // payloadが本体+renote分ともスタブへ書き戻されていること
        let raw: String =
            conn.query_row("SELECT payload FROM note WHERE id = 'n_with_renote'", [], |r| r.get(0)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["user"], serde_json::json!({ "id": "u_main" }));
        assert_eq!(v["renote"]["user"], serde_json::json!({ "id": "u_renote_author" }));
    }

    #[tokio::test]
    async fn load_cached_skips_note_when_referenced_user_row_missing() {
        let s = store();
        {
            let conn = s.conn().lock().unwrap();
            insert_legacy_row(&conn, "n_orphan", 100, serde_json::json!({ "id": "u_orphan" }));
        }

        let got = s.load_cached("col1", 10).await.unwrap();
        assert!(got.is_empty());
    }

    #[tokio::test]
    async fn normalized_columns_populated_for_nql() {
        let s = store();
        s.cache_note("col1", &note("n1", 100)).await.unwrap();
        let conn = s.conn().lock().unwrap();
        // has_link / text_length / files_count 等が正規化カラムに入る
        let (has_link, files_count): (i64, i64) = conn
            .query_row("SELECT has_link, files_count FROM note WHERE id='n1'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(has_link, 1);
        assert_eq!(files_count, 1);
        let cat: String =
            conn.query_row("SELECT mime_category FROM note_file WHERE note_id='n1'", [], |r| r.get(0)).unwrap();
        assert_eq!(cat, "image");
    }

    /// Issue #115: 側テーブルをDELETE+INSERTからUPSERTに変更した後も、
    /// 「今のnoteに無くなった行(取り消されたリアクション等)」は正しく消えること。
    #[tokio::test]
    async fn upsert_note_removes_stale_reaction_after_unreact() {
        let s = store();
        let mut n = note("n1", 100);
        n.reactions = HashMap::from([("👍".into(), 3u32)]);
        s.cache_note("col1", &n).await.unwrap();

        // リアクションが取り消された(reactionsが空になった)状態で再受信
        n.reactions = HashMap::new();
        n.reaction_count = 0;
        n.my_reaction = None;
        s.update_note(&n).await.unwrap();

        let conn = s.conn().lock().unwrap();
        let rc: i64 =
            conn.query_row("SELECT COUNT(*) FROM note_reaction WHERE note_id='n1'", [], |r| r.get(0)).unwrap();
        assert_eq!(rc, 0, "取り消されたリアクションの行はUPSERT化後も削除されること");
    }

    /// Issue #115: 同じリアクションを再受信してもcountが正しく更新される(UPSERTのON CONFLICT DO UPDATEが効いていること)。
    #[tokio::test]
    async fn upsert_note_updates_reaction_count_on_upsert() {
        let s = store();
        let mut n = note("n1", 100);
        n.reactions = HashMap::from([("👍".into(), 3u32)]);
        s.cache_note("col1", &n).await.unwrap();

        n.reactions = HashMap::from([("👍".into(), 5u32)]);
        s.update_note(&n).await.unwrap();

        let conn = s.conn().lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT count FROM note_reaction WHERE note_id='n1' AND emoji_key='👍'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 5);
    }
}
