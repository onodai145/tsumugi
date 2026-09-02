# カラムの縦分割（ペイン化） Slice 3: 右分割・境界ドラッグリサイズ・ペイン移動 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** design doc(`docs/superpowers/specs/2026-07-22-pane-split-design.md`)のフルスコープのうち、Slice 1(下分割ボタン)・Slice 2(高さ%数値入力)で未実装のまま残っている3つを実装する: (1) 「右に分割」ボタン、(2) ペイン境界のマウスドラッグによるリサイズ(Row内のネストしたSplitブロックの幅、Column分割の高さ)、(3) `move_pane`によるドラッグ&ドロップでのペイン移動(行をまたいだ移動を含む)。これで design doc のスコープが全て揃う。

**Architecture:** バックエンドは既存の `PaneNode::insert_sibling`/`remove_group` をそのまま再利用し、新設の `Edge`(Left/Right/Top/Bottom) と、`insert_sibling` を「reference の前/後どちらに挿入するか」を選べるよう内部的に一般化した `insert_sibling_at` を追加する。`move_pane` コマンドは内部的に「削除(`remove_group`)→挿入(`insert_sibling_at`)」の組み合わせで実装する(design doc記載の通り)。フロントは、(2)は `Pane.svelte` の各分割の境界に薄いドラッグハンドルを追加し(Row内Split子は独立px、Column内は隣接ペアで重みを保ったまま増減)、ドラッグ終了時に既存の `resizePane` を呼んで永続化する。(3)は既存のグループドラッグ(`app.draggingGroupId`)を「ライブ並べ替え+`reorderGroups`」方式から「ドロップ先のColumn.svelteの4辺をエッジ判定してハイライトのみ→dragend時に`movePane`を1回呼ぶ」方式に置き換える(design doc: 「同一行内での並べ替えはLeft/Rightエッジへのドロップとして自然に包含される」)。

**Tech Stack:** Rust(Tauri) + Svelte 5(runes) + tauri-specta(型生成) + Vitest。

## Global Constraints

- 仕様: `docs/superpowers/specs/2026-07-22-pane-split-design.md`。本Sliceは以下3点のみを対象とする: 「### Tauriコマンド」節の `move_pane`/`Edge`、「### UI (`ui/Column.svelte`拡張)」節の四辺ドロップゾーン、「### UI (`ui/Pane.svelte`新設)」節の境界ドラッグリサイズ。ペイン中央ドロップでのタブ統合、ペインのズーム/最大化、キーボードショートカットでの分割操作は非対象(design doc「## 非対象(YAGNI)」のまま)。
- 既存の `reorder_groups` コマンド・`ColumnGroup.order` フィールドは削除しない(タブのグループ間移動時の `endDragTab` がまだ使っているため)。本Sliceで変更するのはグループ自体をドラッグする `startDragGroup`/`dragOverGroup`/`endDragGroup` の実装だけ。
- 新しいUUIDは既存コードの慣習通り `uuid::Uuid::new_v4().to_string()` を使う(ただし本SliceではLeaf/Splitの新規idはすべて既存メソッド経由で自動生成されるため、直接書くことはない)。
- Rustのテストは `cargo test`(`src-tauri`ディレクトリ)、フロントは `pnpm check` と `pnpm test`(`frontend`ディレクトリ)で確認する。
- pxのclamp(220〜720)・%のclamp(5〜95)は既存の `ColumnSettings.svelte`/`Column.svelte` と同じ値を使う。

---

### Task 1: `Edge` 型と `PaneNode::insert_sibling_at`(前/後を選べる挿入)

**Files:**
- Modify: `src-tauri/src/domain/pane.rs`
- Modify: `src-tauri/src/domain/mod.rs`

**Interfaces:**
- Consumes: 既存の `PaneNode::insert_sibling`(このタスクで内部実装を委譲先に置き換えるが、公開シグネチャ・挙動は変えない=既存の呼び出し元(`split_pane`)・既存テストはそのまま通る)。
- Produces: `pub enum Edge { Left, Right, Top, Bottom }`(`Serialize`/`Deserialize`/`Type`/`PartialEq`/`Eq`/`Copy`、`#[serde(rename_all = "camelCase")]`)。`impl Edge { pub fn direction(self) -> SplitDirection; pub fn before(self) -> bool; }`(Left/Top→before=true、Right/Bottom→before=false)。`PaneNode::insert_sibling_at(&mut self, reference_group_id: &str, new_group_id: &str, direction: SplitDirection, before: bool) -> bool`(Task 2の`move_pane`から使う)。

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/domain/pane.rs` の `#[cfg(test)] mod tests` 内、`insert_sibling_returns_false_when_reference_not_found` テストの直後に追加:

