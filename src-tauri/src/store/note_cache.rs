//! ノートキャッシュの読み書き（NQL§9 の正規化テーブル + 表示復元用 payload）。
//! `SettingsStore`（= ローカル DB アクセス層）に対する inherent impl として実装する。

use super::settings::SettingsStore;
use crate::domain::{Note, Visibility};
use crate::error::Result;
use rusqlite::{params, Connection};

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

impl SettingsStore {
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
                "INSERT OR IGNORE INTO column_note (column_id, note_id, received_at) VALUES (?1, ?2, ?3)",
                params![column_id, n.id, now],
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
            "SELECT n.payload FROM note n
             JOIN column_note cn ON cn.note_id = n.id
             WHERE cn.column_id = ?1
             ORDER BY n.created_at DESC, n.id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![column_id, limit], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for payload in rows {
            out.push(serde_json::from_str::<Note>(&payload?)?);
        }
        Ok(out)
    }

    /// カラム所属レコードを消す（カラム削除時。note 本体は他カラムと共有しうるので残す）。
    pub fn clear_column_notes(&self, column_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM column_note WHERE column_id = ?1", params![column_id])?;
        Ok(())
    }
}

/// note + user + 関連テーブルを upsert する。関連は入れ替え（DELETE→INSERT）。
fn upsert_note(conn: &Connection, n: &Note) -> Result<()> {
    let payload = serde_json::to_string(n)?;
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

    let u = &n.user;
    conn.execute(
        "INSERT OR REPLACE INTO user (
            id, username, host, name, is_bot, is_cat,
            followers_count, following_count, notes_count
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            u.id, u.username, u.host, u.name, u.is_bot as i64, u.is_cat as i64,
            u.followers_count, u.following_count, u.notes_count
        ],
    )?;

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
    for e in &n.emojis {
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
    use crate::store::db::open_in_memory;
    use std::collections::HashMap;

    fn store() -> SettingsStore {
        SettingsStore::new(open_in_memory().unwrap())
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
            }],
            poll: None,
            tags: vec!["rust".into()],
            mentions: vec![],
            emojis: vec![],
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
