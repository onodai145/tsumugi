//! 設定データ（Account / Column）の永続化。token は含めない（keyring 管轄）。
//! プレーンテキスト(JSON)で1ファイルに保存する。変更のたびに全体を書き出す
//! （書き込み頻度は低いため、SQLiteのようなインクリメンタル更新は不要）。
//! 書き込みは一時ファイル→rename で行い、途中でクラッシュしても壊れないようにする。

use crate::domain::{Account, Column, ColumnGroup, MuteConfig, NotifyConfig, PaneNode, UiPrefs};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SettingsData {
    #[serde(default)]
    accounts: Vec<Account>,
    #[serde(default)]
    groups: Vec<ColumnGroup>,
    #[serde(default)]
    columns: Vec<Column>,
    #[serde(default)]
    mute: MuteConfig,
    #[serde(default)]
    notify: NotifyConfig,
    #[serde(default)]
    ui: UiPrefs,
    #[serde(default)]
    pane_layout: Option<PaneNode>,
}

/// 保存先。テスト用に `Memory`(ディスクI/Oなし)を持つ。
enum Backing {
    File(PathBuf),
    #[cfg(test)]
    Memory,
}

pub struct SettingsStore {
    backing: Backing,
    data: Mutex<SettingsData>,
}

impl SettingsStore {
    /// 指定パスの設定ファイル(JSON)を読み込む。存在しなければ空の設定から始める。
    pub fn new(path: PathBuf) -> Result<Self> {
        let data = load_json_or_default(&path)?;
        Ok(Self {
            backing: Backing::File(path),
            data: Mutex::new(data),
        })
    }

    #[cfg(test)]
    pub(crate) fn new_in_memory() -> Self {
        Self {
            backing: Backing::Memory,
            data: Mutex::new(SettingsData::default()),
        }
    }

    // release ビルドでは Backing::Memory(テスト専用)が存在せず if let が irrefutable になるため許容する。
    #[allow(irrefutable_let_patterns)]
    fn save(&self, data: &SettingsData) -> Result<()> {
        if let Backing::File(path) = &self.backing {
            let json = serde_json::to_string_pretty(data)?;
            let tmp_path = path.with_extension("json.tmp");
            std::fs::write(&tmp_path, json)?;
            std::fs::rename(&tmp_path, path)?;
        }
        Ok(())
    }

    // ---- Account ----

    pub fn load_accounts(&self) -> Result<Vec<Account>> {
        Ok(self.data.lock().unwrap().accounts.clone())
    }

