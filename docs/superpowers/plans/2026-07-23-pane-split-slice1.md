# カラムの縦分割（ペイン化） Slice 1 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 既存の視覚カラム(`ColumnGroup`)を「下に分割」して、その場に新しい空のペインを作れるようにする。分割は木構造(`PaneNode`)で永続化し、再起動後も復元され、最後のタブを閉じるとペインが自動的に畳まれる。

**Architecture:** Rust側に再帰的な2分木 `PaneNode`(`Leaf`/`Split`)を新設し、`SettingsData.pane_layout` として永続化する。フロントは起動時にこの木を取得し、新設の再帰コンポーネント `Pane.svelte` で描画する（`App.svelte` の従来の横一列 `.columns` 直書きをこれに置き換える）。既存の `ColumnGroup.width`/`auto`（px固定幅・横スクロール）は**このSliceでは変更しない**。木は「どのグループがどう配置されているか」だけを表し、各Leafの見た目(幅)は引き続き既存の `Column.svelte` が `group.width`/`group.auto` から決める。新設するのは「下に分割(Column方向)」１つのみ（右分割・ドラッグ移動・リサイズ・数値入力は本Sliceの対象外、design docのフルスコープの一部を切り出したもの）。

**Tech Stack:** Rust(Tauri) + Svelte 5(runes) + tauri-specta(型生成)。

## Global Constraints

- 仕様: `docs/superpowers/specs/2026-07-22-pane-split-design.md`（本Sliceは「挿入(insert_sibling)」「削除(remove_group)」の正規化ルールと、Columnの`flex-grow`ウェイト方式のみを実装対象とする。`move_pane`、Row方向の分割ボタン、`ColumnSettings.svelte`の%入力、`set_pane_auto`は次Sliceへ持ち越し）。
- 既存コマンド `set_group_width`/`set_group_auto`/`reorder_groups`・`ColumnGroup.width`/`auto` フィールドは削除しない(維持する)。
- 新しいUUIDは既存コードの慣習通り `uuid::Uuid::new_v4().to_string()` を使う。
- Rustのテストは `cargo test`、フロントは `pnpm check` で型チェックする。

---

### Task 1: `PaneNode`/`PaneChild`/`SplitDirection` ドメイン型と木操作

**Files:**
- Create: `src-tauri/src/domain/pane.rs`
- Modify: `src-tauri/src/domain/mod.rs`

**Interfaces:**
- Produces: `pub enum SplitDirection { Row, Column }`、`pub enum PaneNode { Leaf { id: String, group_id: String }, Split { id: String, direction: SplitDirection, children: Vec<PaneChild> } }`、`pub struct PaneChild { pub node: PaneNode, pub size: f32, pub auto: bool }`。メソッド `PaneNode::new_leaf(group_id) -> Self`、`PaneNode::insert_sibling(&mut self, reference_group_id: &str, new_group_id: &str, direction: SplitDirection) -> bool`、`PaneNode::remove_group(&mut self, group_id: &str) -> bool`。

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/domain/pane.rs` の末尾に追加(ファイル自体はまだ存在しないので、この時点ではファイル全体を新規作成するが、まずテスト関数だけを実装本体の上に書く形で以下の完成形を一度に書いてよい。TDDの都合上、先に以下のテストモジュールを含めてファイルを作成し、実装本体は空/`todo!()`にしてから次のStepで埋める):

```rust
use serde::{Deserialize, Serialize};
use specta::Type;

