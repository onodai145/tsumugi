//! 設定データ（Account / Column）の永続化。token は含めない（keyring 管轄）。

use crate::domain::{Account, Column, ColumnGroup, MuteConfig};
use crate::error::Result;
use rusqlite::{params, Connection};
use std::sync::Mutex;

const MUTE_KEY: &str = "mute";

pub struct SettingsStore {
    // note_cache.rs（同 crate の別モジュール）からも使うため pub(crate)
    pub(crate) conn: Mutex<Connection>,
}

impl SettingsStore {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    // ---- Account ----

    pub fn load_accounts(&self) -> Result<Vec<Account>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, host, username, user_id, display_name, avatar_url FROM account ORDER BY rowid",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Account {
                id: r.get(0)?,
                host: r.get(1)?,
                username: r.get(2)?,
                user_id: r.get(3)?,
                display_name: r.get(4)?,
                avatar_url: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn upsert_account(&self, a: &Account) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO account (id, host, username, user_id, display_name, avatar_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               host=excluded.host, username=excluded.username, user_id=excluded.user_id,
               display_name=excluded.display_name, avatar_url=excluded.avatar_url",
            params![a.id, a.host, a.username, a.user_id, a.display_name, a.avatar_url],
        )?;
        Ok(())
    }

    /// アカウント削除。紐づくカラムも消す。
    pub fn delete_account(&self, account_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM column_def WHERE account_id = ?1", params![account_id])?;
        conn.execute("DELETE FROM account WHERE id = ?1", params![account_id])?;
        Ok(())
    }

    // ---- ColumnGroup（視覚カラム） ----

    pub fn load_groups(&self) -> Result<Vec<ColumnGroup>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, ord, width FROM column_group ORDER BY ord, rowid")?;
        let rows = stmt.query_map([], |r| {
            Ok(ColumnGroup {
                id: r.get(0)?,
                order: r.get(1)?,
                width: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn upsert_group(&self, g: &ColumnGroup) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO column_group (id, ord, width) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET ord=excluded.ord, width=excluded.width",
            params![g.id, g.order, g.width],
        )?;
        Ok(())
    }

    pub fn set_group_width(&self, group_id: &str, width: i32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE column_group SET width = ?1 WHERE id = ?2",
            params![width, group_id],
        )?;
        Ok(())
    }

    /// グループの並び順を id 順に振り直す。
    pub fn reorder_groups(&self, ordered_ids: &[String]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for (i, id) in ordered_ids.iter().enumerate() {
            tx.execute(
                "UPDATE column_group SET ord = ?1 WHERE id = ?2",
                params![i as i32, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 空になったグループを削除する（タブが 0 のもの）。
    pub fn delete_empty_groups(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM column_group WHERE id NOT IN (SELECT DISTINCT group_id FROM column_def WHERE group_id IS NOT NULL)",
            [],
        )?;
        Ok(())
    }

    // ---- Column（タブ） ----

    pub fn load_columns(&self) -> Result<Vec<Column>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, account_id, kind, ord, filter, notify_sound, notify_desktop, group_id
             FROM column_def ORDER BY ord, rowid",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?, // kind json
                r.get::<_, i32>(3)?,
                r.get::<_, String>(4)?, // filter json
                r.get::<_, i64>(5)? != 0,
                r.get::<_, i64>(6)? != 0,
                r.get::<_, Option<String>>(7)?.unwrap_or_default(),
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, account_id, kind_json, ord, filter_json, notify_sound, notify_desktop, group_id) =
                row?;
            out.push(Column {
                id,
                account_id,
                kind: serde_json::from_str(&kind_json)?,
                order: ord,
                filter: serde_json::from_str(&filter_json)?,
                notify_sound,
                notify_desktop,
                group_id,
            });
        }
        Ok(out)
    }

    pub fn upsert_column(&self, c: &Column) -> Result<()> {
        let kind_json = serde_json::to_string(&c.kind)?;
        let filter_json = serde_json::to_string(&c.filter)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO column_def (id, account_id, kind, ord, width, filter, notify_sound, notify_desktop, group_id)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               account_id=excluded.account_id, kind=excluded.kind, ord=excluded.ord,
               filter=excluded.filter, notify_sound=excluded.notify_sound,
               notify_desktop=excluded.notify_desktop, group_id=excluded.group_id",
            params![
                c.id,
                c.account_id,
                kind_json,
                c.order,
                filter_json,
                c.notify_sound as i64,
                c.notify_desktop as i64,
                c.group_id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_column(&self, column_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM column_def WHERE id = ?1", params![column_id])?;
        Ok(())
    }

    // ---- NG（ミュート）設定 ----

    pub fn load_mute(&self) -> Result<MuteConfig> {
        let conn = self.conn.lock().unwrap();
        let json: Option<String> = conn
            .query_row(
                "SELECT value FROM app_setting WHERE key = ?1",
                params![MUTE_KEY],
                |r| r.get(0),
            )
            .ok();
        match json {
            Some(s) => Ok(serde_json::from_str(&s)?),
            None => Ok(MuteConfig::default()),
        }
    }

    pub fn save_mute(&self, cfg: &MuteConfig) -> Result<()> {
        let json = serde_json::to_string(cfg)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO app_setting (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![MUTE_KEY, json],
        )?;
        Ok(())
    }

    /// タブを別グループへ移動し、そのグループ内での順序を id 順に振り直す。
    pub fn move_tab(&self, tab_id: &str, group_id: &str, ordered_tab_ids: &[String]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE column_def SET group_id = ?1 WHERE id = ?2",
            params![group_id, tab_id],
        )?;
        for (i, id) in ordered_tab_ids.iter().enumerate() {
            tx.execute(
                "UPDATE column_def SET ord = ?1 WHERE id = ?2",
                params![i as i32, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ColumnKind, FilterQuery};
    use crate::store::db::open_in_memory;

    fn store() -> SettingsStore {
        SettingsStore::new(open_in_memory().unwrap())
    }

    fn account(id: &str) -> Account {
        Account {
            id: id.into(),
            host: "misskey.io".into(),
            username: "me".into(),
            user_id: "u1".into(),
            display_name: "Me".into(),
            avatar_url: Some("http://x/a.png".into()),
        }
    }

    fn column(id: &str, account_id: &str, ord: i32) -> Column {
        Column {
            id: id.into(),
            account_id: account_id.into(),
            kind: ColumnKind::Home,
            order: ord,
            filter: FilterQuery::Keywords(vec!["rust".into()]),
            notify_sound: false,
            notify_desktop: true,
            group_id: "g1".into(),
        }
    }

    #[test]
    fn account_roundtrip_and_upsert() {
        let s = store();
        assert!(s.load_accounts().unwrap().is_empty());
        s.upsert_account(&account("a1")).unwrap();
        let mut a = account("a1");
        a.display_name = "Renamed".into();
        s.upsert_account(&a).unwrap(); // 上書き（増えない）
        let loaded = s.load_accounts().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].display_name, "Renamed");
    }

    #[test]
    fn column_roundtrip_preserves_kind_and_filter() {
        let s = store();
        s.upsert_account(&account("a1")).unwrap();
        s.upsert_column(&column("c1", "a1", 0)).unwrap();
        s.upsert_column(&column("c2", "a1", 1)).unwrap();
        let cols = s.load_columns().unwrap();
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].id, "c1");
        assert!(matches!(cols[0].kind, ColumnKind::Home));
        assert_eq!(cols[0].filter, FilterQuery::Keywords(vec!["rust".into()]));
        assert!(cols[0].notify_desktop);
    }

    #[test]
    fn delete_account_cascades_columns() {
        let s = store();
        s.upsert_account(&account("a1")).unwrap();
        s.upsert_column(&column("c1", "a1", 0)).unwrap();
        s.delete_account("a1").unwrap();
        assert!(s.load_accounts().unwrap().is_empty());
        assert!(s.load_columns().unwrap().is_empty());
    }

    #[test]
    fn delete_column_removes_only_target() {
        let s = store();
        s.upsert_account(&account("a1")).unwrap();
        s.upsert_column(&column("c1", "a1", 0)).unwrap();
        s.upsert_column(&column("c2", "a1", 1)).unwrap();
        s.delete_column("c1").unwrap();
        let cols = s.load_columns().unwrap();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].id, "c2");
    }

    #[test]
    fn groups_and_move_tab() {
        let s = store();
        s.upsert_account(&account("a1")).unwrap();
        s.upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 300 }).unwrap();
        s.upsert_group(&ColumnGroup { id: "g2".into(), order: 1, width: 300 }).unwrap();
        s.upsert_column(&column("c1", "a1", 0)).unwrap(); // g1
        // タブを g2 へ移動
        s.move_tab("c1", "g2", &["c1".into()]).unwrap();
        let cols = s.load_columns().unwrap();
        assert_eq!(cols[0].group_id, "g2");
        // g1 は空 → 掃除
        s.delete_empty_groups().unwrap();
        let groups = s.load_groups().unwrap();
        assert_eq!(groups.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["g2"]);
    }

    #[test]
    fn set_group_width_and_reorder() {
        let s = store();
        s.upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 300 }).unwrap();
        s.upsert_group(&ColumnGroup { id: "g2".into(), order: 1, width: 300 }).unwrap();
        s.set_group_width("g1", 420).unwrap();
        s.reorder_groups(&["g2".into(), "g1".into()]).unwrap();
        let groups = s.load_groups().unwrap();
        assert_eq!(groups[0].id, "g2");
        assert_eq!(groups[1].id, "g1");
        assert_eq!(groups[1].width, 420);
    }
}