    pub fn upsert_account(&self, a: &Account) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        match guard.accounts.iter_mut().find(|x| x.id == a.id) {
            Some(existing) => *existing = a.clone(),
            None => guard.accounts.push(a.clone()),
        }
        self.save(&guard)
    }

    /// アカウント削除。紐づくカラムも消す。
    pub fn delete_account(&self, account_id: &str) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard.accounts.retain(|a| a.id != account_id);
        guard.columns.retain(|c| c.account_id != account_id);
        self.save(&guard)
    }

    // ---- ColumnGroup（視覚カラム） ----

    pub fn load_groups(&self) -> Result<Vec<ColumnGroup>> {
        let mut list = self.data.lock().unwrap().groups.clone();
        list.sort_by_key(|g| g.order);
        Ok(list)
    }

    pub fn upsert_group(&self, g: &ColumnGroup) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        match guard.groups.iter_mut().find(|x| x.id == g.id) {
            Some(existing) => *existing = g.clone(),
            None => guard.groups.push(g.clone()),
        }
        self.save(&guard)
    }

    pub fn set_group_width(&self, group_id: &str, width: i32) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        if let Some(g) = guard.groups.iter_mut().find(|x| x.id == group_id) {
            g.width = width;
        }
        self.save(&guard)
    }

    pub fn set_group_auto(&self, group_id: &str, auto: bool) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        if let Some(g) = guard.groups.iter_mut().find(|x| x.id == group_id) {
            g.auto = auto;
        }
        self.save(&guard)
    }

    /// グループの並び順を id 順に振り直す。
    pub fn reorder_groups(&self, ordered_ids: &[String]) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        for (i, id) in ordered_ids.iter().enumerate() {
            if let Some(g) = guard.groups.iter_mut().find(|x| &x.id == id) {
                g.order = i as i32;
            }
        }
        self.save(&guard)
    }

    /// 空になったグループを削除する（タブが 0 のもの）。木構造(pane_layout)からも
    /// 該当Leafを取り除く(唯一のルートだった場合はツリーをリセットする)。
    pub fn delete_empty_groups(&self) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        let used: std::collections::HashSet<String> =
            guard.columns.iter().map(|c| c.group_id.clone()).collect();
        let removed_ids: Vec<String> =
            guard.groups.iter().filter(|g| !used.contains(&g.id)).map(|g| g.id.clone()).collect();
        guard.groups.retain(|g| used.contains(&g.id));
        for gid in &removed_ids {
            if let Some(root) = &mut guard.pane_layout {
                let is_root_leaf_target =
                    matches!(root, PaneNode::Leaf { group_id, .. } if group_id == gid);
                if is_root_leaf_target {
                    guard.pane_layout = None;
                } else {
                    root.remove_group(gid);
                }
            }
        }
        self.save(&guard)
    }

    // ---- Column（タブ） ----

    pub fn load_columns(&self) -> Result<Vec<Column>> {
        let mut list = self.data.lock().unwrap().columns.clone();
        list.sort_by_key(|c| c.order);
        Ok(list)
    }

    /// タブのカスタム名を設定/解除（None で自動生成名に戻す）。
    pub fn set_column_title(&self, column_id: &str, title: Option<&str>) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        if let Some(c) = guard.columns.iter_mut().find(|x| x.id == column_id) {
            c.title = title.map(|s| s.to_string());
        }
        self.save(&guard)
    }

    /// タブごとの通知可否（デスクトップ/音/通知音の選択）を更新する。
    /// ストリーム/キャッシュには影響しない軽量操作。
    pub fn set_column_notify(
        &self,
        column_id: &str,
        notify_desktop: bool,
        notify_sound: bool,
        notify_sound_choice: &str,
    ) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        if let Some(c) = guard.columns.iter_mut().find(|x| x.id == column_id) {
            c.notify_desktop = notify_desktop;
            c.notify_sound = notify_sound;
            c.notify_sound_choice = notify_sound_choice.to_string();
        }
        self.save(&guard)
    }

    pub fn upsert_column(&self, c: &Column) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        match guard.columns.iter_mut().find(|x| x.id == c.id) {
            Some(existing) => *existing = c.clone(),
            None => guard.columns.push(c.clone()),
        }
        self.save(&guard)
    }

    pub fn delete_column(&self, column_id: &str) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard.columns.retain(|c| c.id != column_id);
        self.save(&guard)
    }

    // ---- NG（ミュート）設定 ----

    pub fn load_mute(&self) -> Result<MuteConfig> {
        Ok(self.data.lock().unwrap().mute.clone())
    }

    pub fn save_mute(&self, cfg: &MuteConfig) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard.mute = cfg.clone();
        self.save(&guard)
    }

    pub fn load_notify(&self) -> Result<NotifyConfig> {
        Ok(self.data.lock().unwrap().notify.clone())
    }

    pub fn save_notify(&self, cfg: &NotifyConfig) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard.notify = cfg.clone();
        self.save(&guard)
    }

    pub fn load_ui(&self) -> Result<UiPrefs> {
        Ok(self.data.lock().unwrap().ui.clone())
    }

    pub fn save_ui(&self, prefs: &UiPrefs) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard.ui = prefs.clone();
        self.save(&guard)
    }

    // ---- PaneNode（ペイン分割ツリー） ----

    /// 保存済みツリーがあればそれを返す。無ければ(旧バージョンからの移行)、既存の
    /// groups を order 順に並べた Row 分割としてその場で組み立てて返す
    /// (ファイルへの書き込みは行わない。実体化は次の分割/保存操作まで遅延する)。
    ///
    /// 保存済みツリーに、既に groups から消えたグループを指す Leaf(孤児)が残っている
    /// 場合は、返す前にその場で取り除く(木からも畳む)。`delete_account` 等、
    /// `delete_empty_groups` を経由しない経路でグループが消えた際の後始末を毎回の
    /// 読み込みで自己修復するため(Issue #31)。逆に、木の中に一切登場しない groups
    /// (`add_column` がまだ木への追加を実装していなかった旧バージョンで作られた
    /// グループ等)があれば、ルートのRow末尾に追加して補う。ディスクへの書き戻しは
    /// しない(読むたびに同じ掃除/補完を繰り返すだけで実害が無いため)。
    pub fn load_pane_layout(&self) -> Result<PaneNode> {
        let guard = self.data.lock().unwrap();
        let known: std::collections::HashSet<&str> =
            guard.groups.iter().map(|g| g.id.as_str()).collect();
        if let Some(root) = &guard.pane_layout {
            let mut orphan_ids = Vec::new();
            collect_group_ids(root, &mut orphan_ids);
            let present: std::collections::HashSet<String> = orphan_ids.iter().cloned().collect();
            orphan_ids.retain(|id| !known.contains(id.as_str()));
            let mut cleaned = root.clone();
            for gid in &orphan_ids {
                cleaned.remove_group(gid);
            }
            let root_is_valid = match &cleaned {
                PaneNode::Leaf { group_id, .. } => known.contains(group_id.as_str()),
                PaneNode::Split { .. } => true,
            };
            if root_is_valid {
                let mut missing: Vec<&ColumnGroup> = guard
                    .groups
                    .iter()
                    .filter(|g| !present.contains(&g.id))
                    .collect();
                missing.sort_by_key(|g| g.order);
                for g in missing {
                    cleaned.append_row_leaf(&g.id, g.width as f32);
                }
                return Ok(cleaned);
            }
            // ルート自体が孤児のLeafで自己削除できない(全グループが入れ替わった等)場合は
            // 下の再構築ロジックへフォールバックする。
        }
        let mut groups = guard.groups.clone();
        groups.sort_by_key(|g| g.order);
        if groups.len() == 1 {
            return Ok(PaneNode::new_leaf(groups[0].id.clone()));
        }
        Ok(PaneNode::Split {
            id: uuid::Uuid::new_v4().to_string(),
            direction: crate::domain::SplitDirection::Row,
            children: groups
                .iter()
                .map(|g| crate::domain::PaneChild {
                    node: PaneNode::new_leaf(g.id.clone()),
                    size: g.width as f32,
                    auto: g.auto,
                })
                .collect(),
        })
    }

    pub fn save_pane_layout(&self, root: &PaneNode) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        guard.pane_layout = Some(root.clone());
        self.save(&guard)
    }

    /// タブを別グループへ移動し、そのグループ内での順序を id 順に振り直す。
    pub fn move_tab(&self, tab_id: &str, group_id: &str, ordered_tab_ids: &[String]) -> Result<()> {
        let mut guard = self.data.lock().unwrap();
        if let Some(c) = guard.columns.iter_mut().find(|x| x.id == tab_id) {
            c.group_id = group_id.to_string();
        }
        for (i, id) in ordered_tab_ids.iter().enumerate() {
            if let Some(c) = guard.columns.iter_mut().find(|x| &x.id == id) {
                c.order = i as i32;
            }
        }
        self.save(&guard)
    }
}