```rust
    #[test]
    fn edge_direction_and_before() {
        assert_eq!(Edge::Left.direction(), SplitDirection::Row);
        assert!(Edge::Left.before());
        assert_eq!(Edge::Right.direction(), SplitDirection::Row);
        assert!(!Edge::Right.before());
        assert_eq!(Edge::Top.direction(), SplitDirection::Column);
        assert!(Edge::Top.before());
        assert_eq!(Edge::Bottom.direction(), SplitDirection::Column);
        assert!(!Edge::Bottom.before());
    }

    #[test]
    fn insert_sibling_at_before_same_direction_inserts_ahead_of_reference() {
        // root: Split(Row)[ Leaf(a, size=300), Leaf(b, size=300) ] に、aの手前(before)へcを挿入。
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 300.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 300.0, auto: false },
            ],
        };
        assert!(root.insert_sibling_at("a", "c", SplitDirection::Row, true));
        let PaneNode::Split { children, .. } = &root else { panic!("root must stay Split") };
        assert_eq!(children.len(), 3);
        // 新規(c)がaの手前、aは元の位置のまま(半分ずつに折半)
        let PaneNode::Leaf { group_id, .. } = &children[0].node else { panic!("expected leaf") };
        assert_eq!(group_id, "c");
        assert_eq!(children[0].size, 150.0);
        let PaneNode::Leaf { group_id, .. } = &children[1].node else { panic!("expected leaf") };
        assert_eq!(group_id, "a");
        assert_eq!(children[1].size, 150.0);
        assert_eq!(children[2].size, 300.0); // bは無関係、変化なし
    }

    #[test]
    fn insert_sibling_at_after_same_direction_matches_existing_insert_sibling() {
        // before=false は既存のinsert_sibling(常に直後へ挿入)と同じ構造になる。
        // 新規Leaf(c)のidはinsert_sibling/insert_sibling_at呼び出しのたびに乱数生成される
        // ため、id自体は比較せず、size/auto/group_idの並びが一致することだけを見る。
        let mut a = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 300.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 300.0, auto: false },
            ],
        };
        let mut b = a.clone();
        assert!(a.insert_sibling("a", "c", SplitDirection::Row));
        assert!(b.insert_sibling_at("a", "c", SplitDirection::Row, false));
        let PaneNode::Split { children: children_a, .. } = &a else { panic!("expected Split") };
        let PaneNode::Split { children: children_b, .. } = &b else { panic!("expected Split") };
        assert_eq!(children_a.len(), children_b.len());
        for (x, y) in children_a.iter().zip(children_b.iter()) {
            assert_eq!(x.size, y.size);
            assert_eq!(x.auto, y.auto);
            let PaneNode::Leaf { group_id: gx, .. } = &x.node else { panic!("expected leaf") };
            let PaneNode::Leaf { group_id: gy, .. } = &y.node else { panic!("expected leaf") };
            assert_eq!(gx, gy);
        }
    }

    #[test]
    fn insert_sibling_at_before_wraps_reference_when_direction_differs() {
        // root: Leaf(a) のみ。Column方向・before=trueで挿入すると、Split(Column)[c, a]になる
        // (aが後ろに来る=「aの上に分割」)。
        let mut root = PaneNode::new_leaf("a");
        assert!(root.insert_sibling_at("a", "c", SplitDirection::Column, true));
        let PaneNode::Split { direction, children, .. } = &root else { panic!("root must become Split") };
        assert_eq!(*direction, SplitDirection::Column);
        assert_eq!(children.len(), 2);
        let PaneNode::Leaf { group_id, .. } = &children[0].node else { panic!("expected leaf") };
        assert_eq!(group_id, "c");
        let PaneNode::Leaf { group_id, .. } = &children[1].node else { panic!("expected leaf") };
        assert_eq!(group_id, "a");
    }

    #[test]
    fn insert_sibling_at_returns_false_when_reference_not_found() {
        let mut root = PaneNode::new_leaf("a");
        assert!(!root.insert_sibling_at("nope", "c", SplitDirection::Column, true));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test domain::pane:: -- --nocapture`
Expected: `Edge`/`insert_sibling_at` が存在しないコンパイルエラーで FAIL。

- [ ] **Step 3: Implement**