const DEFAULT_ROW_WEIGHT_PX: f32 = 300.0;
const DEFAULT_COLUMN_WEIGHT: f32 = 1.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SplitDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PaneNode {
    Leaf {
        id: String,
        group_id: String,
    },
    Split {
        id: String,
        direction: SplitDirection,
        children: Vec<PaneChild>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaneChild {
    pub node: PaneNode,
    pub size: f32,
    pub auto: bool,
}

impl PaneNode {
    pub fn new_leaf(group_id: impl Into<String>) -> Self {
        todo!()
    }

    fn default_weight(direction: SplitDirection) -> f32 {
        todo!()
    }

    pub fn insert_sibling(&mut self, reference_group_id: &str, new_group_id: &str, direction: SplitDirection) -> bool {
        todo!()
    }

    pub fn remove_group(&mut self, group_id: &str) -> bool {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_sibling_same_direction_halves_reference_size() {
        // root: Split(Row)[ Leaf(a, size=300), Leaf(b, size=300) ]
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 300.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 300.0, auto: false },
            ],
        };
        assert!(root.insert_sibling("a", "c", SplitDirection::Row));
        let PaneNode::Split { children, .. } = &root else { panic!("root must stay Split") };
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].size, 150.0); // 分割元(a)は半分になる
        assert_eq!(children[1].size, 150.0); // 新規(c)がもう半分
        assert_eq!(children[2].size, 300.0); // 無関係な兄弟(b)は変化しない
        let PaneNode::Leaf { group_id, .. } = &children[1].node else { panic!("expected leaf") };
        assert_eq!(group_id, "c");
    }

    #[test]
    fn insert_sibling_different_direction_wraps_reference() {
        // root: Leaf(a) のみ(まだ分割されていない単一グループ)
        let mut root = PaneNode::new_leaf("a");
        assert!(root.insert_sibling("a", "c", SplitDirection::Column));
        let PaneNode::Split { direction, children, .. } = &root else { panic!("root must become Split") };
        assert_eq!(*direction, SplitDirection::Column);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].size, 1.0);
        assert_eq!(children[1].size, 1.0);
        let PaneNode::Leaf { group_id, .. } = &children[0].node else { panic!("expected leaf") };
        assert_eq!(group_id, "a");
        let PaneNode::Leaf { group_id, .. } = &children[1].node else { panic!("expected leaf") };
        assert_eq!(group_id, "c");
    }

    #[test]
    fn insert_sibling_wraps_inside_existing_row_when_direction_differs() {
        // root: Split(Row)[ Leaf(a, size=300), Leaf(b, size=300) ]
        // a を下に分割(Column)すると、a の位置が Split(Column)[a, c] にラップされ、
        // root の子リスト自体は2つのまま(bはそのまま)、a のスロットの size(300)は維持される。
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 300.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 300.0, auto: false },
            ],
        };
        assert!(root.insert_sibling("a", "c", SplitDirection::Column));
        let PaneNode::Split { direction: root_dir, children, .. } = &root else { panic!("root must stay Split") };
        assert_eq!(*root_dir, SplitDirection::Row);
        assert_eq!(children.len(), 2); // aの位置がラップされただけで、root直下の子数は変わらない
        assert_eq!(children[0].size, 300.0); // ラップ後もaスロット自体のsizeは維持
        assert_eq!(children[1].size, 300.0); // bは無関係、変化なし
        let PaneNode::Split { direction: inner_dir, children: inner, .. } = &children[0].node else {
            panic!("a's slot must now be a Column split")
        };
        assert_eq!(*inner_dir, SplitDirection::Column);
        assert_eq!(inner.len(), 2);
        assert_eq!(inner[0].size, 1.0);
        assert_eq!(inner[1].size, 1.0);
    }

    #[test]
    fn insert_sibling_returns_false_when_reference_not_found() {
        let mut root = PaneNode::new_leaf("a");
        assert!(!root.insert_sibling("nope", "c", SplitDirection::Column));
    }

    #[test]
    fn remove_group_gives_freed_size_to_previous_sibling() {
        // root: Split(Row)[a(150), c(150), b(300)] から c を消す -> a が300に戻る、bは無関係
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 150.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lc".into(), group_id: "c".into() }, size: 150.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 300.0, auto: false },
            ],
        };
        assert!(root.remove_group("c"));
        let PaneNode::Split { children, .. } = &root else { panic!("root must stay Split (3->2 children)") };
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].size, 300.0); // aがcの分を引き継ぐ
        assert_eq!(children[1].size, 300.0); // bは無関係
    }

    #[test]
    fn remove_group_gives_freed_size_to_next_sibling_when_removing_first() {
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 150.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 300.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lc".into(), group_id: "c".into() }, size: 300.0, auto: false },
            ],
        };
        assert!(root.remove_group("a"));
        let PaneNode::Split { children, .. } = &root else { panic!("root must stay Split") };
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].size, 450.0); // 先頭(a)を消したので直後のbが引き継ぐ
    }

    #[test]
    fn remove_group_collapses_split_with_one_remaining_child() {
        // root: Split(Row)[ Split(Column)[a(1.0), c(1.0)] (size=300 in root), b(300) ]
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild {
                    node: PaneNode::Split {
                        id: "inner".into(),
                        direction: SplitDirection::Column,
                        children: vec![
                            PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 1.0, auto: false },
                            PaneChild { node: PaneNode::Leaf { id: "lc".into(), group_id: "c".into() }, size: 1.0, auto: false },
                        ],
                    },
                    size: 300.0,
                    auto: false,
                },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 300.0, auto: false },
            ],
        };
        assert!(root.remove_group("c"));
        let PaneNode::Split { children, .. } = &root else { panic!("root must stay Split") };
        assert_eq!(children.len(), 2); // rootの子数は変わらない(inner splitがLeafに畳まれただけ)
        // inner splitが畳まれ、aのLeafがrootの直接の子(size=300を引き継ぐ)になっている
        let PaneNode::Leaf { group_id, .. } = &children[0].node else { panic!("inner split must collapse into a leaf") };
        assert_eq!(group_id, "a");
        assert_eq!(children[0].size, 300.0); // 畳まれたSplit自身が親から見て持っていたsizeを引き継ぐ
    }

    #[test]
    fn remove_group_returns_false_when_group_not_found() {
        let mut root = PaneNode::new_leaf("a");
        assert!(!root.remove_group("nope"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test domain::pane:: -- --nocapture`
Expected: すべて `todo!()` の `not yet implemented` パニックで FAIL（コンパイル自体は通ること）。

- [ ] **Step 3: Implement `PaneNode`/`PaneChild`/`SplitDirection`**

`todo!()` を以下の実装で置き換える:

```rust
impl PaneNode {
    pub fn new_leaf(group_id: impl Into<String>) -> Self {
        PaneNode::Leaf {
            id: uuid::Uuid::new_v4().to_string(),
            group_id: group_id.into(),
        }
    }

    fn default_weight(direction: SplitDirection) -> f32 {
        match direction {
            SplitDirection::Row => DEFAULT_ROW_WEIGHT_PX,
            SplitDirection::Column => DEFAULT_COLUMN_WEIGHT,
        }
    }

    /// reference_group_id を持つ Leaf の直後に、new_group_id の Leaf を direction 方向へ
    /// 挿入する。reference の親が同じ direction の Split ならその子として差し込み、
    /// reference 自身の size を半分にして新規 Leaf に半分を渡す(他の兄弟は不変)。
    /// 異なる direction(またはルート直下の裸Leaf)なら reference の位置を新しい
    /// direction の Split でラップし、内部2子を既定ウェイトで均等にする。
    /// reference が見つからなければ false。
    pub fn insert_sibling(&mut self, reference_group_id: &str, new_group_id: &str, direction: SplitDirection) -> bool {
        if let PaneNode::Leaf { group_id, .. } = self {
            if group_id != reference_group_id {
                return false;
            }
            let old = std::mem::replace(self, PaneNode::new_leaf(String::new()));
            let w = Self::default_weight(direction);
            *self = PaneNode::Split {
                id: uuid::Uuid::new_v4().to_string(),
                direction,
                children: vec![
                    PaneChild { node: old, size: w, auto: false },
                    PaneChild { node: PaneNode::new_leaf(new_group_id), size: w, auto: false },
                ],
            };
            return true;
        }
        let PaneNode::Split { direction: my_dir, children, .. } = self else {
            unreachable!("Leaf case handled above")
        };
        if let Some(idx) = children
            .iter()
            .position(|c| matches!(&c.node, PaneNode::Leaf { group_id, .. } if group_id == reference_group_id))
        {
            if *my_dir == direction {
                let half = children[idx].size / 2.0;
                children[idx].size = half;
                children.insert(idx + 1, PaneChild { node: PaneNode::new_leaf(new_group_id), size: half, auto: false });
            } else {
                let old_child = children.remove(idx);
                let w = Self::default_weight(direction);
                let wrapped = PaneNode::Split {
                    id: uuid::Uuid::new_v4().to_string(),
                    direction,
                    children: vec![
                        PaneChild { node: old_child.node, size: w, auto: false },
                        PaneChild { node: PaneNode::new_leaf(new_group_id), size: w, auto: false },
                    ],
                };
                children.insert(idx, PaneChild { node: wrapped, size: old_child.size, auto: old_child.auto });
            }
            return true;
        }
        for child in children.iter_mut() {
            if child.node.insert_sibling(reference_group_id, new_group_id, direction) {
                return true;
            }
        }
        false
    }

    /// group_id を持つ Leaf を木から取り除く。空いた size は直前の兄弟(無ければ直後の
    /// 兄弟)にすべて譲る(他の兄弟は不変)。親の子が1つになったら、その親 Split を
    /// 残った子で置き換えて畳む(size/autoは畳まれたSplit自身が親から見て持っていた
    /// 値を引き継ぐ)。見つかって除去できたら true。ルート自体がLeafの場合は
    /// 除去対象が無い(最後の1ペインは呼び出し側で別途扱う)ので false。
    pub fn remove_group(&mut self, group_id: &str) -> bool {
        let PaneNode::Split { children, .. } = self else {
            return false;
        };
        if let Some(idx) = children
            .iter()
            .position(|c| matches!(&c.node, PaneNode::Leaf { group_id: g, .. } if g == group_id))
        {
            let removed = children.remove(idx);
            if idx > 0 {
                children[idx - 1].size += removed.size;
            } else if !children.is_empty() {
                children[0].size += removed.size;
            }
            if children.len() == 1 {
                let only = children.remove(0);
                *self = only.node;
            }
            return true;
        }
        for child in children.iter_mut() {
            if child.node.remove_group(group_id) {
                return true;
            }
        }
        false
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test domain::pane:: -- --nocapture`
Expected: 全テスト PASS。

- [ ] **Step 5: Register the module**

`src-tauri/src/domain/mod.rs` を開き、既存の `pub use column::{...}` に類する記述の並びに以下を追加する:

```rust
pub mod pane;
pub use pane::{PaneChild, PaneNode, SplitDirection};
```

- [ ] **Step 6: Run the full domain test suite**

Run: `cd src-tauri && cargo test domain::`
Expected: 既存テスト含め全て PASS。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/domain/pane.rs src-tauri/src/domain/mod.rs
git commit -m "feat: ペイン分割ツリー(PaneNode)のドメイン型と挿入/削除ロジックを追加"
```

---

### Task 2: `SettingsStore` への永続化とマイグレーション

**Files:**
- Modify: `src-tauri/src/store/settings.rs`

**Interfaces:**
- Consumes: `PaneNode`/`PaneChild`/`SplitDirection`（Task 1で追加）、既存の `SettingsData.groups: Vec<ColumnGroup>`。
- Produces: `SettingsStore::load_pane_layout(&self) -> Result<PaneNode>`、`SettingsStore::save_pane_layout(&self, root: &PaneNode) -> Result<()>`。既存の `delete_empty_groups` を拡張(シグネチャは変えない)。

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/store/settings.rs` の既存 `#[cfg(test)] mod tests` ブロック内(ファイル末尾、既存の `set_group_width_and_reorder` テスト等がある場所)に追記する:

```rust
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
    fn pane_layout_round_trips_through_save_and_load() {
        let s = SettingsStore::new_in_memory();
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test store::settings:: -- --nocapture`
Expected: `load_pane_layout`/`save_pane_layout` が存在しないコンパイルエラーで FAIL。

- [ ] **Step 3: Implement**

`src-tauri/src/store/settings.rs` の `use` 文に追加:

```rust
use crate::domain::{Account, Column, ColumnGroup, MuteConfig, NotifyConfig, PaneNode, UiPrefs};
```

`SettingsData` に1フィールド追加:

```rust
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
```

`delete_empty_groups` を以下に置き換え:

```rust
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
```

`load_ui`/`save_ui` の直後(既存の `move_tab` の手前)に追加:

```rust
    // ---- PaneNode（ペイン分割ツリー） ----

    /// 保存済みツリーがあればそれを返す。無ければ(旧バージョンからの移行)、既存の
    /// groups を order 順に並べた Row 分割としてその場で組み立てて返す
    /// (ファイルへの書き込みは行わない。実体化は次の分割/保存操作まで遅延する)。
    pub fn load_pane_layout(&self) -> Result<PaneNode> {
        let guard = self.data.lock().unwrap();
        if let Some(root) = &guard.pane_layout {
            return Ok(root.clone());
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
```

グループが0件のとき（起動直後の空状態）は `groups.len() == 1` にも該当しないため `Split{ children: vec![] }` が返る。これはフロント側で `app.groups.length === 0` の分岐が先に効くため描画されず問題ない。

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test store::settings::`
Expected: 全テスト PASS(既存テストも壊れていないこと)。

- [ ] **Step 5: Run the full Rust test suite**

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/store/settings.rs
git commit -m "feat: pane_layoutの永続化とdelete_empty_groups連動によるペイン畳み込みを追加"
```

---

### Task 3: Tauriコマンド `split_pane` / `load_pane_layout` / `discard_empty_group`

**Files:**
- Modify: `src-tauri/src/commands/column.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Task 2の `state.settings.load_pane_layout()`/`save_pane_layout()`/`delete_empty_groups()`、Task 1の `PaneNode::insert_sibling`。
- Produces: `#[tauri::command] async fn split_pane(state, reference_group_id: String, direction: SplitDirection) -> Result<ColumnGroup>`、`#[tauri::command] async fn load_pane_layout(state) -> Result<PaneNode>`、`#[tauri::command] async fn discard_empty_group(state, group_id: String) -> Result<()>`。

- [ ] **Step 1: Implement the commands**

`src-tauri/src/commands/column.rs` の `use crate::domain::{...}` に `PaneNode, SplitDirection` を追加:

```rust
use crate::domain::{
    Column, ColumnGroup, ColumnKind, FilterQuery, Note, Notification, PaneNode, SourceItem,
    SplitDirection, User, UserList,
};
```

`add_column` の直後に追加:

```rust
/// reference_group_id の隣に空の新規グループ(タブなし)を挿入し、その ColumnGroup を返す。
/// フロントは戻り値の group.id で AddColumnModal を「このグループにタブ追加」モードで開く。
#[tauri::command]
#[specta::specta]
pub async fn split_pane(
    state: State<'_, AppState>,
    reference_group_id: String,
    direction: SplitDirection,
) -> Result<ColumnGroup> {
    let order = state.settings.load_groups()?.len() as i32;
    let width = state
        .settings
        .load_ui()
        .map(|p| p.default_column_width)
        .unwrap_or(DEFAULT_WIDTH)
        .clamp(220, 720);
    let group = ColumnGroup { id: uuid::Uuid::new_v4().to_string(), order, width, auto: false };
    state.settings.upsert_group(&group)?;

    let mut root = state.settings.load_pane_layout()?;
    if !root.insert_sibling(&reference_group_id, &group.id, direction) {
        // reference_group_idはフロントが既存グループのidしか渡さない前提のため通常到達しないが、
        // 到達した場合は作成済みの空グループ(まだタブが無い)を後始末してからエラーを返す。
        state.settings.delete_empty_groups()?;
        return Err(Error::Invalid(format!("unknown reference group: {reference_group_id}")));
    }
    state.settings.save_pane_layout(&root)?;
    Ok(group)
}

/// 永続化済みペイン分割ツリー(起動時のレイアウト復元用)。
#[tauri::command]
#[specta::specta]
pub async fn load_pane_layout(state: State<'_, AppState>) -> Result<PaneNode> {
    state.settings.load_pane_layout()
}

/// タブが1つも無い空グループを削除する(split_paneでタブ追加をキャンセルされた後始末用)。
/// タブが残っている場合は何もしない(誤操作防止)。
#[tauri::command]
#[specta::specta]
pub async fn discard_empty_group(state: State<'_, AppState>, group_id: String) -> Result<()> {
    let has_tabs = state.settings.load_columns()?.iter().any(|c| c.group_id == group_id);
    if has_tabs {
        return Ok(());
    }
    state.settings.delete_empty_groups()?; // group_id自体がタブ0件ならここで消え、木からも畳まれる
    Ok(())
}
```

- [ ] **Step 2: Register in `specta_builder()`**

`src-tauri/src/lib.rs` の `commands::column::add_column,` の直後に追加:

```rust
            commands::column::split_pane,
            commands::column::load_pane_layout,
            commands::column::discard_empty_group,
```

- [ ] **Step 3: Run `cargo test` to regenerate TS bindings and verify compilation**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts` に `splitPane`/`loadPaneLayout`/`discardEmptyGroup`/`PaneNode`/`PaneChild`/`SplitDirection` が生成される。

- [ ] **Step 4: Run the full Rust test suite**

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: split_pane/load_pane_layout/discard_empty_groupコマンドを追加しTSバインディングを再生成"
```

---

### Task 4: フロント store — `paneRoot` の保持と `splitPane`/`discardEmptyGroup`

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: `commands.loadPaneLayout()`, `commands.splitPane(referenceGroupId, direction)`, `commands.discardEmptyGroup(groupId)`(いずれもTask 3で生成されたバインディング), 既存の `unwrap`/`this.#fail`。
- Produces: `AppStore.paneRoot: PaneNode`、`AppStore.splitPane(groupId: string, direction: "row" | "column"): Promise<string | null>`(新規グループidを返す。失敗時null)、`AppStore.discardEmptyGroup(groupId: string): Promise<void>`。

- [ ] **Step 1: Add `paneRoot` state and load it during boot**

`frontend/src/lib/store.svelte.ts` の `groups = $state<GroupView[]>([]);` の直後に追加:

```typescript
  paneRoot = $state<PaneNode>({ type: "split", id: "boot", direction: "row", children: [] });
```

ファイル先頭の型importに `PaneNode` を追加(既存の `import type { ... } from "../bindings/tauri.gen";` へ追記)。

`boot()` 内、`this.groups = groupDefs.map(...)` の直後に追加:

```typescript
      this.paneRoot = await unwrap(commands.loadPaneLayout());
```

- [ ] **Step 2: Add `splitPane`/`discardEmptyGroup`**

`setGroupAuto` の直後(グループ関連メソッド群の末尾)に追加:

```typescript
  // ---- ペイン分割(Issue #31 Slice 1: 下方向のみ) ----

  async splitPane(groupId: string, direction: "row" | "column"): Promise<string | null> {
    try {
      const newGroup = await unwrap(commands.splitPane(groupId, direction));
      this.groups = [...this.groups, { id: newGroup.id, width: newGroup.width, auto: newGroup.auto, tabs: [], activeTabId: "" }];
      this.paneRoot = await unwrap(commands.loadPaneLayout());
      return newGroup.id;
    } catch (e) {
      this.#fail(e);
      return null;
    }
  }

  /// splitPaneで作ったがタブ追加をキャンセルされた空グループを後始末する
  /// (discardEmptyGroupコマンドはタブが残っている場合は何もしないので、
  /// 成功時にも安全に呼べる)。
  async discardEmptyGroup(groupId: string) {
    const g = this.groups.find((x) => x.id === groupId);
    if (!g || g.tabs.length > 0) return; // 既にタブが追加済みなら何もしない
    try {
      await unwrap(commands.discardEmptyGroup(groupId));
      this.groups = this.groups.filter((x) => x.id !== groupId);
      this.paneRoot = await unwrap(commands.loadPaneLayout());
    } catch (e) {
      this.#fail(e);
    }
  }
```

- [ ] **Step 3: Type-check**

Run: `cd frontend && pnpm check`
Expected: エラー無し(既存のエラーが無い状態から変化しないこと)。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts frontend/src/lib/store.svelte.ts
git commit -m "feat: フロントにpaneRoot状態とsplitPane/discardEmptyGroupを追加"
```

---

### Task 5: `Pane.svelte`(再帰描画) と `App.svelte`/`Column.svelte` への配線

**Files:**
- Create: `frontend/src/ui/Pane.svelte`
- Modify: `frontend/src/App.svelte`
- Modify: `frontend/src/ui/Column.svelte`

**Interfaces:**
- Consumes: `app.paneRoot`(Task 4)、`app.groups`、`app.splitPane`/`app.discardEmptyGroup`(Task 4)。
- Produces: `Pane.svelte` の props `{ node: PaneNode; onAddTab; onEditTab; onEditGroup; onSplitDown }`。`Column.svelte` に新規 prop `onSplitDown: (groupId: string) => void`。

- [ ] **Step 1: Create `Pane.svelte`**

```svelte
<script lang="ts">
  import type { PaneNode } from "../bindings/tauri.gen";
  import type { TabView } from "../lib/store.svelte";
  import { app } from "../lib/store.svelte";
  import Column from "./Column.svelte";

  let {
    node,
    onAddTab,
    onEditTab,
    onEditGroup,
    onSplitDown,
  }: {
    node: PaneNode;
    onAddTab: (groupId: string) => void;
    onEditTab: (tab: TabView) => void;
    onEditGroup: (groupId: string) => void;
    onSplitDown: (groupId: string) => void;
  } = $props();
</script>

{#if node.type === "leaf"}
  {@const group = app.groups.find((g) => g.id === node.groupId)}
  {#if group}
    <Column {group} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
  {/if}
{:else if node.direction === "row"}
  <div class="row">
    {#each node.children as child (child.node.id)}
      <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
    {/each}
  </div>
{:else}
  <div class="col">
    {#each node.children as child (child.node.id)}
      <div class="col-item" style={`flex:${child.size} 1 0`}>
        <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} />
      </div>
    {/each}
  </div>
{/if}

<style>
  .row {
    display: flex;
    height: 100%;
    overflow-x: auto;
  }
  .col {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .col-item {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }
</style>
```

`node.type === "leaf"` の分岐で該当 `group` が(理論上)見つからないケース(タブ0件で描画されない空グループ等)は何も描画しない。

- [ ] **Step 2: Swap `App.svelte`'s `.columns` block for `<Pane>`**

`frontend/src/App.svelte` の以下のブロックを:

```svelte
      <div class="columns">
        {#each app.groups as group (group.id)}
          <Column {group} onAddTab={openAddTab} onEditTab={openEditTab} onEditGroup={openColumnSettings} />
        {/each}
      </div>
```

以下に置き換える:

```svelte
      <div class="columns">
        <Pane node={app.paneRoot} onAddTab={openAddTab} onEditTab={openEditTab} onEditGroup={openColumnSettings} onSplitDown={splitDown} />
      </div>
```

`import Column from "./ui/Column.svelte";` の直後に `import Pane from "./ui/Pane.svelte";` を追加。

`.columns` のCSS(既存の `display:flex; height:100%; overflow-x:auto;`)は `Pane.svelte` の `.row` と重複するが、ルート要素がRow分割なら二重にflexコンテナがネストされるだけで実害は無いため、`App.svelte` 側の `.columns` はそのまま残してよい(削除するとcolumns内のパディング等他のスタイルに影響しうるため、本Sliceでは触らない)。

`addTabGroupId`/`editTab`等の宣言の並びに追加:

```typescript
  let pendingSplitGroupId = $state<string | null>(null);

  async function splitDown(groupId: string) {
    const newGroupId = await app.splitPane(groupId, "column");
    if (!newGroupId) return;
    pendingSplitGroupId = newGroupId;
    openAddTab(newGroupId);
  }
```

`showAddColumn` の `AddColumnModal` の `onclose` を以下に変更:

```svelte
    <AddColumnModal
      groupId={addTabGroupId}
      editTab={editTab ?? undefined}
      onclose={() => {
        showAddColumn = false;
        if (pendingSplitGroupId) {
          void app.discardEmptyGroup(pendingSplitGroupId);
          pendingSplitGroupId = null;
        }
      }}
    />
```

(`discardEmptyGroup` は内部でタブが既に追加済みなら何もしないので、成功時にもここを通って問題ない。)

- [ ] **Step 3: Add the split button to `Column.svelte`**

`frontend/src/ui/Column.svelte` の props に `onSplitDown` を追加:

```typescript
  let {
    group,
    onAddTab,
    onEditTab,
    onEditGroup,
    onSplitDown,
  }: {
    group: GroupView;
    onAddTab: (groupId: string) => void;
    onEditTab: (tab: TabView) => void;
    onEditGroup: (groupId: string) => void;
    onSplitDown: (groupId: string) => void;
  } = $props();
```

タブバー内、`<button class="tab-add" ...>` の直後に追加:

```svelte
    <button class="tab-add" title="下に分割" onclick={() => onSplitDown(group.id)}>⬓</button>
```

CSS はそのまま `.tab-add` を流用するので追加不要。

- [ ] **Step 4: Type-check**

Run: `cd frontend && pnpm check`
Expected: エラー無し。

- [ ] **Step 5: Manual verification**

Run: `cargo tauri dev`(プロジェクトルートで)

1. 既存カラムのタブバーの「⬓」ボタンをクリック → AddColumnModalが開く → 適当なソースを選んで追加 → 元のカラムの下に新しいペインが表示されること。
2. アプリを再起動 → 縦分割が維持されていること。
3. 新しく作ったペインの最後のタブを閉じる → ペインが消え、元のカラムが縦分割前と同じ高さ(画面いっぱい)に戻ること。
4. 「⬓」をクリックした直後にAddColumnModalを閉じる(キャンセル) → 空ペインが残らないこと。

- [ ] **Step 6: Commit**

```bash
git add frontend/src/ui/Pane.svelte frontend/src/App.svelte frontend/src/ui/Column.svelte
git commit -m "feat: Pane.svelteによる木構造描画と「下に分割」ボタンを追加"
```

---

## Slice 1 完了後にやること

- design doc(`docs/superpowers/specs/2026-07-22-pane-split-design.md`)のうち未実装(`move_pane`によるドラッグ移動、Row方向の分割ボタン、`resize_pane`/`set_pane_auto`とColumn境界のドラッグリサイズ、`ColumnSettings.svelte`の%数値入力、`ColumnGroup.width`/`auto`の`PaneChild`への統合)を、Slice 1の動作確認後に別Sliceとして計画する。