/// 木の中に出現する group_id を(順不同で)全て集める。
fn collect_group_ids(node: &PaneNode, out: &mut Vec<String>) {
    match node {
        PaneNode::Leaf { group_id, .. } => out.push(group_id.clone()),
        PaneNode::Split { children, .. } => {
            for c in children {
                collect_group_ids(&c.node, out);
            }
        }
    }
}

fn load_json_or_default(path: &Path) -> Result<SettingsData> {
    if !path.exists() {
        return Ok(SettingsData::default());
    }
    let s = std::fs::read_to_string(path)?;
    let mut value: serde_json::Value = serde_json::from_str(&s)?;
    migrate_legacy_pane_group_id(&mut value);
    Ok(serde_json::from_value(value)?)
}

/// pane_layout(Issue #31)は当初 PaneNode::Leaf.group_id を "group_id" で書き出していたが、
/// 他フィールドとの camelCase 一貫性のため後から "groupId" にリネームした。この変更前の
/// ビルドで既に実データを永続化した既存ユーザ(開発者自身の環境含む)が起動不能になるのを防ぐため、
/// pane_layout 部分木内の "group_id" キーを "groupId" に読み替えてから型付きデシリアライズする。
fn migrate_legacy_pane_group_id(root: &mut serde_json::Value) {
    if let Some(pane_layout) = root.get_mut("pane_layout") {
        rename_key_recursive(pane_layout, "group_id", "groupId");
    }
}

