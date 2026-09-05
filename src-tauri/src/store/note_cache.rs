//! ノートキャッシュの読み書き（TQL§9 の正規化テーブル + 表示復元用 payload）。
//! 設定(`SettingsStore`)とは別ファイル・別接続で持つ（バックアップ対象を小さな設定DBに絞るため）。
//!
//! Issue #115 Phase 1: バックエンドを `NoteCacheBackend` トレイトとして抽出し、
//! 具体実装(`SqliteBackend`、将来的な `PostgresBackend`/`MySqlBackend`)へ差し替え可能にした。
//! `rusqlite` は同期APIのため、`SqliteBackend`(`store/sqlite_backend.rs`)は
//! `tauri::async_runtime::spawn_blocking` で包んで非同期トレイトメソッドとして提供する。

use crate::domain::{Note, Visibility};
use crate::error::Result;
use crate::store::user_ref::{
    collect_user_id_refs, collect_users, fetch_users_by_ids, fill_user_from_snapshot,
    has_legacy_full_user, hydrate_user_refs, is_legacy_full_user, stub_user_refs, upsert_user,
};
use rusqlite::{params, Connection};

pub(crate) fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn visibility_str(v: Visibility) -> &'static str {
    match v {
        Visibility::Public => "public",
        Visibility::Home => "home",
        Visibility::Followers => "followers",
        Visibility::Specified => "specified",
    }
}

fn mime_category(mime: &str) -> &str {
    mime.split('/').next().unwrap_or("other")
}

fn has_url(text: &str) -> bool {
    text.contains("http://") || text.contains("https://")
}