`src-tauri/src/domain/pane.rs` の `SplitDirection` の直後に追加:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    pub fn direction(self) -> SplitDirection {
        match self {
            Edge::Left | Edge::Right => SplitDirection::Row,
            Edge::Top | Edge::Bottom => SplitDirection::Column,
        }
    }

    pub fn before(self) -> bool {
        matches!(self, Edge::Left | Edge::Top)
    }
}
```

`impl PaneNode` 内の `insert_sibling` メソッドの中身を、`before: false` を渡すだけの薄い委譲に置き換え、同じ内容を `insert_sibling_at` として `before: bool` 引数を受け取る形に一般化する。既存の `insert_sibling` 全体を以下で置き換える:

```rust
    pub fn insert_sibling(&mut self, reference_group_id: &str, new_group_id: &str, direction: SplitDirection) -> bool {
        self.insert_sibling_at(reference_group_id, new_group_id, direction, false)
    }

    /// insert_siblingの一般化版。beforeがtrueならreferenceの手前に、falseなら直後に
    /// 新規Leafを挿入する(挙動の詳細はinsert_siblingのドキュメントコメント参照。
    /// before/afterの違いは「新規Leafとreferenceのどちらが子リストで先に来るか」だけで、
    /// size折半・auto継承・ラップの計算方法自体は同じ)。
    pub fn insert_sibling_at(&mut self, reference_group_id: &str, new_group_id: &str, direction: SplitDirection, before: bool) -> bool {
        if let PaneNode::Leaf { group_id, .. } = self {
            if group_id != reference_group_id {
                return false;
            }
            let old = std::mem::replace(self, PaneNode::new_leaf(String::new()));
            let (w, auto) = Self::default_wrap_child(direction);
            let new_child = PaneChild { node: PaneNode::new_leaf(new_group_id), size: w, auto };
            let old_child = PaneChild { node: old, size: w, auto };
            let children = if before { vec![new_child, old_child] } else { vec![old_child, new_child] };
            *self = PaneNode::Split { id: uuid::Uuid::new_v4().to_string(), direction, children };
            return true;
        }
        let PaneNode::Split { direction: my_dir, children, .. } = self else {
            unreachable!("Leaf case handled above")
        };
        if let Some(idx) = children
            .iter()
            .position(|c| matches!(&c.node, PaneNode::Leaf { group_id, .. } if group_id == reference_group_id))
        {
            let insert_at = if before { idx } else { idx + 1 };
            if *my_dir == direction {
                if direction == SplitDirection::Column && children[idx].auto {
                    children.insert(
                        insert_at,
                        PaneChild { node: PaneNode::new_leaf(new_group_id), size: DEFAULT_COLUMN_AUTO_FALLBACK_PERCENT, auto: true },
                    );
                } else {
                    let half = children[idx].size / 2.0;
                    children[idx].size = half;
                    children.insert(insert_at, PaneChild { node: PaneNode::new_leaf(new_group_id), size: half, auto: false });
                }
            } else {
                let old_child = children.remove(idx);
                let (w, auto) = Self::default_wrap_child(direction);
                let new_child = PaneChild { node: PaneNode::new_leaf(new_group_id), size: w, auto };
                let old_wrapped = PaneChild { node: old_child.node, size: w, auto };
                let inner_children = if before { vec![new_child, old_wrapped] } else { vec![old_wrapped, new_child] };
                let wrapped = PaneNode::Split { id: uuid::Uuid::new_v4().to_string(), direction, children: inner_children };
                children.insert(idx, PaneChild { node: wrapped, size: old_child.size, auto: old_child.auto });
            }
            return true;
        }
        for child in children.iter_mut() {
            if child.node.insert_sibling_at(reference_group_id, new_group_id, direction, before) {
                return true;
            }
        }
        false
    }
```

この置き換えにより、Step 1で追加した `insert_sibling_at_after_same_direction_matches_existing_insert_sibling` が「`insert_sibling`(旧実装のまま)」と「`insert_sibling_at(..., before: false)`」を比較する形になるが、`insert_sibling` 自体を委譲に変えたので実質「新実装 vs 新実装」になる。既存の `insert_sibling_*` テスト群(`insert_sibling_same_direction_halves_reference_size` 等)が壊れていないことも合わせて確認する。

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test domain::pane:: -- --nocapture`
Expected: 全テスト PASS(既存の `insert_sibling_*` テストも含む)。

- [ ] **Step 5: Export `Edge` from `domain/mod.rs`**

`src-tauri/src/domain/mod.rs` の `pub use pane::{PaneChild, PaneNode, SplitDirection};` を以下に変更(Task 2で `commands/column.rs` から `crate::domain::Edge` として使うため):

```rust
pub use pane::{Edge, PaneChild, PaneNode, SplitDirection};
```

- [ ] **Step 6: Run the full Rust test suite**

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/domain/pane.rs src-tauri/src/domain/mod.rs
git commit -m "feat: Edge型とPaneNode::insert_sibling_atを追加(move_pane用)"
```

---

### Task 2: Tauriコマンド `move_pane`

**Files:**
- Modify: `src-tauri/src/commands/column.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Task 1の `PaneNode::remove_group`(既存)/`insert_sibling_at`/`Edge`。
- Produces: `#[tauri::command] async fn move_pane(state, dragged_group_id: String, target_group_id: String, edge: Edge) -> Result<()>`。

- [ ] **Step 1: Implement the command**

`src-tauri/src/commands/column.rs` の `use crate::domain::{...}` に `Edge` を追加:

```rust
use crate::domain::{
    Column, ColumnGroup, ColumnKind, Edge, FilterQuery, MuteConfig, Note, Notification, PaneNode,
    SourceItem, SplitDirection, User, UserList,
};
```

`set_pane_auto` の直後に追加:

