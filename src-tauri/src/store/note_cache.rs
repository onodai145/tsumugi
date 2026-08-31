//! ノートキャッシュの読み書き（TQL§9 の正規化テーブル + 表示復元用 payload）。
//! 設定(`SettingsStore`)とは別ファイル・別接続で持つ（バックアップ対象を小さな設定DBに絞るため）。

use crate::domain::{Note, Visibility};
use crate::error::Result;
use crate::store::user_ref::{
    collect_user_id_refs, collect_users, fetch_users_by_ids, has_legacy_full_user,
    hydrate_user_refs, is_legacy_full_user, stub_user_refs, upsert_user,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::Mutex;

/// ノートキャッシュ専用のSQLite接続。`SettingsStore`とは別ファイル(cache.db)を持つ。
/// 破棄しても再取得で復元できるため、設定ほど重要ではない。
pub struct NoteCacheStore {
    conn: Mutex<Connection>,
}

impl NoteCacheStore {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }
}

fn now_epoch() -> i64 {
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

impl NoteCacheStore {
    /// ノート群をキャッシュへ upsert し、カラム所属を記録する（1トランザクション）。
    pub fn cache_notes(&self, column_id: &str, notes: &[Note]) -> Result<()> {
        if notes.is_empty() {
            return Ok(());
        }
        let mut guard = self.conn.lock().unwrap();
        let tx = guard.transaction()?;
        let now = now_epoch();
        for n in notes {
            upsert_note(&tx, n)?;
            tx.execute(
                "INSERT OR IGNORE INTO column_note (column_id, note_id, received_at, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![column_id, n.id, now, n.created_at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 1件のノートをキャッシュ（Streaming 受信時に使う）。
    pub fn cache_note(&self, column_id: &str, note: &Note) -> Result<()> {
        self.cache_notes(column_id, std::slice::from_ref(note))
    }

    /// カラムの直近ノートをキャッシュから取得（新しい順・最大 limit）。
    pub fn load_cached(&self, column_id: &str, limit: u32) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT n.id, n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = ?1
             ORDER BY cn.created_at DESC, cn.note_id DESC
             LIMIT ?2",
        )?;
        let rows: Vec<(String, String)> = stmt
            .query_map(params![column_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        resolve_payload_rows(&conn, rows)
    }

    /// カラムのキャッシュから until_id より古いノートを取得（新しい順、最大 limit 件）。
    /// backfill のキャッシュ優先パス用（load_cached の until_id 版）。
    pub fn load_cached_before(&self, column_id: &str, until_id: &str, limit: u32) -> Result<Vec<Note>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT n.id, n.payload FROM column_note cn
             JOIN note n ON n.id = cn.note_id
             WHERE cn.column_id = ?1 AND cn.note_id < ?2
             ORDER BY cn.note_id DESC
             LIMIT ?3",
        )?;
        let rows: Vec<(String, String)> = stmt
            .query_map(params![column_id, until_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut out = resolve_payload_rows(&conn, rows)?;
        // 絞り込み・LIMITの選抜は境界比較と同じ id 基準で行い、表示順への並べ替えだけを
        // ここで行う。created_at で選抜すると、連合ノート(idとcreated_atの順序が食い違う)が
        // 範囲内にあるのに LIMIT の外へ弾かれて静かに欠落する(Issue #228)。
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        Ok(out)
    }

    /// note_id 単体をキャッシュから取得する（column_note を経由しない）。
    /// 自分のリアクション操作やstreamingのnoteUpdatedをキャッシュへ反映する際、
    /// 対象ノートがどのカラムに属すか気にせず読み書きするために使う。
    pub fn get_note(&self, note_id: &str) -> Result<Option<Note>> {
        let conn = self.conn.lock().unwrap();
        let row: Option<(String, String)> = conn
            .query_row("SELECT id, payload FROM note WHERE id = ?1", params![note_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        Ok(match row {
            Some((id, payload)) => resolve_payload_rows(&conn, vec![(id, payload)])?.into_iter().next(),
            None => None,
        })
    }

    /// 1件のノートのキャッシュ内容を更新する（column_note には触れない）。
    /// 自分のリアクション操作やstreamingのnoteUpdatedをキャッシュへ反映するために使う。
    /// 対象がまだキャッシュに無ければ何もしない想定の呼び出し元(get_noteでSomeを確認済み)向け。
    pub fn update_note(&self, note: &Note) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        upsert_note(&conn, note)
    }

    /// カラム所属レコードを消す（カラム削除時。note 本体は他カラムと共有しうるので残す）。
    pub fn clear_column_notes(&self, column_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM column_note WHERE column_id = ?1", params![column_id])?;
        conn.execute("DELETE FROM column_fetch_boundary WHERE column_id = ?1", params![column_id])?;
        Ok(())
    }

    /// カラムの境界(oldest_fetched_id)を取得。未確定ならNone。
    pub fn get_fetch_boundary(&self, column_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let v: Option<String> = conn
            .query_row(
                "SELECT oldest_fetched_id FROM column_fetch_boundary WHERE column_id = ?1",
                params![column_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v)
    }

    /// 境界を new_oldest_id で無条件に新規セット/上書きする(初回REST取得時に使う)。
    pub fn set_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES (?1, ?2)
             ON CONFLICT(column_id) DO UPDATE SET oldest_fetched_id = excluded.oldest_fetched_id",
            params![column_id, new_oldest_id],
        )?;
        Ok(())
    }

    /// 境界を new_oldest_id まで延長する(古い方向へのみ、単調性を保証)。
    /// 既存値の方が既に古ければ何もしない。
    pub fn extend_fetch_boundary(&self, column_id: &str, new_oldest_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO column_fetch_boundary (column_id, oldest_fetched_id) VALUES (?1, ?2)
             ON CONFLICT(column_id) DO UPDATE SET
                oldest_fetched_id = MIN(oldest_fetched_id, excluded.oldest_fetched_id)",
            params![column_id, new_oldest_id],
        )?;
        Ok(())
    }

    /// 全カラムのbackfill境界を削除する(未確定状態に戻す)。ミュート設定変更時など、
    /// キャッシュされたフィルタ済みノート集合の前提が崩れる操作の後に呼ぶ(Issue #228)。
    pub fn clear_all_fetch_boundaries(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM column_fetch_boundary", [])?;
        Ok(())
    }

    /// キャッシュ済みノートの総数。Backstageのステータス表示用。
    /// specta が i64 の直接エクスポートを禁止するため i32 で返す(ローカルキャッシュ件数が
    /// 21億件を超えることは実運用上ない)。
    pub fn note_count(&self) -> Result<i32> {
        let conn = self.conn.lock().unwrap();
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM note", [], |r| r.get(0))?;
        Ok(count)
    }

    /// キャッシュを間引く（Issue #6: 無制限に溜まり続けないようにする）。3つの上限を順に適用する:
    /// 1. `max_age_days` 日より古いノートを削除
    /// 2. 件数が `keep` を超えていれば古い順に削除
    /// 3. DBサイズが `max_size_mb` を超えていれば古い順に削除（incremental_vacuumで実サイズへ反映）
    ///
    /// 各上限は `<= 0` で無効（無制限）。戻り値は実際に削除した件数の合計。
    pub fn prune(&self, keep: i32, max_age_days: i32, max_size_mb: i32) -> Result<usize> {
        let mut guard = self.conn.lock().unwrap();
        let mut deleted: i64 = 0;
        {
            let tx = guard.transaction()?;
            if max_age_days > 0 {
                let cutoff = now_epoch() - max_age_days as i64 * 86_400;
                deleted += delete_matching(
                    &tx,
                    "SELECT id FROM note WHERE created_at < ?1",
                    params![cutoff],
                )?;
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
            deleted += shrink_to_size(&guard, max_size_mb as i64 * 1024 * 1024)?;
        }
        Ok(deleted as usize)
    }

    /// 投稿日時(created_at, epoch秒)が since_epoch_secs 以降のノート件数。
    /// 流速表示用: DBへのINSERT件数ではなく実際の投稿時刻で数えるため、起動時ギャップ埋めや
    /// 上スクロールでの過去取得(古いcreated_atのノートをまとめてupsertする)による誤った
    /// 跳ね上がりが起きない。idx_note_created を使う。
    pub fn notes_since(&self, since_epoch_secs: i32) -> Result<i32> {
        let conn = self.conn.lock().unwrap();
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM note WHERE created_at >= ?1",
            params![since_epoch_secs],
            |r| r.get(0),
        )?;
        Ok(count)
    }

    /// TQL `cache` ソース: ローカルSQLiteキャッシュ全体を where 句で検索する（受信せず検索のみ）。
    /// until_id は作成順の境界（id 自体は sortable なので created_at の代わりに使える）。
    pub fn search_cache(
        &self,
        where_sql: &crate::filter::sql::SqlWhere,
        until_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Note>> {
        use crate::filter::sql::SqlParam;

        let mut sql = String::from(
            "SELECT n.id, n.payload FROM note n JOIN user u ON u.id = n.user_id WHERE (",
        );
        sql.push_str(&where_sql.sql);
        sql.push(')');
        if until_id.is_some() {
            sql.push_str(" AND n.id < ?");
        }
        sql.push_str(" ORDER BY n.created_at DESC, n.id DESC LIMIT ?");

        let conn = self.conn.lock().unwrap();
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
        resolve_payload_rows(&conn, rows)
    }
}

/// (note_id, payload) の行群を Note へ復元する(Issue #263)。
/// - 旧形式(userフルオブジェクト埋め込み)を検知した行は、その場で user テーブルへ抽出し
///   payload をスタブ形式へ書き戻してから復元する(自己修復)。
/// - user参照(本体+renote分)が user テーブルに見つからない行は、ログ警告してスキップする
///   (呼び出し元をエラーにしない。deserialize_note_or_warn と同じポリシー)。
fn resolve_payload_rows(conn: &Connection, rows: Vec<(String, String)>) -> Result<Vec<Note>> {
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
                upsert_user(conn, &user)?;
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

/// note + user + 関連テーブルを upsert する。関連は入れ替え（DELETE→INSERT）。
fn upsert_note(conn: &Connection, n: &Note) -> Result<()> {
    let mut payload_value = serde_json::to_value(n)?;
    stub_user_refs(&mut payload_value);
    let payload = serde_json::to_string(&payload_value)?;
    let text_length = n.text.as_deref().map(|t| t.chars().count()).unwrap_or(0) as i64;
    let has_link = n.text.as_deref().map(has_url).unwrap_or(false) as i64;

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

    // 本体+renote分すべての User を正規化テーブルへ反映する(Issue #263)。
    for user in collect_users(n) {
        upsert_user(conn, user)?;
    }

    // 関連テーブルは入れ替え
    for table in ["note_reaction", "note_tag", "note_mention", "note_emoji", "note_file"] {
        conn.execute(&format!("DELETE FROM {table} WHERE note_id = ?1"), params![n.id])?;
    }
    for (emoji, count) in &n.reactions {
        conn.execute(
            "INSERT INTO note_reaction (note_id, emoji_key, count) VALUES (?1, ?2, ?3)",
            params![n.id, emoji, count],
        )?;
    }
    for tag in &n.tags {
        conn.execute("INSERT INTO note_tag (note_id, tag) VALUES (?1, ?2)", params![n.id, tag])?;
    }
    for uid in &n.mentions {
        conn.execute("INSERT INTO note_mention (note_id, user_id) VALUES (?1, ?2)", params![n.id, uid])?;
    }
    for e in n.emojis.keys() {
        conn.execute("INSERT INTO note_emoji (note_id, emoji) VALUES (?1, ?2)", params![n.id, e])?;
    }
    for f in &n.files {
        conn.execute(
            "INSERT INTO note_file (note_id, mime_type, mime_category, is_sensitive) VALUES (?1, ?2, ?3, ?4)",
            params![n.id, f.mime_type, mime_category(&f.mime_type), f.is_sensitive as i64],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DriveFile, User};
    use crate::store::db::open_cache_in_memory;
    use std::collections::HashMap;

    fn store() -> NoteCacheStore {
        NoteCacheStore::new(open_cache_in_memory().unwrap())
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

    #[test]
    fn upsert_note_stores_stubbed_user_in_payload() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100)]).unwrap();

        let raw_payload: String = {
            let conn = s.conn.lock().unwrap();
            conn.query_row("SELECT payload FROM note WHERE id = 'n1'", [], |r| r.get(0)).unwrap()
        };
        let v: serde_json::Value = serde_json::from_str(&raw_payload).unwrap();
        assert_eq!(v["user"], serde_json::json!({ "id": "u1" }));
    }

    #[test]
    fn upsert_note_upserts_both_note_and_renote_authors_into_user_table() {
        let s = store();
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
        s.cache_notes("col1", &[n]).unwrap();

        let conn = s.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM user WHERE id IN ('u1','u2')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn cache_roundtrip_preserves_note_and_order() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 150)]).unwrap();
        let got = s.load_cached("col1", 10).unwrap();
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
    #[test]
    fn load_cached_skips_row_with_legacy_array_emojis_payload() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200)]).unwrap();
        {
            let conn = s.conn.lock().unwrap();
            let legacy_payload = payload_with_array_emojis(&note("n1", 100));
            conn.execute(
                "UPDATE note SET payload = ?1 WHERE id = 'n1'",
                params![legacy_payload],
            )
            .unwrap();
        }

        let got = s.load_cached("col1", 10).unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2"]);
    }

    /// load_cached と同様、search_cache も壊れた行1件で全体を空にしない。
    #[test]
    fn search_cache_skips_row_with_legacy_array_emojis_payload() {
        use crate::filter::{parser, sql};
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200)]).unwrap();
        {
            let conn = s.conn.lock().unwrap();
            let legacy_payload = payload_with_array_emojis(&note("n1", 100));
            conn.execute(
                "UPDATE note SET payload = ?1 WHERE id = 'n1'",
                params![legacy_payload],
            )
            .unwrap();
        }

        let ctx = sql::SqlCtx { my_ids: vec![], following_ids: None };
        let expr = parser::parse_predicate("has_files").unwrap();
        let w = sql::build_where(&expr, &ctx).unwrap();
        let got = s.search_cache(&w, None, 10).unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2"]);
    }

    #[test]
    fn get_note_returns_none_when_not_cached() {
        let s = store();
        assert!(s.get_note("missing").unwrap().is_none());
    }

    /// 旧形式(配列)の emojis payload は「読めないので未キャッシュ扱い」とし、
    /// Err で呼び出し元(react/unreact/noteUpdated反映)を永続的に沈黙させない(Issue #150)。
    #[test]
    fn get_note_returns_none_for_row_with_legacy_array_emojis_payload() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100)]).unwrap();
        {
            let conn = s.conn.lock().unwrap();
            let legacy_payload = payload_with_array_emojis(&note("n1", 100));
            conn.execute(
                "UPDATE note SET payload = ?1 WHERE id = 'n1'",
                params![legacy_payload],
            )
            .unwrap();
        }

        assert!(s.get_note("n1").unwrap().is_none());
    }

    #[test]
    fn update_note_persists_without_column_note_and_get_note_reflects_it() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100)]).unwrap();

        let mut n = s.get_note("n1").unwrap().unwrap();
        n.reactions.insert("😀".into(), 1);
        n.reaction_count += 1;
        n.my_reaction = Some("😀".into());
        s.update_note(&n).unwrap();

        // update_note は column_note に触れないので、既存の所属は変わらない
        let got = s.load_cached("col1", 10).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].reactions.get("😀"), Some(&1));
        assert_eq!(got[0].reaction_count, 4); // 元の3 + 1
        assert_eq!(got[0].my_reaction.as_deref(), Some("😀"));

        // get_note 単体でも同じ内容が読める
        let single = s.get_note("n1").unwrap().unwrap();
        assert_eq!(single.reactions.get("😀"), Some(&1));
    }

    #[test]
    fn search_cache_applies_predicate_and_until_id_boundary() {
        use crate::filter::{parser, sql};
        let s = store();
        s.cache_notes("col1", &[note("a1", 300), note("a2", 200), note("a3", 100)]).unwrap();

        let ctx = sql::SqlCtx { my_ids: vec![], following_ids: None };

        // 述語(has_files)は全件trueなので until_id 境界のみで絞られる
        let expr = parser::parse_predicate("has_files").unwrap();
        let w = sql::build_where(&expr, &ctx).unwrap();
        let got = s.search_cache(&w, Some("a3"), 10).unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["a1", "a2"]);

        // 述語が全件falseなら空
        let expr2 = parser::parse_predicate("cw").unwrap();
        let w2 = sql::build_where(&expr2, &ctx).unwrap();
        assert!(s.search_cache(&w2, None, 10).unwrap().is_empty());
    }

    #[test]
    fn upsert_replaces_and_relations_not_duplicated() {
        let s = store();
        s.cache_note("col1", &note("n1", 100)).unwrap();
        s.cache_note("col1", &note("n1", 100)).unwrap(); // 再受信
        let got = s.load_cached("col1", 10).unwrap();
        assert_eq!(got.len(), 1); // 重複しない
        // 関連テーブルも重複していない
        let conn = s.conn.lock().unwrap();
        let rc: i64 = conn
            .query_row("SELECT COUNT(*) FROM note_reaction WHERE note_id='n1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rc, 1);
    }

    #[test]
    fn column_isolation_and_clear() {
        let s = store();
        s.cache_note("colA", &note("n1", 100)).unwrap();
        s.cache_note("colB", &note("n2", 100)).unwrap();
        assert_eq!(s.load_cached("colA", 10).unwrap().len(), 1);
        assert_eq!(s.load_cached("colB", 10).unwrap().len(), 1);
        s.clear_column_notes("colA").unwrap();
        assert_eq!(s.load_cached("colA", 10).unwrap().len(), 0);
        assert_eq!(s.load_cached("colB", 10).unwrap().len(), 1); // 他カラムは残る
    }

    #[test]
    fn fetch_boundary_roundtrip() {
        let s = store();
        assert!(s.get_fetch_boundary("col1").unwrap().is_none());

        s.set_fetch_boundary("col1", "n100").unwrap();
        assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n100"));
    }

    #[test]
    fn set_fetch_boundary_overwrites_unconditionally() {
        let s = store();
        s.set_fetch_boundary("col1", "n100").unwrap();
        s.set_fetch_boundary("col1", "n999").unwrap(); // より新しい値でも無条件に上書き
        assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n999"));
    }

    #[test]
    fn extend_fetch_boundary_only_moves_older() {
        let s = store();
        s.set_fetch_boundary("col1", "n500").unwrap();

        // より古い値(n300)への延長は反映される
        s.extend_fetch_boundary("col1", "n300").unwrap();
        assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n300"));

        // より新しい値(n800)は無視される(単調性)
        s.extend_fetch_boundary("col1", "n800").unwrap();
        assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n300"));
    }

    #[test]
    fn extend_fetch_boundary_sets_when_absent() {
        let s = store();
        assert!(s.get_fetch_boundary("col1").unwrap().is_none());
        s.extend_fetch_boundary("col1", "n300").unwrap();
        assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n300"));
    }

    #[test]
    fn clear_column_notes_also_removes_boundary() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100)]).unwrap();
        s.set_fetch_boundary("col1", "n1").unwrap();

        s.clear_column_notes("col1").unwrap();

        assert!(s.get_fetch_boundary("col1").unwrap().is_none());
    }

    #[test]
    fn load_cached_before_returns_notes_older_than_until_id_desc() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).unwrap();

        let got = s.load_cached_before("col1", "n3", 10).unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2", "n1"]);
    }

    #[test]
    fn load_cached_before_respects_limit_and_column_scope() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).unwrap();
        s.cache_notes("col2", &[note("m1", 250)]).unwrap();

        let got = s.load_cached_before("col1", "n3", 1).unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n2"]);

        // col2 のノートは混ざらない
        let got_all = s.load_cached_before("col1", "n3", 10).unwrap();
        assert!(got_all.iter().all(|n| n.id != "m1"));
    }

    /// 連合ノートは id(受信順) と created_at(発信元での投稿時刻) の順序が食い違いうる。
    /// LIMIT の選抜は必ず id 基準で行い、created_at が古いという理由だけで
    /// 範囲内のノートが脱落しないこと(Issue #228)。
    #[test]
    fn load_cached_before_selects_by_id_not_created_at() {
        let s = store();
        // id順: n1 < n2 < n3、created_at順: n2(100) < n3(800) < n1(900)
        s.cache_notes("col1", &[note("n1", 900), note("n2", 100), note("n3", 800)]).unwrap();

        let got = s.load_cached_before("col1", "n9", 2).unwrap();
        // id の大きい方から2件(n3, n2)が選抜される。created_at で選ぶと n1, n3 になり n2 が欠落する。
        let ids: Vec<&str> = got.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["n3", "n2"], "id基準で選抜し created_at DESC で並べること");
    }

    #[test]
    fn prune_removes_oldest_beyond_keep_and_related_rows() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).unwrap();
        let deleted = s.prune(2, 0, 0).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(s.note_count().unwrap(), 2);
        // 最古(n1)が消え、残りは新しい2件
        let got = s.load_cached("col1", 10).unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n3", "n2"]);
        // 関連テーブル・column_note も一緒に消えていること
        let conn = s.conn.lock().unwrap();
        let rc: i64 = conn
            .query_row("SELECT COUNT(*) FROM note_reaction WHERE note_id='n1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rc, 0);
        let cn: i64 = conn
            .query_row("SELECT COUNT(*) FROM column_note WHERE note_id='n1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cn, 0);
    }

    #[test]
    fn prune_is_noop_when_under_or_unlimited() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200)]).unwrap();
        assert_eq!(s.prune(10, 0, 0).unwrap(), 0); // 上限未満
        assert_eq!(s.prune(0, 0, 0).unwrap(), 0); // 全て無制限
        assert_eq!(s.note_count().unwrap(), 2);
    }

    #[test]
    fn prune_removes_notes_older_than_max_age_days() {
        let s = store();
        let now = now_epoch();
        let one_day = 86_400;
        s.cache_notes(
            "col1",
            &[
                note("old", now - 40 * one_day),
                note("recent", now - 1 * one_day),
            ],
        )
        .unwrap();
        let deleted = s.prune(0, 30, 0).unwrap();
        assert_eq!(deleted, 1);
        let got = s.load_cached("col1", 10).unwrap();
        assert_eq!(got.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["recent"]);
    }

    #[test]
    fn prune_shrinks_db_below_max_size_mb() {
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
        s.cache_notes("col1", &notes).unwrap();
        let before_count = s.note_count().unwrap();
        let before_size = {
            let conn = s.conn.lock().unwrap();
            db_size_bytes(&conn).unwrap()
        };
        assert!(before_size > 1024 * 1024, "test setup should exceed 1MB, got {before_size}");

        let deleted = s.prune(0, 0, 1).unwrap();
        assert!(deleted > 0);
        assert!(s.note_count().unwrap() < before_count);
        let after_size = {
            let conn = s.conn.lock().unwrap();
            db_size_bytes(&conn).unwrap()
        };
        assert!(after_size < before_size);
    }

    #[test]
    fn prune_raises_boundary_to_surviving_oldest_note_after_keep_exceeded() {
        let s = store();
        s.cache_notes("col1", &[note("n1", 100), note("n2", 200), note("n3", 300)]).unwrap();
        s.set_fetch_boundary("col1", "n1").unwrap(); // n1まで(=全件)取得済みと主張

        let deleted = s.prune(2, 0, 0).unwrap(); // 最古のn1が削除される
        assert_eq!(deleted, 1);

        // n1が消えたので、生存最古のn2まで境界を引き上げる
        assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n2"));
    }

    /// created_at は古いが id は生存最古より新しいノート(連合ノート)が prune で消えた場合、
    /// 生存最古ID だけでは境界を引き上げられない。削除されたノートの最大IDまで引き上げること(Issue #228)。
    #[test]
    fn prune_raises_boundary_past_deleted_note_with_newer_id() {
        let s = store();
        // id順: n1 < n2 < n5、created_at順: n5(100) < n1(200) < n2(300)
        s.cache_notes("col1", &[note("n5", 100), note("n1", 200), note("n2", 300)]).unwrap();
        s.set_fetch_boundary("col1", "n1").unwrap();

        let deleted = s.prune(2, 0, 0).unwrap(); // created_at 最古の n5 が削除される
        assert_eq!(deleted, 1);

        // 生存最古IDは n1 のままなので、削除された n5 まで境界を引き上げる必要がある
        assert_eq!(s.get_fetch_boundary("col1").unwrap().as_deref(), Some("n5"));
    }

    #[test]
    fn clear_all_fetch_boundaries_removes_every_column() {
        let s = store();
        s.set_fetch_boundary("col1", "n100").unwrap();
        s.set_fetch_boundary("col2", "n200").unwrap();

        s.clear_all_fetch_boundaries().unwrap();

        assert!(s.get_fetch_boundary("col1").unwrap().is_none());
        assert!(s.get_fetch_boundary("col2").unwrap().is_none());
    }

    #[test]
    fn prune_clears_boundary_when_column_fully_pruned() {
        let s = store();
        let now = now_epoch();
        let one_day = 86_400;
        s.cache_notes("col1", &[note("old", now - 40 * one_day)]).unwrap();
        s.set_fetch_boundary("col1", "old").unwrap();

        let deleted = s.prune(0, 30, 0).unwrap();
        assert_eq!(deleted, 1);

        // カラムのキャッシュが全滅したので境界は未確定に戻る
        assert!(s.get_fetch_boundary("col1").unwrap().is_none());
    }

    #[test]
    fn prune_leaves_unaffected_columns_boundary_untouched() {
        let s = store();
        s.cache_notes("colA", &[note("a1", 50)]).unwrap();
        s.cache_notes("colB", &[note("b1", 100), note("b2", 200), note("b3", 300)]).unwrap();
        s.set_fetch_boundary("colA", "a1").unwrap();
        s.set_fetch_boundary("colB", "b1").unwrap();

        let deleted = s.prune(3, 0, 0).unwrap(); // 4件中keep=3 → 全体最古のa1のみ削除
        assert_eq!(deleted, 1);

        assert!(s.get_fetch_boundary("colA").unwrap().is_none());
        assert_eq!(s.get_fetch_boundary("colB").unwrap().as_deref(), Some("b1")); // 変わらない
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

    #[test]
    fn load_cached_self_heals_legacy_full_user_payload() {
        let s = store();
        {
            let conn = s.conn.lock().unwrap();
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

        let got = s.load_cached("col1", 10).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].user.id, "u_legacy");
        let instance = got[0].user.instance.as_ref().expect("instance should be hydrated");
        assert_eq!(instance.name.as_deref(), Some("Remote"));

        // payload がスタブ形式へ書き戻されていること
        let conn = s.conn.lock().unwrap();
        let raw: String = conn.query_row("SELECT payload FROM note WHERE id = 'n_legacy'", [], |r| r.get(0)).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["user"], serde_json::json!({ "id": "u_legacy" }));

        // user テーブルへ抽出されていること
        let name: String = conn.query_row("SELECT name FROM user WHERE id = 'u_legacy'", [], |r| r.get(0)).unwrap();
        assert_eq!(name, "Carol");
    }

    #[test]
    fn load_cached_skips_note_when_referenced_user_row_missing() {
        let s = store();
        {
            let conn = s.conn.lock().unwrap();
            insert_legacy_row(&conn, "n_orphan", 100, serde_json::json!({ "id": "u_orphan" }));
        }

        let got = s.load_cached("col1", 10).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn normalized_columns_populated_for_nql() {
        let s = store();
        s.cache_note("col1", &note("n1", 100)).unwrap();
        let conn = s.conn.lock().unwrap();
        // has_link / text_length / files_count 等が正規化カラムに入る
        let (has_link, files_count): (i64, i64) = conn
            .query_row("SELECT has_link, files_count FROM note WHERE id='n1'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(has_link, 1);
        assert_eq!(files_count, 1);
        let cat: String = conn
            .query_row("SELECT mime_category FROM note_file WHERE note_id='n1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cat, "image");
    }
}