#[async_trait::async_trait]
pub(crate) trait NoteCacheBackend: Send + Sync {
    async fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()>;
    async fn cache_note(&self, column_id: &str, note: &Note) -> Result<()>;
    async fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>>;
    async fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>>;
    async fn get_note(&self, note_id: &str) -> Result<Option<Note>>;
    async fn update_note(&self, note: &Note) -> Result<()>;
    async fn clear_column_notes(&self, column_id: &str) -> Result<()>;
    async fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>>;
    async fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()>;
    async fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()>;
    async fn clear_all_fetch_boundaries(&self) -> Result<()>;
    async fn note_count(&self) -> Result<i32>;
    async fn notes_since(&self, since_epoch_secs: i32) -> Result<i32>;
    async fn prune(&self, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize>;
    async fn search_cache(
        &self,
        where_sql: &crate::filter::sql::SqlWhere,
        until_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Note>>;
}

/// note cacheの公開API。内部の`NoteCacheBackend`実装(`SqliteBackend`、将来的な
/// `PostgresBackend`/`MySqlBackend`)へ委譲する薄いラッパー。
pub struct NoteCacheStore {
    backend: std::sync::Mutex<std::sync::Arc<dyn NoteCacheBackend>>,
}

impl NoteCacheStore {
    /// テスト専用コンストラクタ(SQLiteバックエンドを直接渡す用途)。本番コードは
    /// 常に`new_from_arc`(起動時に設定に応じてSqlite/Postgresを事前に選んだ
    /// `Arc<dyn NoteCacheBackend>`を渡す)経由でのみ構築する。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new(backend: impl NoteCacheBackend + 'static) -> Self {
        Self { backend: std::sync::Mutex::new(std::sync::Arc::new(backend)) }
    }

    /// 既に`Arc`化済みのバックエンドから構築する(起動時、設定に応じてSqlite/Postgresの
    /// いずれかを事前に選んで`Arc<dyn NoteCacheBackend>`にした後、それをそのまま渡す用途)。
    pub fn new_from_arc(backend: std::sync::Arc<dyn NoteCacheBackend>) -> Self {
        Self { backend: std::sync::Mutex::new(backend) }
    }

    /// バックエンドを即時差し替える。ロックは一瞬だけ保持し(`.await`をまたがない)、
    /// 差し替え中に進行中の呼び出しは古いバックエンド(cloneされた`Arc`)のまま
    /// 完走する(設計書「バックエンド抽象化」節参照)。
    pub fn swap_backend(&self, new_backend: std::sync::Arc<dyn NoteCacheBackend>) {
        *self.backend.lock().unwrap() = new_backend;
    }

    fn backend(&self) -> std::sync::Arc<dyn NoteCacheBackend> {
        std::sync::Arc::clone(&self.backend.lock().unwrap())
    }

    /// ノート群をキャッシュへ upsert し、カラム所属を記録する（1トランザクション）。
    pub async fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()> {
        self.backend().cache_notes(column_id, notes).await
    }

    /// 1件のノートをキャッシュ（Streaming 受信時に使う）。
    pub async fn cache_note(&self, column_id: &str, note: &Note) -> Result<()> {
        self.backend().cache_note(column_id, note).await
    }

    /// カラムの直近ノートをキャッシュから取得（新しい順・最大 limit）。
    pub async fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>> {
        self.backend().load_cached(column_id, limit).await
    }

    /// カラムのキャッシュから until_id より古いノートを取得（新しい順、最大 limit 件）。
    /// backfill のキャッシュ優先パス用（load_cached の until_id 版）。
    pub async fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>> {
        self.backend().load_cached_before(column_id, until_id, limit).await
    }

    /// note_id 単体をキャッシュから取得する（column_note を経由しない）。
    /// 自分のリアクション操作やstreamingのnoteUpdatedをキャッシュへ反映する際、
    /// 対象ノートがどのカラムに属すか気にせず読み書きするために使う。
    pub async fn get_note(&self, note_id: &str) -> Result<Option<Note>> {
        self.backend().get_note(note_id).await
    }

    /// 1件のノートのキャッシュ内容を更新する（column_note には触れない）。
    /// 自分のリアクション操作やstreamingのnoteUpdatedをキャッシュへ反映するために使う。
    /// 対象がまだキャッシュに無ければ何もしない想定の呼び出し元(get_noteでSomeを確認済み)向け。
    pub async fn update_note(&self, note: &Note) -> Result<()> {
        self.backend().update_note(note).await
    }

    /// カラム所属レコードを消す（カラム削除時。note 本体は他カラムと共有しうるので残す）。
    pub async fn clear_column_notes(&self, column_id: &str) -> Result<()> {
        self.backend().clear_column_notes(column_id).await
    }

    /// カラムの境界(oldest_fetched_id)を取得。未確定ならNone。
    pub async fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>> {
        self.backend().get_fetch_boundary(column_id).await
    }

    /// 境界を new_oldest_id で無条件に新規セット/上書きする(初回REST取得時に使う)。
    pub async fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        self.backend().set_fetch_boundary(column_id, new_oldest_id).await
    }