```rust
/// dragged_group_idを木から取り外し(親が1子になれば畳む)、target_group_idの指定エッジに
/// 挿入する(内部的には「remove_group→insert_sibling_at」の組み合わせ)。
/// dragged_group_id == target_group_idの場合は何もしない(同じ場所への無意味なドロップ)。
#[tauri::command]
#[specta::specta]
pub async fn move_pane(state: State<'_, AppState>, dragged_group_id: String, target_group_id: String, edge: Edge) -> Result<()> {
    if dragged_group_id == target_group_id {
        return Ok(());
    }
    let mut root = state.settings.load_pane_layout()?;
    if !root.remove_group(&dragged_group_id) {
        return Err(Error::Invalid(format!("unknown dragged group: {dragged_group_id}")));
    }
    if !root.insert_sibling_at(&target_group_id, &dragged_group_id, edge.direction(), edge.before()) {
        return Err(Error::Invalid(format!("unknown target group: {target_group_id}")));
    }
    state.settings.save_pane_layout(&root)
}
```

- [ ] **Step 2: Register in `specta_builder()`**

`src-tauri/src/lib.rs` の `commands::column::set_pane_auto,` の直後に追加:

```rust
            commands::column::move_pane,
```

- [ ] **Step 3: Run `cargo test` to regenerate TS bindings and verify compilation**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts` に `movePane` と `export type Edge = "left" | "right" | "top" | "bottom";` が生成される。

- [ ] **Step 4: Run the full Rust test suite**

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: move_paneコマンドを追加"
```

---

### Task 3: フロント — エッジ判定の純粋関数 `paneEdge.ts`

**Files:**
- Create: `frontend/src/lib/paneEdge.ts`
- Create: `frontend/src/lib/paneEdge.test.ts`

**Interfaces:**
- Produces: `export function edgeFromPointer(offsetX: number, offsetY: number, width: number, height: number): Edge | null`(`Edge` は `"left" | "right" | "top" | "bottom"`。要素の中心に近いほど`null`=ドロップ対象外を返す。四辺いずれかまでの距離が最小かつ幅/高さの25%以内ならそのエッジを返す)。Task 5で `Column.svelte` から使う。

- [ ] **Step 1: Write the failing tests**

`frontend/src/lib/paneEdge.test.ts`:

```typescript
import { describe, expect, it } from "vitest";
import { edgeFromPointer } from "./paneEdge";

describe("edgeFromPointer", () => {
  it("returns left when pointer is near the left edge", () => {
    expect(edgeFromPointer(5, 100, 200, 200)).toBe("left");
  });

  it("returns right when pointer is near the right edge", () => {
    expect(edgeFromPointer(195, 100, 200, 200)).toBe("right");
  });

  it("returns top when pointer is near the top edge", () => {
    expect(edgeFromPointer(100, 5, 200, 200)).toBe("top");
  });

  it("returns bottom when pointer is near the bottom edge", () => {
    expect(edgeFromPointer(100, 195, 200, 200)).toBe("bottom");
  });

  it("returns null at the dead center", () => {
    expect(edgeFromPointer(100, 100, 200, 200)).toBeNull();
  });

  it("picks the nearest edge in a corner-ish position on a wide rect", () => {
    // 幅800/高さ100の横長要素。左上寄りでも、上下の余白比率(y=10/100=10%)の方が
    // 左右の余白比率(x=50/800=6.25%)より小さくないので、xの近さ(6.25%<25%)が勝つ。
    expect(edgeFromPointer(50, 10, 800, 100)).toBe("left");
  });

  it("returns null when neither axis is within the 25% margin", () => {
    expect(edgeFromPointer(250, 250, 800, 800)).toBeNull();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm vitest run src/lib/paneEdge.test.ts`
Expected: `paneEdge.ts` が存在しないエラーで FAIL。

- [ ] **Step 3: Implement**

`frontend/src/lib/paneEdge.ts`:

```typescript
import type { Edge } from "../bindings/tauri.gen";

/// 要素内でのポインタ位置(offsetX/Y)から、ドロップ先のエッジ(Left/Right/Top/Bottom)を
/// 判定する。4辺までの距離のうち、幅/高さに対する比率(0〜0.5)が最小のものを採用する。
/// 最小の比率が0.25(25%)を超える=中央寄りすぎる場合はnull(ドロップ対象外=タブ統合等
/// 本Sliceの対象外の中央エリア)を返す。
const EDGE_MARGIN_RATIO = 0.25;

export function edgeFromPointer(offsetX: number, offsetY: number, width: number, height: number): Edge | null {
  if (width <= 0 || height <= 0) return null;
  const nx = Math.min(offsetX, width - offsetX) / width;
  const ny = Math.min(offsetY, height - offsetY) / height;
  if (nx > EDGE_MARGIN_RATIO && ny > EDGE_MARGIN_RATIO) return null;
  if (nx <= ny) {
    return offsetX < width / 2 ? "left" : "right";
  }
  return offsetY < height / 2 ? "top" : "bottom";
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm vitest run src/lib/paneEdge.test.ts`
Expected: 全テスト PASS。

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/paneEdge.ts frontend/src/lib/paneEdge.test.ts
git commit -m "feat: エッジ判定の純粋関数paneEdgeFromPointerを追加"
```

---

### Task 4: フロント — store: グループドラッグを `move_pane` ベースに置き換え

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`