fn rename_key_recursive(value: &mut serde_json::Value, from: &str, to: &str) {
    match value {
        serde_json::Value::Object(map) => {
            if !map.contains_key(to) {
                if let Some(v) = map.remove(from) {
                    map.insert(to.to_string(), v);
                }
            }
            for v in map.values_mut() {
                rename_key_recursive(v, from, to);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                rename_key_recursive(v, from, to);
            }
        }
        _ => {}
    }
}

/// 旧バージョン(SQLite一体型 tsumugi.db)からの一回限りの移行。
/// 既存の `db::open_settings`(スキーマ適用＋旧マイグレーション込み)で開いた接続から
/// 設定4テーブルを読み出し、新しい JSON ストアへ書き出す。
pub fn migrate_from_legacy_sqlite(
    json_path: &Path,
    legacy_conn: &rusqlite::Connection,
) -> Result<SettingsStore> {
    use rusqlite::params;

    let mut stmt = legacy_conn.prepare(
        "SELECT id, host, username, user_id, display_name, avatar_url FROM account ORDER BY rowid",
    )?;
    let accounts = stmt
        .query_map([], |r| {
            Ok(Account {
                id: r.get(0)?,
                host: r.get(1)?,
                username: r.get(2)?,
                user_id: r.get(3)?,
                display_name: r.get(4)?,
                avatar_url: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stmt =
        legacy_conn.prepare("SELECT id, ord, width, auto FROM column_group ORDER BY ord, rowid")?;
    let groups = stmt
        .query_map([], |r| {
            Ok(ColumnGroup {
                id: r.get(0)?,
                order: r.get(1)?,
                width: r.get(2)?,
                auto: r.get::<_, i64>(3)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stmt = legacy_conn.prepare(
        "SELECT id, account_id, kind, ord, filter, notify_sound, notify_desktop,
                group_id, title, notify_sound_choice
         FROM column_def ORDER BY ord, rowid",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i32>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, i64>(5)? != 0,
            r.get::<_, i64>(6)? != 0,
            r.get::<_, Option<String>>(7)?.unwrap_or_default(),
            r.get::<_, Option<String>>(8)?,
            r.get::<_, Option<String>>(9)?.unwrap_or_default(),
        ))
    })?;
    let mut columns = Vec::new();
    for row in rows {
        let (id, account_id, kind_json, ord, filter_json, notify_sound, notify_desktop, group_id, title, notify_sound_choice) =
            row?;
        columns.push(Column {
            id,
            account_id,
            kind: serde_json::from_str(&kind_json)?,
            order: ord,
            filter: serde_json::from_str(&filter_json)?,
            notify_sound,
            notify_desktop,
            notify_sound_choice,
            group_id,
            title,
        });
    }

    let get_kv = |key: &str| -> Result<Option<String>> {
        Ok(legacy_conn
            .query_row("SELECT value FROM app_setting WHERE key = ?1", params![key], |r| r.get(0))
            .ok())
    };
    let mute = get_kv("mute")?
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default();
    let notify = get_kv("notify")?
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default();
    let ui = get_kv("ui")?
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default();

    let store = SettingsStore {
        backing: Backing::File(json_path.to_path_buf()),
        data: Mutex::new(SettingsData { accounts, groups, columns, mute, notify, ui, pane_layout: None }),
    };
    store.save(&store.data.lock().unwrap())?;
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ColumnKind, FilterQuery, PaneChild, SplitDirection};

    fn store() -> SettingsStore {
        SettingsStore::new_in_memory()
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
            notify_sound_choice: String::new(),
            group_id: "g1".into(),
            title: None,
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
        s.upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 300, auto: false }).unwrap();
        s.upsert_group(&ColumnGroup { id: "g2".into(), order: 1, width: 300, auto: false }).unwrap();
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
        s.upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 300, auto: false }).unwrap();
        s.upsert_group(&ColumnGroup { id: "g2".into(), order: 1, width: 300, auto: false }).unwrap();
        s.set_group_width("g1", 420).unwrap();
        s.reorder_groups(&["g2".into(), "g1".into()]).unwrap();
        let groups = s.load_groups().unwrap();
        assert_eq!(groups[0].id, "g2");
        assert_eq!(groups[1].id, "g1");
        assert_eq!(groups[1].width, 420);
    }

    #[test]
    fn pane_layout_defaults_to_row_of_existing_groups_when_unset() {
        let s = SettingsStore::new_in_memory();
        let g1 = ColumnGroup { id: "g1".into(), order: 0, width: 300, auto: false };
        let g2 = ColumnGroup { id: "g2".into(), order: 1, width: 320, auto: false };
        s.upsert_group(&g1).unwrap();
        s.upsert_group(&g2).unwrap();
        let root = s.load_pane_layout().unwrap();
        let PaneNode::Split { direction, children, .. } = root else { panic!("expected Split") };
        assert_eq!(direction, SplitDirection::Row);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].size, 300.0);
        assert_eq!(children[1].size, 320.0);
    }

    #[test]
    fn load_pane_layout_prunes_orphaned_leaf_and_collapses() {
        // delete_account等、delete_empty_groupsを経由しない経路でグループが消えた場合の
        // 自己修復(Issue #31)。g1(存在)とorphan(groupsに無い)の2子Splitは、読み込み時点で
        // orphanが取り除かれ1子になり、g1単体のLeafへ畳まれて返る。
        let s = SettingsStore::new_in_memory();
        s.upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 300, auto: false }).unwrap();
        let stale = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Column,
            children: vec![
                PaneChild {
                    node: PaneNode::Leaf { id: "l1".into(), group_id: "g1".into() },
                    size: 1.0,
                    auto: false,
                },
                PaneChild {
                    node: PaneNode::Leaf { id: "l2".into(), group_id: "orphan".into() },
                    size: 1.0,
                    auto: false,
                },
            ],
        };
        s.save_pane_layout(&stale).unwrap();

        let cleaned = s.load_pane_layout().unwrap();
        let PaneNode::Leaf { group_id, .. } = &cleaned else { panic!("expected collapsed Leaf") };
        assert_eq!(group_id, "g1");
    }

    #[test]
    fn load_pane_layout_appends_groups_missing_from_saved_tree() {
        // 旧バージョンのadd_column(木への追加を実装する前)が作った、木に一切登場しない
        // グループがある場合、読み込み時点でRowの末尾に補って返す(Issue #31)。
        let s = SettingsStore::new_in_memory();
        s.upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 300, auto: false }).unwrap();
        s.upsert_group(&ColumnGroup { id: "g2".into(), order: 1, width: 400, auto: false }).unwrap();
        s.save_pane_layout(&PaneNode::new_leaf("g1")).unwrap();

        let root = s.load_pane_layout().unwrap();
        let PaneNode::Split { direction, children, .. } = &root else { panic!("expected Split") };
        assert_eq!(*direction, SplitDirection::Row);
        assert_eq!(children.len(), 2);
        let PaneNode::Leaf { group_id, .. } = &children[0].node else { panic!("expected leaf") };
        assert_eq!(group_id, "g1");
        let PaneNode::Leaf { group_id, .. } = &children[1].node else { panic!("expected leaf") };
        assert_eq!(group_id, "g2");
        assert_eq!(children[1].size, 400.0);
    }

    #[test]
    fn load_pane_layout_falls_back_to_reconstruction_when_root_itself_is_orphaned() {
        // 全グループが入れ替わった等でルート自体(裸のLeaf)が孤児になった場合は、
        // remove_groupで自己削除できないため、現在のgroupsから素直に再構築する。
        let s = SettingsStore::new_in_memory();
        s.upsert_group(&ColumnGroup { id: "g_new".into(), order: 0, width: 400, auto: false }).unwrap();
        s.save_pane_layout(&PaneNode::Leaf { id: "l1".into(), group_id: "g_old_gone".into() }).unwrap();

        let root = s.load_pane_layout().unwrap();
        let PaneNode::Leaf { group_id, .. } = &root else { panic!("expected Leaf") };
        assert_eq!(group_id, "g_new");
    }

    #[test]
    fn pane_layout_round_trips_through_save_and_load() {
        let s = SettingsStore::new_in_memory();
        s.upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 300, auto: false }).unwrap();
        let root = PaneNode::new_leaf("g1");
        s.save_pane_layout(&root).unwrap();
        assert_eq!(s.load_pane_layout().unwrap(), root);
    }

    #[test]
    fn delete_empty_groups_prunes_pane_layout_and_collapses() {
        let s = SettingsStore::new_in_memory();
        let g1 = ColumnGroup { id: "g1".into(), order: 0, width: 300, auto: false };
        let g2 = ColumnGroup { id: "g2".into(), order: 1, width: 300, auto: false };
        s.upsert_group(&g1).unwrap();
        s.upsert_group(&g2).unwrap();
        // g1をColumn方向に分割してg3を作る(タブは無い状態を模す)
        let mut root = s.load_pane_layout().unwrap();
        assert!(root.insert_sibling("g1", "g3", SplitDirection::Column));
        s.save_pane_layout(&root).unwrap();
        let g3 = ColumnGroup { id: "g3".into(), order: 2, width: 300, auto: false };
        s.upsert_group(&g3).unwrap();
        // g3にはタブが無いまま delete_empty_groups を呼ぶ(g1/g2にはタブがある体で columns に積む)
        let c1 = Column {
            id: "c1".into(), account_id: "a".into(), kind: ColumnKind::Home, order: 0,
            filter: FilterQuery::Keywords(vec![]), notify_sound: false, notify_desktop: false,
            notify_sound_choice: String::new(), group_id: "g1".into(), title: None,
        };
        let c2 = Column { id: "c2".into(), group_id: "g2".into(), ..c1.clone() };
        s.upsert_column(&c1).unwrap();
        s.upsert_column(&c2).unwrap();
        s.delete_empty_groups().unwrap();
        // g3(タブ無し)は消え、pane_layoutからも取り除かれてg1側に畳まれている
        let groups = s.load_groups().unwrap();
        assert_eq!(groups.iter().map(|g| g.id.clone()).collect::<Vec<_>>(), vec!["g1", "g2"]);
        let root = s.load_pane_layout().unwrap();
        let PaneNode::Split { children, .. } = root else { panic!("expected Split") };
        assert_eq!(children.len(), 2);
        let PaneNode::Leaf { group_id, .. } = &children[0].node else { panic!("expected leaf") };
        assert_eq!(group_id, "g1"); // g3が畳まれ、g1が直接の子に戻っている
    }

    #[test]
    fn group_auto_roundtrips_and_set_group_auto_updates() {
        let s = store();
        s.upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 300, auto: true }).unwrap();
        s.upsert_group(&ColumnGroup { id: "g2".into(), order: 1, width: 300, auto: false }).unwrap();
        let groups = s.load_groups().unwrap();
        assert!(groups.iter().find(|g| g.id == "g1").unwrap().auto);
        assert!(!groups.iter().find(|g| g.id == "g2").unwrap().auto);

        s.set_group_auto("g2", true).unwrap();
        let groups = s.load_groups().unwrap();
        assert!(groups.iter().find(|g| g.id == "g2").unwrap().auto);
    }

    /// JSONファイルへの実書き込み・再読み込みを検証（Memoryバッキングでは検証できない部分）。
    #[test]
    fn persists_to_plain_text_json_file_and_reloads() {
        let path = std::env::temp_dir().join(format!("tsumugi-settings-test-{}.json", uuid::Uuid::new_v4()));
        {
            let s = SettingsStore::new(path.clone()).unwrap();
            s.upsert_account(&account("a1")).unwrap();
            s.upsert_group(&ColumnGroup { id: "g1".into(), order: 0, width: 300, auto: false }).unwrap();
            s.upsert_column(&column("c1", "a1", 0)).unwrap();
        }

        // ファイルはプレーンテキスト(JSON)であること
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"a1\""));
        assert!(serde_json::from_str::<serde_json::Value>(&raw).is_ok());

        // 再読み込みでデータが復元される
        let reloaded = SettingsStore::new(path.clone()).unwrap();
        assert_eq!(reloaded.load_accounts().unwrap().len(), 1);
        assert_eq!(reloaded.load_columns().unwrap().len(), 1);

        std::fs::remove_file(&path).ok();
    }

    /// リネーム前のビルドが書き出した旧キー名("group_id")の pane_layout を含む設定ファイルでも
    /// 起動時に読み込めること(Issue #31)。実際にユーザ環境で発生した"missing field groupId"クラッシュの回帰テスト。
    #[test]
    fn loads_settings_with_legacy_snake_case_pane_layout_group_id() {
        let path = std::env::temp_dir()
            .join(format!("tsumugi-legacy-pane-layout-{}.json", uuid::Uuid::new_v4()));
        let legacy_json = r#"{
            "groups": [{"id": "g1", "order": 0, "width": 300, "auto": false}],
            "pane_layout": {
                "type": "split",
                "id": "root",
                "direction": "row",
                "children": [
                    {
                        "node": {"type": "leaf", "id": "l1", "group_id": "g1"},
                        "size": 300.0,
                        "auto": false
                    }
                ]
            }
        }"#;
        std::fs::write(&path, legacy_json).unwrap();

        let s = SettingsStore::new(path.clone()).unwrap();
        let root = s.load_pane_layout().unwrap();
        let PaneNode::Split { children, .. } = root else { panic!("expected Split") };
        let PaneNode::Leaf { group_id, .. } = &children[0].node else { panic!("expected Leaf") };
        assert_eq!(group_id, "g1");

        std::fs::remove_file(&path).ok();
    }

    /// 旧バージョン(SQLite一体型 tsumugi.db)からの移行: 実際に使う
    /// `db::open_settings`(スキーマ+旧マイグレーション適用)経由で開いた接続から、
    /// account/column/column_group/app_setting が新しいJSON設定へ正しく移ることを検証する。
    #[test]
    fn migrates_from_legacy_sqlite_to_json() {
        let legacy_path =
            std::env::temp_dir().join(format!("tsumugi-legacy-migrate-{}.db", uuid::Uuid::new_v4()));
        let json_path =
            std::env::temp_dir().join(format!("tsumugi-migrated-settings-{}.json", uuid::Uuid::new_v4()));

        let legacy_conn = crate::store::db::open_settings(&legacy_path).unwrap();
        legacy_conn
            .execute(
                "INSERT INTO account (id, host, username, user_id, display_name, avatar_url)
                 VALUES ('acc1', 'misskey.io', 'me', 'u1', 'Me', NULL)",
                [],
            )
            .unwrap();
        legacy_conn
            .execute("INSERT INTO column_group (id, ord, width, auto) VALUES ('g1', 0, 300, 0)", [])
            .unwrap();
        legacy_conn
            .execute(
                "INSERT INTO column_def (id, account_id, kind, ord, width, filter, notify_sound, notify_desktop, group_id)
                 VALUES ('c1', 'acc1', '{\"type\":\"home\"}', 0, 300, '{\"kind\":\"keywords\",\"value\":[]}', 0, 1, 'g1')",
                [],
            )
            .unwrap();
        legacy_conn
            .execute(
                "INSERT INTO app_setting (key, value) VALUES ('mute', '{\"ngWords\":[\"spam\"],\"ngUsers\":[],\"ngInstances\":[]}')",
                [],
            )
            .unwrap();

        let migrated = migrate_from_legacy_sqlite(&json_path, &legacy_conn).unwrap();
        drop(legacy_conn);

        assert_eq!(migrated.load_accounts().unwrap().len(), 1);
        assert_eq!(migrated.load_accounts().unwrap()[0].id, "acc1");
        assert_eq!(migrated.load_columns().unwrap().len(), 1);
        assert_eq!(migrated.load_groups().unwrap().len(), 1);
        assert_eq!(migrated.load_mute().unwrap().ng_words, vec!["spam".to_string()]);

        // JSONファイルとして実際に書き出されている(プレーンテキスト)ことも確認
        let raw = std::fs::read_to_string(&json_path).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&raw).is_ok());
        assert!(raw.contains("acc1"));

        std::fs::remove_file(&legacy_path).ok();
        std::fs::remove_file(&json_path).ok();
    }
}