    /// 境界を new_oldest_id まで延長する(古い方向へのみ、単調性を保証)。
    /// 既存値の方が既に古ければ何もしない。
    pub async fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        self.backend().extend_fetch_boundary(column_id, new_oldest_id).await
    }

    /// 全カラムのbackfill境界を削除する(未確定状態に戻す)。ミュート設定変更時など、
    /// キャッシュされたフィルタ済みノート集合の前提が崩れる操作の後に呼ぶ(Issue #228)。
    pub async fn clear_all_fetch_boundaries(&self) -> Result<()> {
        self.backend().clear_all_fetch_boundaries().await
    }

    /// キャッシュ済みノートの総数。Backstageのステータス表示用。
    /// specta が i64 の直接エクスポートを禁止するため i32 で返す(ローカルキャッシュ件数が
    /// 21億件を超えることは実運用上ない)。
    pub async fn note_count(&self) -> Result<i32> {
        self.backend().note_count().await
    }

    /// 投稿日時(created_at, epoch秒)が since_epoch_secs 以降のノート件数。
    /// 流速表示用: DBへのINSERT件数ではなく実際の投稿時刻で数えるため、起動時ギャップ埋めや
    /// 上スクロールでの過去取得(古いcreated_atのノートをまとめてupsertする)による誤った
    /// 跳ね上がりが起きない。idx_note_created を使う。
    pub async fn notes_since(&self, since_epoch_secs: i32) -> Result<i32> {
        self.backend().notes_since(since_epoch_secs).await
    }

    /// キャッシュを間引く（Issue #6: 無制限に溜まり続けないようにする）。3つの上限を順に適用する:
    /// 1. `max_age_days` 日より古いノートを削除
    /// 2. 件数が `keep` を超えていれば古い順に削除
    /// 3. DBサイズが `max_size_mb` を超えていれば古い順に削除（incremental_vacuumで実サイズへ反映）
    ///
    /// 各上限は `<= 0` で無効（無制限）。戻り値は実際に削除した件数の合計。
    pub async fn prune(&self, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize> {
        self.backend().prune(keep, max_age_days, max_size_mb).await
    }

    /// TQL `cache` ソース: ローカルSQLiteキャッシュ全体を where 句で検索する（受信せず検索のみ）。
    /// until_id は作成順の境界（id 自体は sortable なので created_at の代わりに使える）。
    pub async fn search_cache(
        &self,
        where_sql: &crate::filter::sql::SqlWhere,
        until_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Note>> {
        self.backend().search_cache(where_sql, until_id, limit).await
    }
}

/// (note_id, payload) の行群を Note へ復元する(Issue #263)。
/// - 旧形式(userフルオブジェクト埋め込み)を検知した行は、その場で user テーブルへ抽出し
///   payload をスタブ形式へ書き戻してから復元する(自己修復)。
/// - user参照(本体+renote分)が user テーブルに見つからない行は、ログ警告してスキップする
///   (呼び出し元をエラーにしない。deserialize_note_or_warn と同じポリシー)。
pub(crate) fn resolve_payload_rows(conn: &Connection, rows: Vec<(String, String)>) -> Result<Vec<Note>> {
    let mut values: Vec<(String, serde_json::Value)> = Vec::with_capacity(rows.len());
    for (id, payload) in rows {
        match serde_json::from_str::<serde_json::Value>(&payload) {
            Ok(mut v) => {
                if has_legacy_full_user(&v) {
                    self_heal_legacy_row(conn, &id, &mut v)?;
                }
                values.push((id, v));
            }
            Err(e) => {
                log::warn!("skipping note cache row {id} with unparsable payload: {e}");
            }
        }
    }

    let mut ids = Vec::new();
    for (_, v) in &values {
        collect_user_id_refs(v, &mut ids);
    }
    ids.sort();
    ids.dedup();
    let users = fetch_users_by_ids(conn, &ids)?;

    let mut out = Vec::with_capacity(values.len());
    for (id, mut v) in values {
        if !hydrate_user_refs(&mut v, &users) {
            log::warn!("skipping note cache row {id}: referenced user not found in user table");
            continue;
        }
        match serde_json::from_value::<Note>(v) {
            Ok(note) => out.push(note),
            Err(e) => log::warn!("skipping note cache row {id} with undeserializable payload: {e}"),
        }
    }
    Ok(out)
}

/// 旧形式(userフルオブジェクト埋め込み)の行を検知した際、抽出できた user を
/// user テーブルへ upsert し、抽出できた箇所だけ payload をスタブ形式へ書き戻す
/// (抽出に失敗した箇所は元のまま残す。中途半端な書き換えで既存データを失わないため)。
fn self_heal_legacy_row(conn: &Connection, note_id: &str, value: &mut serde_json::Value) -> Result<()> {
    let changed = self_heal_node(conn, value)?;
    if changed {
        let new_payload = serde_json::to_string(value)?;
        conn.execute("UPDATE note SET payload = ?1 WHERE id = ?2", params![new_payload, note_id])?;
    }
    Ok(())
}

/// 1ノード分(本体 or renote)の user を自己修復する。renote へ再帰する。
/// 戻り値: このノード以下で1箇所でも書き換えたら true。
fn self_heal_node(conn: &Connection, node: &mut serde_json::Value) -> Result<bool> {
    let mut changed = false;
    if let Some(user_value) = node.get("user").cloned() {
        if is_legacy_full_user(&user_value) {
            if let Ok(user) = serde_json::from_value::<crate::domain::User>(user_value.clone()) {
                fill_user_from_snapshot(conn, &user)?;
                if let Some(id) = user_value.get("id").cloned() {
                    node["user"] = serde_json::json!({ "id": id });
                    changed = true;
                }
            }
        }
    }
    if node.get("renote").map(|r| r.is_object()).unwrap_or(false) {
        changed |= self_heal_node(conn, &mut node["renote"])?;
    }
    Ok(changed)
}

/// `select_sql`（`SELECT id FROM note ...` 形式）にマッチするノートと、その関連テーブル
/// （note_reaction 等）・column_note を削除する（FK制約は張っていないため手動カスケード）。
/// 削除によって影響を受けたカラムの backfill 境界(column_fetch_boundary)も、生存している
/// 最古ノートIDまで引き上げる（全滅したカラムは境界ごと削除）。境界が「削除前の完全な範囲」
/// を主張したままだと、prune後にキャッシュに無いノートを「完全」と誤認して欠落表示になる
/// ため(Issue #228)。
/// 戻り値は削除したノート件数。
fn delete_matching(conn: &Connection, select_sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<i64> {
    conn.execute(&format!("CREATE TEMP TABLE prune_ids AS {select_sql}"), params)?;
    let deleted = conn.execute("DELETE FROM note WHERE id IN (SELECT id FROM prune_ids)", [])? as i64;

    // column_note をカスケード削除する前に、影響を受けるカラムと、各カラムで削除された
    // ノートの中の最大ID(=境界を少なくともこの値までは引き上げる必要がある)を確定しておく。
    let affected_columns: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT column_id FROM column_note WHERE note_id IN (SELECT id FROM prune_ids)",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let max_deleted_by_column: std::collections::HashMap<String, String> = {
        let mut stmt = conn.prepare(
            "SELECT column_id, MAX(note_id) FROM column_note
             WHERE note_id IN (SELECT id FROM prune_ids) GROUP BY column_id",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?
    };

    for table in ["column_note", "note_reaction", "note_tag", "note_mention", "note_emoji", "note_file"] {
        conn.execute(
            &format!("DELETE FROM {table} WHERE note_id IN (SELECT id FROM prune_ids)"),
            [],
        )?;
    }
    conn.execute("DROP TABLE prune_ids", [])?;

    for column_id in &affected_columns {
        let survivor: Option<String> = match conn
            .query_row(
                "SELECT MIN(note_id) FROM column_note WHERE column_id = ?1",
                params![column_id],
                |r| r.get::<_, Option<String>>(0),
            ) {
            Ok(v) => v,
            Err(e) => return Err(e.into()),
        };
        match survivor {
            Some(oldest) => {
                // 削除対象は created_at 基準で選ばれるため、生存最古IDより新しいIDのノートが
                // 消えていることがある(連合ノート)。その場合は削除された最大IDまで
                // 引き上げないと、消えた範囲を「完全」と主張し続けてしまう(Issue #228)。
                let candidate = match max_deleted_by_column.get(column_id) {
                    Some(max_deleted) if max_deleted.as_str() > oldest.as_str() => max_deleted.clone(),
                    _ => oldest,
                };
                conn.execute(
                    "UPDATE column_fetch_boundary SET oldest_fetched_id = ?2
                     WHERE column_id = ?1 AND oldest_fetched_id < ?2",
                    params![column_id, candidate],
                )?;
            }
            None => {
                conn.execute(
                    "DELETE FROM column_fetch_boundary WHERE column_id = ?1",
                    params![column_id],
                )?;
            }
        }
    }
    Ok(deleted)
}

/// `page_count * page_size` からDBの論理サイズ(バイト)を求める。
fn db_size_bytes(conn: &Connection) -> Result<i64> {
    let page_count: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    Ok(page_count * page_size)
}

/// DBサイズが `budget_bytes` を下回るまで、古いノートから削除して incremental_vacuum で
/// 実サイズへ反映する。DELETEだけでは解放ページがファイル内に留まり縮まらないため
/// （`open_cache` で auto_vacuum=INCREMENTAL を有効化している前提）。
/// 削除見積もりが外れても収束するよう最大3ラウンドまでとする。
fn shrink_to_size(conn: &Connection, budget_bytes: i64) -> Result<i64> {
    let mut deleted = 0i64;
    for _ in 0..3 {
        conn.execute_batch("PRAGMA incremental_vacuum")?;
        let size = db_size_bytes(conn)?;
        if size <= budget_bytes {
            break;
        }
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM note", [], |r| r.get(0))?;
        if total == 0 {
            break;
        }
        let over_ratio = (size - budget_bytes) as f64 / size as f64;
        let to_delete = ((total as f64) * over_ratio).ceil() as i64;
        let to_delete = to_delete.clamp(1, total);
        deleted += delete_matching(
            conn,
            "SELECT id FROM note ORDER BY created_at ASC, id ASC LIMIT ?1",
            params![to_delete],
        )?;
    }
    Ok(deleted)
}

/// note + user + 関連テーブルを upsert する。関連テーブルはUPSERT+失効行クリーンアップ(Issue #115)。
pub(crate) fn upsert_note(conn: &Connection, n: &Note) -> Result<()> {
    let mut payload_value = serde_json::to_value(n)?;
    stub_user_refs(&mut payload_value);
    let payload = serde_json::to_string(&payload_value)?;
    let text_length = n.text.as_deref().map(|t| t.chars().count()).unwrap_or(0) as i64;
    let has_link = n.text.as_deref().map(has_url).unwrap_or(false) as i64;

    // note行より先にuserをupsertする(Issue #263 最終レビュー指摘)。
    // upsert_note はトランザクション外で呼ばれることがあるため、クラッシュ時に
    // 「参照されないuser行が残るだけ」で済むようにし、「userの無いnote行が
    // 永久に読めなくなる」事態(hydrate_user_refs失敗でスキップされ続ける)を避ける。
    for user in collect_users(n) {
        upsert_user(conn, user)?;
    }

    conn.execute(
        "INSERT OR REPLACE INTO note (
            id, created_at, text, text_length, cw, visibility, local_only, user_id,
            reply_id, reply_user_id, renote_id, channel_id, via, lang,
            files_count, has_poll, has_link, is_pinned,
            reaction_count, renote_count, reply_count, my_reaction,
            is_renoted_by_me, is_favorited_by_me, payload
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
            ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18,
            ?19, ?20, ?21, ?22,
            ?23, ?24, ?25
        )",
        params![
            n.id,
            n.created_at,
            n.text,
            text_length,
            n.cw,
            visibility_str(n.visibility),
            n.local_only as i64,
            n.user.id,
            n.reply_id,
            Option::<String>::None, // reply_user_id: Note には無いため NULL（reply_to_me は限定的）
            n.renote_id,
            n.channel_id,
            n.via,
            n.lang,
            n.files.len() as i64,
            n.poll.is_some() as i64,
            has_link,
            n.is_pinned as i64,
            n.reaction_count,
            n.renote_count,
            n.reply_count,
            n.my_reaction,
            n.is_renoted_by_me as i64,
            n.is_favorited_by_me as i64,
            payload,
        ],
    )?;

    // 関連テーブルはUPSERT(Task 1で追加したUNIQUE制約に基づく)。DELETE+INSERTではなく
    // ON CONFLICTで書き換えることで、複数プロセスからの同時書き込みでも一時的な重複行・
    // 空状態が起きないようにする(Phase 2以降の外部DB利用を見据えた変更、Issue #115)。
    for (emoji, count) in &n.reactions {
        conn.execute(
            "INSERT INTO note_reaction (note_id, emoji_key, count) VALUES (?1, ?2, ?3)
             ON CONFLICT(note_id, emoji_key) DO UPDATE SET count = excluded.count",
            params![n.id, emoji, count],
        )?;
    }
    for tag in &n.tags {
        conn.execute(
            "INSERT INTO note_tag (note_id, tag) VALUES (?1, ?2) ON CONFLICT(note_id, tag) DO NOTHING",
            params![n.id, tag],
        )?;
    }
    for uid in &n.mentions {
        conn.execute(
            "INSERT INTO note_mention (note_id, user_id) VALUES (?1, ?2) ON CONFLICT(note_id, user_id) DO NOTHING",
            params![n.id, uid],
        )?;
    }
    for e in n.emojis.keys() {
        conn.execute(
            "INSERT INTO note_emoji (note_id, emoji) VALUES (?1, ?2) ON CONFLICT(note_id, emoji) DO NOTHING",
            params![n.id, e],
        )?;
    }
    for f in &n.files {
        conn.execute(
            "INSERT INTO note_file (note_id, mime_type, mime_category, is_sensitive) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(note_id, mime_type, mime_category, is_sensitive) DO NOTHING",
            params![n.id, f.mime_type, mime_category(&f.mime_type), f.is_sensitive as i64],
        )?;
    }

    // 旧行(現在のnoteの内容に含まれなくなったreaction/tag/mention/emoji/file)を掃除する。
    // 例: リアクションが取り消された、タグが編集で消えた、等。json_eachはSQLiteのJSON1拡張
    // (rusqliteのbundled機能で有効)を使う。
    conn.execute(
        "DELETE FROM note_reaction WHERE note_id = ?1 AND emoji_key NOT IN (SELECT value FROM json_each(?2))",
        params![n.id, serde_json::to_string(&n.reactions.keys().collect::<Vec<_>>())?],
    )?;
    conn.execute(
        "DELETE FROM note_tag WHERE note_id = ?1 AND tag NOT IN (SELECT value FROM json_each(?2))",
        params![n.id, serde_json::to_string(&n.tags)?],
    )?;
    conn.execute(
        "DELETE FROM note_mention WHERE note_id = ?1 AND user_id NOT IN (SELECT value FROM json_each(?2))",
        params![n.id, serde_json::to_string(&n.mentions)?],
    )?;
    conn.execute(
        "DELETE FROM note_emoji WHERE note_id = ?1 AND emoji NOT IN (SELECT value FROM json_each(?2))",
        params![n.id, serde_json::to_string(&n.emojis.keys().collect::<Vec<_>>())?],
    )?;
    // note_fileはUNIQUEキーが複合(4列)でjson_eachのタプル比較ができないため、行ごとに比較する。
    let current_file_keys: Vec<String> = n
        .files
        .iter()
        .map(|f| format!("{}\u{0}{}\u{0}{}", f.mime_type, mime_category(&f.mime_type), f.is_sensitive as i64))
        .collect();
    let mut stmt = conn.prepare("SELECT rowid, mime_type, mime_category, is_sensitive FROM note_file WHERE note_id = ?1")?;
    let existing_files: Vec<(i64, String, String, i64)> = stmt
        .query_map(params![n.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    for (rowid, mime_type, mime_category_val, is_sensitive) in existing_files {
        let key = format!("{mime_type}\u{0}{mime_category_val}\u{0}{is_sensitive}");
        if !current_file_keys.contains(&key) {
            conn.execute("DELETE FROM note_file WHERE rowid = ?1", params![rowid])?;
        }
    }
    Ok(())
}

/// `NoteCacheStore::prune` の同期本体。`SqliteBackend::prune` から `spawn_blocking` 内で呼ばれる。
pub(crate) fn prune_sync(conn: &Connection, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize> {
    let mut deleted: i64 = 0;
    {
        let tx_conn = conn;
        // note: 呼び出し元は `&Connection`（`MutexGuard<Connection>` を deref した参照）を渡すが、
        // トランザクションは可変参照が必要なため、ここでは `unchecked_transaction` を使う
        // （`rusqlite::Connection::unchecked_transaction` は `&self` で取得できる代わりに、
        // 呼び出し元が同時に別のトランザクションを開始しないことを保証する必要がある。
        // ここではロックを保持したまま単一のスレッドで呼ぶため安全）。
        let tx = tx_conn.unchecked_transaction()?;
        if max_age_days > 0 {
            let cutoff = now_epoch() - max_age_days as i64 * 86_400;
            deleted += delete_matching(&tx, "SELECT id FROM note WHERE created_at < ?1", params![cutoff])?;
        }
        if keep > 0 {
            let total: i64 = tx.query_row("SELECT COUNT(*) FROM note", [], |r| r.get(0))?;
            let overflow = total - keep as i64;
            if overflow > 0 {
                deleted += delete_matching(
                    &tx,
                    "SELECT id FROM note ORDER BY created_at ASC, id ASC LIMIT ?1",
                    params![overflow],
                )?;
            }
        }
        tx.commit()?;
    }
    if max_size_mb > 0 {
        deleted += shrink_to_size(conn, max_size_mb as i64 * 1024 * 1024)?;
    }
    Ok(deleted as usize)
}

/// `NoteCacheStore::search_cache` の同期本体。`SqliteBackend::search_cache` から
/// `spawn_blocking` 内で呼ばれる。
pub(crate) fn search_cache_sync(
    conn: &Connection,
    where_sql: &crate::filter::sql::SqlWhere,
    until_id: Option<&str>,
    limit: u32,
) -> Result<Vec<Note>> {
    use crate::filter::sql::SqlParam;

    let mut sql = String::from("SELECT n.id, n.payload FROM note n JOIN user u ON u.id = n.user_id WHERE (");
    sql.push_str(&where_sql.sql);
    sql.push(')');
    if until_id.is_some() {
        sql.push_str(" AND n.id < ?");
    }
    sql.push_str(" ORDER BY n.created_at DESC, n.id DESC LIMIT ?");

    let mut stmt = conn.prepare(&sql)?;
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    for p in &where_sql.params {
        binds.push(match p {
            SqlParam::Text(s) => Box::new(s.clone()),
            SqlParam::Real(x) => Box::new(*x),
        });
    }
    if let Some(u) = until_id {
        binds.push(Box::new(u.to_string()));
    }
    binds.push(Box::new(limit));
    let params_ref: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
    let rows: Vec<(String, String)> = stmt
        .query_map(params_ref.as_slice(), |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    resolve_payload_rows(conn, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DriveFile, User};
    use std::collections::HashMap;

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

    #[test]
    fn upsert_note_stores_stubbed_user_in_payload() {
        let conn = crate::store::db::open_cache_in_memory().unwrap();
        upsert_note(&conn, &note("n1", 100)).unwrap();

        let raw_payload: String =
            conn.query_row("SELECT payload FROM note WHERE id = 'n1'", [], |r| r.get(0)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw_payload).unwrap();
        assert_eq!(v["user"], serde_json::json!({ "id": "u1" }));
    }

    #[tokio::test]
    async fn swap_backend_switches_active_backend_for_subsequent_calls() {
        let backend_a = crate::store::sqlite_backend::SqliteBackend::new(
            crate::store::db::open_cache_in_memory().unwrap(),
        );
        let store = NoteCacheStore::new(backend_a);
        store.cache_note("col1", &note("n1", 100)).await.unwrap();
        assert_eq!(store.load_cached("col1", 10).await.unwrap().len(), 1);

        let backend_b = crate::store::sqlite_backend::SqliteBackend::new(
            crate::store::db::open_cache_in_memory().unwrap(),
        );
        store.swap_backend(std::sync::Arc::new(backend_b));

        // 切替後は新しい(空の)バックエンドを見ている
        assert_eq!(store.load_cached("col1", 10).await.unwrap().len(), 0);
    }

    #[test]
    fn upsert_note_upserts_both_note_and_renote_authors_into_user_table() {
        let conn = crate::store::db::open_cache_in_memory().unwrap();
        let mut n = note("n1", 100);
        n.renote = Some(Box::new({
            let mut renoted = note("n0", 50);
            renoted.user = User {
                id: "u2".into(),
                username: "bob".into(),
                host: None,
                name: Some("Bob".into()),
                avatar_url: None,
                is_bot: false,
                is_cat: false,
                followers_count: 0,
                following_count: 0,
                notes_count: 0,
                emojis: HashMap::new(),
                bio: None,
                banner_url: None,
                instance: None,
            };
            renoted
        }));
        upsert_note(&conn, &n).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM user WHERE id IN ('u1','u2')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
}