**Interfaces:**
- Consumes: `commands.movePane(draggedGroupId, targetGroupId, edge)`(Task 2)、`Edge`型(`../bindings/tauri.gen`から)。
- Produces: `AppStore.dragOverEdgeTarget: { groupId: string; edge: Edge } | null`(`$state`)、`AppStore.dragOverPaneEdge(groupId: string, edge: Edge | null): void`(ハイライトのみ、木は変更しない)。
- 変更: 既存の `dragOverGroup(overId: string)` メソッドを削除する(ライブ並べ替えは廃止)。`endDragGroup()` の実装を、`dragOverEdgeTarget` を見て `movePane` を呼ぶ形に置き換える。`draggingGroupId`/`startDragGroup` は変更しない。

- [ ] **Step 1: Replace the group-drag section**

`frontend/src/lib/store.svelte.ts` の `// ---- グループの並べ替え / 幅 ----` セクション、`draggingGroupId`〜`endDragGroup` を以下で置き換える:

```typescript
  draggingGroupId = $state<string | null>(null);
  dragOverEdgeTarget = $state<{ groupId: string; edge: Edge } | null>(null);

  startDragGroup(id: string) {
    this.draggingGroupId = id;
    this.dragOverEdgeTarget = null;
  }
  /// groupId(ドロップ先候補)上でのエッジ判定結果をハイライト用に保持するだけ。
  /// 木構造はここでは変更しない(実際の移動はendDragGroupでmovePaneを1回呼ぶ)。
  /// 自分自身へのドロップ(groupId === draggingGroupId)は常に対象外。
  dragOverPaneEdge(groupId: string, edge: Edge | null) {
    if (!edge || groupId === this.draggingGroupId) {
      this.dragOverEdgeTarget = null;
      return;
    }
    this.dragOverEdgeTarget = { groupId, edge };
  }
  async endDragGroup() {
    const draggedId = this.draggingGroupId;
    const target = this.dragOverEdgeTarget;
    this.draggingGroupId = null;
    this.dragOverEdgeTarget = null;
    if (!draggedId || !target) return;
    try {
      await unwrap(commands.movePane(draggedId, target.groupId, target.edge));
      this.paneRoot = await unwrap(commands.loadPaneLayout());
    } catch (e) {
      this.#logFailure(e);
    }
  }
```

ファイル先頭の `import type { ... } from "../bindings/tauri.gen";` の型import一覧に `Edge` を追加する(`PaneNode` の隣に並べる):

```typescript
  PaneNode,
  Edge,
```

- [ ] **Step 2: Type-check**

Run: `cd frontend && pnpm check`
Expected: `Column.svelte` がまだ古い `app.dragOverGroup(...)` を呼んでいるためエラーになる(Task 5で解消する想定なのでこの時点でのエラーは許容し、次のStepで確認だけする)。

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/store.svelte.ts
git commit -m "feat: グループドラッグをmove_paneベースのエッジ判定に置き換え"
```

---

### Task 5: フロント — `Column.svelte` に4辺ドロップゾーンを追加

**Files:**
- Modify: `frontend/src/ui/Column.svelte`

**Interfaces:**
- Consumes: Task 3の `edgeFromPointer`、Task 4の `app.dragOverPaneEdge`/`app.dragOverEdgeTarget`/`app.endDragGroup`(呼び出し元は既存のまま、`ondragend={() => app.endDragGroup()}` は変更不要)。

- [ ] **Step 1: Update the drag-over handler and add the edge overlay**

`frontend/src/ui/Column.svelte` の先頭 import に追加:

```typescript
  import { edgeFromPointer } from "../lib/paneEdge";
```

`<section class="column-root ...">` の `ondragover` を以下に置き換える:

```svelte
  ondragover={(e) => {
    if (!app.draggingGroupId) return;
    e.preventDefault();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const edge = edgeFromPointer(e.clientX - rect.left, e.clientY - rect.top, rect.width, rect.height);
    app.dragOverPaneEdge(group.id, edge);
  }}
  ondragleave={(e) => {
    if (app.dragOverEdgeTarget?.groupId === group.id) app.dragOverPaneEdge(group.id, null);
  }}
```

`</section>` の直前(既存の幅リサイズハンドル `{#if !stretch && !group.auto}...{/if}` の直後)に、エッジハイライト用のオーバーレイを追加:

```svelte
  {#if app.draggingGroupId && app.draggingGroupId !== group.id && app.dragOverEdgeTarget?.groupId === group.id}
    {@const edge = app.dragOverEdgeTarget.edge}
    <div
      class="pointer-events-none absolute bg-[color-mix(in_srgb,var(--color-primary)_35%,transparent)]"
      style:left={edge === "right" ? "auto" : "0"}
      style:right={edge === "left" ? "auto" : "0"}
      style:top={edge === "bottom" ? "auto" : "0"}
      style:bottom={edge === "top" ? "auto" : "0"}
      style:width={edge === "left" || edge === "right" ? "35%" : "auto"}
      style:height={edge === "top" || edge === "bottom" ? "35%" : "auto"}
      style="z-index:6"
    ></div>
  {/if}
```

- [ ] **Step 2: Type-check**

Run: `cd frontend && pnpm check`
Expected: エラー無し。

- [ ] **Step 3: Manual verification**

Run: `cargo tauri dev`(プロジェクトルートで)

1. 3つ以上カラムがある状態で、あるカラムのグリップ(左端の縦ドット)をドラッグし、別のカラムの左端/右端に重ねる → その辺に半透明のハイライトが出ること。上端/下端に重ねても同様にハイライトが出ること。
2. 右端でドロップ → ドロップ先の右に新しいカラムとして挿入される(同一行内の並べ替えとして機能する)こと。
3. 下端でドロップ → ドロップ先の下に縦分割で挿入される(行をまたいだ移動ができる)こと。
4. 中央付近(どの辺からも25%以上離れた位置)でドロップ → 何も起きない(ハイライトも出ない)こと。
5. 自分自身の上にドラッグしてもハイライトが出ない(自己ドロップが無効)こと。

- [ ] **Step 4: Commit**

```bash
git add frontend/src/ui/Column.svelte
git commit -m "feat: Column.svelteに4辺ドロップゾーンを追加しペイン移動を可能にする"
```

---

### Task 6: フロント — 「右に分割」ボタン

**Files:**
- Modify: `frontend/src/ui/Column.svelte`
- Modify: `frontend/src/ui/Pane.svelte`
- Modify: `frontend/src/App.svelte`

**Interfaces:**
- Produces: `Column.svelte` に新規必須prop `onSplitRight: (groupId: string) => void`。`Pane.svelte` に新規必須prop `onSplitRight: (groupId: string) => void`(そのまま`Column`/自己再帰に伝播)。`App.svelte` に `splitRight(groupId: string)`(`app.splitPane(groupId, "row")` を呼ぶ、`splitDown` と同じ形)。

- [ ] **Step 1: `Column.svelte` に prop とメニュー項目を追加**

import一覧に `SquareSplitHorizontal` を追加(既存の `SquareSplitVertical` の隣):

```typescript
  import { X, GripVertical, MoreHorizontal, Plus, SquareSplitHorizontal, SquareSplitVertical, Settings } from "@lucide/svelte";
```

props定義に `onSplitRight` を追加:

```typescript
  let {
    group,
    onAddTab,
    onEditTab,
    onEditGroup,
    onSplitDown,
    onSplitRight,
    stretch = false,
  }: {
    group: GroupView;
    onAddTab: (groupId: string) => void;
    onEditTab: (tab: TabView) => void;
    onEditGroup: (groupId: string) => void;
    onSplitDown: (groupId: string) => void;
    onSplitRight: (groupId: string) => void;
    stretch?: boolean;
  } = $props();
```

「下に分割」メニュー項目の直前に追加:

```svelte
        <button
          type="button"
          role="menuitem"
          class="box-border flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted"
          onclick={() => pickMenuItem(() => onSplitRight(group.id))}
        >
          <SquareSplitHorizontal size={16} /> 右に分割
        </button>
```

- [ ] **Step 2: `Pane.svelte` に prop を追加して伝播**

props定義に `onSplitRight` を追加(`onSplitDown` の隣、他は変更なし):

```typescript
  let {
    node,
    onAddTab,
    onEditTab,
    onEditGroup,
    onSplitDown,
    onSplitRight,
    stretch = false,
  }: {
    node: PaneNode;
    onAddTab: (groupId: string) => void;
    onEditTab: (tab: TabView) => void;
    onEditGroup: (groupId: string) => void;
    onSplitDown: (groupId: string) => void;
    onSplitRight: (groupId: string) => void;
    stretch?: boolean;
  } = $props();
```

ファイル内の `<Column ... {onSplitDown} .../>` と `<Pane node={child.node} ... {onSplitDown} .../>`(自己再帰呼び出し、Row/Column両分岐の全箇所)に `{onSplitRight}` を追加する(`{onSplitDown}` が出てくる箇所すべてに並べて足すだけ)。

- [ ] **Step 3: `App.svelte` に `splitRight` を追加**

`splitDown` 関数の直後に追加:

```typescript
  async function splitRight(groupId: string) {
    const newGroupId = await app.splitPane(groupId, "row");
    if (!newGroupId) return;
    pendingSplitGroupId = newGroupId;
    openAddTab(newGroupId);
  }
```

`<Pane node={app.paneRoot} ... onSplitDown={splitDown} />` の呼び出しに `onSplitRight={splitRight}` を追加:

```svelte
        <Pane node={app.paneRoot} onAddTab={openAddTab} onEditTab={openEditTab} onEditGroup={openColumnSettings} onSplitDown={splitDown} onSplitRight={splitRight} />
```

- [ ] **Step 4: Type-check**

Run: `cd frontend && pnpm check`
Expected: エラー無し。

- [ ] **Step 5: Manual verification**

Run: `cargo tauri dev`

1. カラムのメニュー(⋯)を開く → 「右に分割」が「下に分割」の上に表示されること。
2. 「右に分割」をクリック → タブ追加モーダルが開き、追加すると元のカラムの右側に新しいカラムが並ぶこと。
3. キャンセルすると空カラムが消える(既存の`discardEmptyGroup`ロジックがそのまま効く)こと。

- [ ] **Step 6: Commit**

```bash
git add frontend/src/ui/Column.svelte frontend/src/ui/Pane.svelte frontend/src/App.svelte
git commit -m "feat: 右に分割ボタンを追加"
```

---

### Task 7: フロント — `Pane.svelte`: Row内ネストSplit子のドラッグリサイズ

**Files:**
- Modify: `frontend/src/ui/Pane.svelte`

**Interfaces:**
- Consumes: 既存の `app.resizePane(nodeId, size)`。

- [ ] **Step 1: Add a drag handle to non-leaf Row children**

`frontend/src/ui/Pane.svelte` の `<script>` に、Row分岐で使うドラッグ状態とハンドラを追加(`</script>`の直前):

```typescript
  // Row内のネストしたSplit子の幅(px)ドラッグリサイズ。Leaf子はColumn.svelte自身の
  // ハンドル(group.width)を使うのでここでは扱わない。
  let rowResizing = $state<{ nodeId: string; startX: number; startW: number } | null>(null);

  function onRowSplitResizeDown(e: PointerEvent, child: PaneChild) {
    rowResizing = { nodeId: child.node.id, startX: e.clientX, startW: child.size ?? 300 };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onRowSplitResizeMove(e: PointerEvent, child: PaneChild) {
    if (!rowResizing || rowResizing.nodeId !== child.node.id) return;
    child.size = Math.min(720, Math.max(220, rowResizing.startW + (e.clientX - rowResizing.startX)));
  }
  function onRowSplitResizeUp(child: PaneChild) {
    if (!rowResizing || rowResizing.nodeId !== child.node.id) return;
    rowResizing = null;
    app.resizePane(child.node.id, child.size ?? 300);
  }
```

`PaneChild` 型のimportを追加(ファイル先頭のimportに `PaneNode` と並べて):

```typescript
  import type { PaneChild, PaneNode } from "../bindings/tauri.gen";
```

Row分岐(`node.direction === "row"`)の、非Leaf子(`{:else}`側、既存の「ネストしたSplit」を描画しているdiv)にハンドルを追加する。既存の該当ブロックを以下に置き換える:

```svelte
      {:else}
        <!-- ネストしたSplit(例: 下に分割された塊)にはColumn.svelteに相当する幅指定元が
             無いため、PaneChild.size/autoをそのままflex指定に使う。 -->
        <div
          class="relative flex flex-col h-full min-h-0 min-w-0"
          style={child.auto ? "flex:1 1 0;min-width:220px" : `flex:0 0 ${child.size}px`}
        >
          <Pane node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} {onSplitRight} />
          {#if !child.auto}
            <div
              class="absolute right-[-3px] top-0 h-full w-1.5 cursor-col-resize hover:bg-[color-mix(in_srgb,var(--color-primary)_40%,transparent)]"
              style="z-index:5"
              onpointerdown={(e) => onRowSplitResizeDown(e, child)}
              onpointermove={(e) => onRowSplitResizeMove(e, child)}
              onpointerup={() => onRowSplitResizeUp(child)}
              role="separator"
              aria-label="幅を変更"
            ></div>
          {/if}
        </div>
      {/if}
```

- [ ] **Step 2: Type-check**

Run: `cd frontend && pnpm check`
Expected: エラー無し。

- [ ] **Step 3: Manual verification**

Run: `cargo tauri dev`

1. カラムAを「下に分割」してA/A'の縦分割ブロックを作る(この分割ブロック全体がRowの中の1子になる)。
2. その分割ブロックの右端にマウスを合わせてドラッグ → 幅がドラッグに追従して変わること。
3. ドラッグを離す → 幅が確定し、アプリを再起動しても維持されること。
4. 「カラム設定」の「分割ブロック全体の幅」の数値入力と併用しても矛盾なく動くこと(どちらで変更しても同じ`resizePane`を呼ぶため一貫する)。

- [ ] **Step 4: Commit**

```bash
git add frontend/src/ui/Pane.svelte
git commit -m "feat: Row内ネストSplit子の幅をドラッグでリサイズできるようにする"
```

---

### Task 8: フロント — `Pane.svelte`: Column分割境界のペアドラッグリサイズ

**Files:**
- Modify: `frontend/src/ui/Pane.svelte`

**Interfaces:**
- Consumes: 既存の `app.resizePane(nodeId, size)`。

- [ ] **Step 1: Add paired drag handles between Column split children**

`frontend/src/ui/Pane.svelte` の `<script>` に、Task 7のRow用状態の直後に追加:

```typescript
  // Column分割の境界(children[i]とchildren[i+1]の間)のペアドラッグリサイズ。
  // 2子の合計ウェイトを保ったまま、ポインタのY移動量をpx→ウェイトに変換して増減させる。
  const MIN_COLUMN_PANE_PX = 60;
  let colResizing = $state<{
    a: PaneChild;
    b: PaneChild;
    startY: number;
    startHeightA: number;
    startHeightB: number;
    startSizeA: number;
    startSizeB: number;
  } | null>(null);

  function onColSplitResizeDown(e: PointerEvent, a: PaneChild, b: PaneChild) {
    const boundary = e.currentTarget as HTMLElement;
    const elA = boundary.previousElementSibling as HTMLElement | null;
    const elB = boundary.nextElementSibling as HTMLElement | null;
    if (!elA || !elB) return;
    colResizing = {
      a,
      b,
      startY: e.clientY,
      startHeightA: elA.getBoundingClientRect().height,
      startHeightB: elB.getBoundingClientRect().height,
      startSizeA: a.size ?? 50,
      startSizeB: b.size ?? 50,
    };
    boundary.setPointerCapture(e.pointerId);
  }
  function onColSplitResizeMove(e: PointerEvent) {
    if (!colResizing) return;
    const { a, b, startY, startHeightA, startHeightB, startSizeA, startSizeB } = colResizing;
    const totalPx = startHeightA + startHeightB;
    const totalWeight = startSizeA + startSizeB;
    if (totalPx <= 0 || totalWeight <= 0) return;
    const deltaY = e.clientY - startY;
    const newHeightA = Math.min(totalPx - MIN_COLUMN_PANE_PX, Math.max(MIN_COLUMN_PANE_PX, startHeightA + deltaY));
    const weightPerPx = totalWeight / totalPx;
    a.size = newHeightA * weightPerPx;
    b.size = totalWeight - a.size;
  }
  function onColSplitResizeUp() {
    if (!colResizing) return;
    const { a, b } = colResizing;
    colResizing = null;
    app.resizePane(a.node.id, a.size ?? 50);
    app.resizePane(b.node.id, b.size ?? 50);
  }
```

Column分岐(`node.direction === "column"`、design docでは「Column方向」、既存実装は最後の`{:else}`ブロック)の `{#each}` を、境界ハンドル付きに以下で置き換える:

```svelte
{:else}
  <div class="flex flex-col flex-auto h-full min-h-0">
    {#each node.children as child, i (child.node.id)}
      <div class="relative flex flex-col min-h-0 min-w-0" style={child.auto ? "flex:1 1 0" : `flex:0 0 ${child.size}%`}>
        <Pane node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} {onSplitRight} stretch={true} />
      </div>
      {#if i < node.children.length - 1 && !child.auto && !node.children[i + 1].auto}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="h-1.5 flex-none cursor-row-resize hover:bg-[color-mix(in_srgb,var(--color-primary)_40%,transparent)]"
          onpointerdown={(e) => onColSplitResizeDown(e, child, node.children[i + 1])}
          onpointermove={onColSplitResizeMove}
          onpointerup={onColSplitResizeUp}
          role="separator"
          aria-label="高さを変更"
        ></div>
      {/if}
    {/each}
  </div>
{/if}
```

(どちらかがauto=trueの境界ではドラッグハンドルを出さない: autoはflexboxが自動的に残り領域を割り付ける仕組みなので、隣接ペアのウェイトだけを弄っても見た目に反映されず紛らわしいため。)

- [ ] **Step 2: Type-check**

Run: `cd frontend && pnpm check`
Expected: エラー無し。

- [ ] **Step 3: Manual verification**

Run: `cargo tauri dev`

1. カラムAを「下に分割」してA(上)/A'(下)を作る(両方とも既定でauto=false、固定%)。
2. AとA'の境界にマウスを合わせてドラッグ → 上下の高さが連動して(合計を保ったまま)変わること。
3. ドラッグを離す → 高さが確定し、アプリを再起動しても維持されること。
4. 「カラム設定」の「高さ」を「自動調整」にした状態で境界を見る → ドラッグハンドルが出ない(または触っても何も起きない)こと。
5. 3つ以上の縦分割(A/A'/A'')がある状態で、A・A'間の境界をドラッグしても A'' の高さは変化しないこと(ペア外の兄弟は不変)。

- [ ] **Step 4: Commit**

```bash
git add frontend/src/ui/Pane.svelte
git commit -m "feat: Column分割境界のドラッグリサイズを追加"
```
