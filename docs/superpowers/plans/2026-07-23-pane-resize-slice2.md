# カラムの縦分割（ペイン化） Slice 2: 高さの数値リサイズ 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Slice 1(「下に分割」ボタン)で作った縦分割ペインの高さを、既存の「カラム設定」モーダル(グリップをダブルクリックで開く)から数値(%)で調整できるようにする。

**Architecture:** Rust側に、ペイン木の任意ノード(Leaf/Split問わず)の `size` を1つだけ上書きする `PaneNode::set_size` を追加し、新設の `resize_pane` コマンドで呼び出す。フロントは `app.paneRoot` を辿って「このグループの Leaf は Column分割の直下にいるか」を判定するヘルパーを追加し、`ColumnSettings.svelte` にその場合だけ「高さ（%）」の数値入力を表示する。

**Tech Stack:** Rust(Tauri) + Svelte 5(runes) + tauri-specta。

## Global Constraints

- 仕様: `docs/superpowers/specs/2026-07-22-pane-split-design.md`の「### UI (`ui/ColumnSettings.svelte` 拡張)」節のうち **Column分割の場合(%入力)のみ** を実装対象とする。Row(px)側のUIは既存のまま一切変更しない(Slice 1で `ColumnGroup.width`/`auto` は廃止していないため、Row側は今まで通り機能している)。
- ドラッグでの境界リサイズ、`set_pane_auto`、右分割ボタン、`move_pane`(ドラッグ移動)は本Sliceの対象外(それぞれ別Sliceとして後日計画する)。
- %入力は5〜95の範囲にclampする(100%ちょうどにすると兄弟の高さが0になるため)。
- 新しいUUIDは使わない(このSliceでは新規ノードを作らない)。
- Rustのテストは `cargo test`(src-tauriディレクトリ)、フロントは `pnpm check`(frontendディレクトリ)で確認する。

---

### Task 1: `PaneNode::id`/`PaneNode::set_size`

**Files:**
- Modify: `src-tauri/src/domain/pane.rs`

**Interfaces:**
- Produces: `PaneNode::id(&self) -> &str`(Leaf/Splitどちらもidを持つので共通ヘルパー)、`PaneNode::set_size(&mut self, node_id: &str, size: f32) -> bool`(木の中からnode_idに一致する子(Leaf/Split問わず)を探し、その子を親から見た`PaneChild.size`を上書きする。見つかって更新できたらtrue。ルート自身のnode_idを指定した場合は親が無くsizeを保持する場所が無いためfalse)。

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/domain/pane.rs` の既存 `#[cfg(test)] mod tests` ブロック内、`insert_sibling_returns_false_when_reference_not_found` テストの直後に追加:

```rust
    #[test]
    fn id_returns_leaf_and_split_ids() {
        let leaf = PaneNode::Leaf { id: "l1".into(), group_id: "g1".into() };
        assert_eq!(leaf.id(), "l1");
        let split = PaneNode::Split { id: "s1".into(), direction: SplitDirection::Row, children: vec![] };
        assert_eq!(split.id(), "s1");
    }

    #[test]
    fn set_size_updates_direct_child_leaf() {
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Column,
            children: vec![
                PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 1.0, auto: false },
                PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 1.0, auto: false },
            ],
        };
        assert!(root.set_size("la", 3.0));
        let PaneNode::Split { children, .. } = &root else { panic!("expected Split") };
        assert_eq!(children[0].size, 3.0);
        assert_eq!(children[1].size, 1.0); // 兄弟は変化しない
    }

    #[test]
    fn set_size_updates_nested_split_by_its_own_id() {
        // root(Row)[ Leaf(a), Split(Column, id="inner")[...] ] の inner 自身のsizeを更新できる
        let mut root = PaneNode::Split {
            id: "root".into(),
            direction: SplitDirection::Row,
            children: vec![
                PaneChild { node: PaneNode::Leaf { id: "la".into(), group_id: "a".into() }, size: 300.0, auto: false },
                PaneChild {
                    node: PaneNode::Split {
                        id: "inner".into(),
                        direction: SplitDirection::Column,
                        children: vec![
                            PaneChild { node: PaneNode::Leaf { id: "lb".into(), group_id: "b".into() }, size: 1.0, auto: false },
                            PaneChild { node: PaneNode::Leaf { id: "lc".into(), group_id: "c".into() }, size: 1.0, auto: false },
                        ],
                    },
                    size: 300.0,
                    auto: false,
                },
            ],
        };
        assert!(root.set_size("inner", 450.0));
        let PaneNode::Split { children, .. } = &root else { panic!("expected Split") };
        assert_eq!(children[1].size, 450.0);
    }

    #[test]
    fn set_size_returns_false_when_node_id_not_found() {
        let mut root = PaneNode::new_leaf("a");
        assert!(!root.set_size("nope", 1.0));
    }

    #[test]
    fn set_size_returns_false_for_root_itself() {
        // ルート自身のidを指定しても、sizeを保持する親が無いのでfalse。
        let mut root = PaneNode::Split { id: "root".into(), direction: SplitDirection::Row, children: vec![] };
        assert!(!root.set_size("root", 1.0));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test domain::pane:: -- --nocapture`
Expected: `id`/`set_size` が存在しないコンパイルエラーで FAIL。

- [ ] **Step 3: Implement**

`impl PaneNode` ブロック内、`new_leaf` の直後に追加:

```rust
    pub fn id(&self) -> &str {
        match self {
            PaneNode::Leaf { id, .. } => id,
            PaneNode::Split { id, .. } => id,
        }
    }
```

`remove_group` の直後(implブロックの末尾)に追加:

```rust
    /// node_id(Leaf/Splitどちらのidでも可)を持つノードを親から見たsizeを上書きする。
    /// 見つかって更新できたらtrue。node_idがルート自身を指す場合はsizeを保持する
    /// 親が無いためfalse(呼び出し元はエラー扱いにしてよい)。
    pub fn set_size(&mut self, node_id: &str, size: f32) -> bool {
        let PaneNode::Split { children, .. } = self else {
            return false;
        };
        for child in children.iter_mut() {
            if child.node.id() == node_id {
                child.size = size;
                return true;
            }
            if child.node.set_size(node_id, size) {
                return true;
            }
        }
        false
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test domain::pane:: -- --nocapture`
Expected: 全テスト PASS。

- [ ] **Step 5: Run the full Rust test suite**

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS(既存テストも壊れていないこと)。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/domain/pane.rs
git commit -m "feat: PaneNode::id/set_sizeを追加(ペイン高さの数値リサイズ用)"
```

---

### Task 2: Tauriコマンド `resize_pane`

**Files:**
- Modify: `src-tauri/src/commands/column.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Task 1の `PaneNode::set_size`、既存の `state.settings.load_pane_layout()`/`save_pane_layout()`。
- Produces: `#[tauri::command] async fn resize_pane(state, node_id: String, size: f32) -> Result<()>`。

- [ ] **Step 1: Implement the command**

`src-tauri/src/commands/column.rs` の `split_pane` の直後に追加:

```rust
/// ペインノード(Leaf/Splitどちらのidでも可)のsizeを更新する(Column分割の高さ調整用)。
#[tauri::command]
#[specta::specta]
pub async fn resize_pane(state: State<'_, AppState>, node_id: String, size: f32) -> Result<()> {
    let mut root = state.settings.load_pane_layout()?;
    if !root.set_size(&node_id, size) {
        return Err(Error::Invalid(format!("unknown pane node: {node_id}")));
    }
    state.settings.save_pane_layout(&root)
}
```

- [ ] **Step 2: Register in `specta_builder()`**

`src-tauri/src/lib.rs` の `commands::column::split_pane,` の直後に追加:

```rust
            commands::column::resize_pane,
```

- [ ] **Step 3: Run `cargo test` to regenerate TS bindings and verify compilation**

Run: `cd src-tauri && cargo test generates_frontend_bindings`
Expected: PASS。`frontend/src/bindings/tauri.gen.ts` に `resizePane` が生成される。

- [ ] **Step 4: Run the full Rust test suite**

Run: `cd src-tauri && cargo test`
Expected: 全テスト PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/column.rs src-tauri/src/lib.rs frontend/src/bindings/tauri.gen.ts
git commit -m "feat: resize_paneコマンドを追加"
```

---

### Task 3: Column分割内Leafの幅を「常に利用可能幅いっぱいに広げる」ように修正

**背景**: Slice 1の `split_pane` は新規グループを `auto: false, width: 300` で作る。Column分割の中に置かれた Leaf は、実際に確保されている幅(例えば600px)とは無関係に、`Column.svelte` 自身の `group.width`/`auto` に基づく固定px幅で描画されてしまい、その分割の中身が変な幅で表示される。Column分割内のLeafは常にその親スロットの幅いっぱいに広げるべきで、`ColumnGroup.width`/`auto` はそもそも意味を持たない(Row内でのみ意味がある)。

**Files:**
- Modify: `frontend/src/ui/Pane.svelte`
- Modify: `frontend/src/ui/Column.svelte`

**Interfaces:**
- Produces: `Column.svelte` に新規 optional prop `stretch?: boolean`(デフォルト `false`)。`true` のとき `group.width`/`group.auto` を無視して常に `flex:1 1 0;min-width:0` で描画し、幅リサイズハンドルも表示しない。
- Produces: `Pane.svelte` に新規 optional prop `stretch?: boolean`(デフォルト `false`)。Column分割(`.col`)の子を描画する際は、子が Leaf/Split どちらであっても常に `stretch={true}` を再帰呼び出しに渡す(Split側の分岐は無視するので実害無し。Leaf側は下記の通りColumnへ伝える)。Row分割(`.row`)・ルート直下のLeaf分岐は今まで通り `stretch` を渡さない(既定 `false` のまま)。

- [ ] **Step 1: Update `Column.svelte`**

`frontend/src/ui/Column.svelte` の props定義を以下に変更:

```typescript
  let {
    group,
    onAddTab,
    onEditTab,
    onEditGroup,
    onSplitDown,
    stretch = false,
  }: {
    group: GroupView;
    onAddTab: (groupId: string) => void;
    onEditTab: (tab: TabView) => void;
    onEditGroup: (groupId: string) => void;
    onSplitDown: (groupId: string) => void;
    stretch?: boolean;
  } = $props();
```

`<section class="column" style={...} ...>` の `style` 属性を以下に変更:

```svelte
  style={stretch ? "flex:1 1 0;min-width:0" : group.auto ? "flex:1 1 0;min-width:220px" : `width:${group.width}px`}
```

幅リサイズハンドルの表示条件 `{#if !group.auto}` を `{#if !stretch && !group.auto}` に変更(stretch時は幅がColumnGroup.widthと無関係になるため、ドラッグしても見た目に反映されず紛らわしい)。

- [ ] **Step 2: Update `Pane.svelte`**

`frontend/src/ui/Pane.svelte` の props定義に `stretch` を追加:

```typescript
  let {
    node,
    onAddTab,
    onEditTab,
    onEditGroup,
    onSplitDown,
    stretch = false,
  }: {
    node: PaneNode;
    onAddTab: (groupId: string) => void;
    onEditTab: (tab: TabView) => void;
    onEditGroup: (groupId: string) => void;
    onSplitDown: (groupId: string) => void;
    stretch?: boolean;
  } = $props();
```

Leaf分岐の `<Column>` 呼び出しに `{stretch}` を追加:

```svelte
{#if node.type === "leaf"}
  {@const group = app.groups.find((g) => g.id === node.groupId)}
  {#if group}
    <Column {group} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} {stretch} />
  {/if}
```

Column分割(`.col`)の子を描画している `{#each node.children as child (child.node.id)}` ブロック内の `<svelte:self>` 呼び出しに `stretch={true}` を追加(Leaf/Split問わず常に渡す):

```svelte
{:else}
  <div class="col">
    {#each node.children as child (child.node.id)}
      <div class="col-item" style={`flex:${child.size} 1 0`}>
        <svelte:self node={child.node} {onAddTab} {onEditTab} {onEditGroup} {onSplitDown} stretch={true} />
      </div>
    {/each}
  </div>
{/if}
```

Row分岐(`.row`)内の2箇所(Leaf直描画、row-item経由のSplit)は変更しない(`stretch` を渡さないので既定 `false` のまま)。

- [ ] **Step 3: Type-check**

Run: `cd frontend && pnpm check`
Expected: エラー無し。

- [ ] **Step 4: Manual verification**

Run: `cargo tauri dev`

1. まだ縦分割していない普通のカラムの幅(px固定/自動)が今まで通り機能すること(回帰していないこと)。
2. 「下に分割」で作った上下のペインが、どちらも親スロットの幅いっぱいに正しく広がって表示されること(300px固定で狭くならないこと)。

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ui/Pane.svelte frontend/src/ui/Column.svelte
git commit -m "fix: Column分割内のLeafが利用可能幅いっぱいに広がるよう修正"
```

---

### Task 4: フロント — `paneColumnContext`/`resizePane` と `ColumnSettings.svelte` の%入力

**Files:**
- Modify: `frontend/src/lib/store.svelte.ts`
- Modify: `frontend/src/ui/ColumnSettings.svelte`

**Interfaces:**
- Consumes: `commands.resizePane(nodeId, size)`(Task 2で生成)、既存の `app.paneRoot`。
- Produces: `AppStore.paneColumnContext(groupId: string): { nodeId: string; size: number; othersSum: number } | null`(そのグループのLeafがColumn分割の直下に無ければnull)、`AppStore.resizePane(nodeId: string, size: number): Promise<void>`。

- [ ] **Step 1: Add `paneColumnContext`/`resizePane` to the store**

`frontend/src/lib/store.svelte.ts` の `discardEmptyGroup` メソッドの直後に追加:

```typescript
  /// groupIdのLeafがColumn分割の直下に居れば、そのノードid・現在のsize・
  /// 他の兄弟のsize合計(othersSum)を返す。Row分割の直下(または見つからない)ならnull。
  paneColumnContext(groupId: string): { nodeId: string; size: number; othersSum: number } | null {
    const search = (node: PaneNode): { nodeId: string; size: number; othersSum: number } | null => {
      if (node.type !== "split") return null;
      if (node.direction === "column") {
        const idx = node.children.findIndex(
          (c) => c.node.type === "leaf" && c.node.groupId === groupId,
        );
        if (idx >= 0) {
          const size = node.children[idx].size ?? 1;
          const othersSum = node.children.reduce(
            (sum, c, i) => (i === idx ? sum : sum + (c.size ?? 1)),
            0,
          );
          return { nodeId: node.children[idx].node.id, size, othersSum };
        }
      }
      for (const c of node.children) {
        const found = search(c.node);
        if (found) return found;
      }
      return null;
    };
    return search(this.paneRoot);
  }

  async resizePane(nodeId: string, size: number) {
    try {
      await unwrap(commands.resizePane(nodeId, size));
      this.paneRoot = await unwrap(commands.loadPaneLayout());
    } catch (e) {
      this.#fail(e);
    }
  }
```

型importに `PaneNode` が既に含まれている(Task 4のSlice 1で追加済み)ため追加のimportは不要。

- [ ] **Step 2: Add the height (%) input to `ColumnSettings.svelte`**

`frontend/src/ui/ColumnSettings.svelte` の `<script>` 部分、既存の `setWidth` 関数の直後に追加:

```typescript
  const paneCtx = $derived(groupId ? app.paneColumnContext(groupId) : null);
  const heightPercent = $derived(
    paneCtx ? Math.round((paneCtx.size / (paneCtx.size + paneCtx.othersSum)) * 100) : 0,
  );

  function setHeightPercent(p: number) {
    if (!paneCtx || !Number.isFinite(p)) return;
    const clamped = Math.min(95, Math.max(5, Math.round(p)));
    const size = (paneCtx.othersSum * clamped) / (100 - clamped);
    app.resizePane(paneCtx.nodeId, size);
  }
```

マークアップ部分、`{#if group}` の中身を以下に置き換える(Column分割内のLeafは幅が意味を持たない=Task 3で常にstretchするため、幅UIは隠して高さUIだけ出す):

```svelte
    {#if group}
      {#if !paneCtx}
        <div class="field">
          <span>幅</span>
          <label class="check-row">
            <input type="radio" name="width-mode" checked={!group.auto} onchange={() => setAuto(false)} /> 固定（ドラッグで調整）
          </label>
          <label class="check-row">
            <input type="radio" name="width-mode" checked={group.auto} onchange={() => setAuto(true)} /> 自動調整（ウィンドウ幅に合わせて均等割付）
          </label>
        </div>

        {#if !group.auto}
          <label class="field num-field">
            <span>幅（px、220〜720）</span>
            <input
              type="number"
              min="220"
              max="720"
              value={group.width}
              onchange={(e) => setWidth(Number((e.currentTarget as HTMLInputElement).value))}
            />
          </label>
        {/if}
      {:else}
        <label class="field num-field">
          <span>高さ（%、5〜95）</span>
          <input
            type="number"
            min="5"
            max="95"
            value={heightPercent}
            onchange={(e) => setHeightPercent(Number((e.currentTarget as HTMLInputElement).value))}
          />
        </label>
      {/if}
    {/if}
```

既存の `{#if group}...{/if}` ブロック全体(幅のラジオ+px入力)をこの内容で置き換える(元のインデント・閉じタグ位置は既存ファイルを参照して合わせること)。

- [ ] **Step 3: Type-check**

Run: `cd frontend && pnpm check`
Expected: エラー無し(既存のエラーが無い状態から変化しないこと)。

- [ ] **Step 4: Manual verification**

Run: `cargo tauri dev`(プロジェクトルートで)

1. 「下に分割」で縦分割ペインを作る。
2. 上(または下)のペインのグリップをダブルクリックして「カラム設定」を開く → 幅の欄は出ず、「高さ（%）」の入力欄だけが表示されること。
3. 値を変更 → 反映されて即座に高さが変わること。
4. アプリを再起動 → 高さが維持されていること。
5. Row直下(縦分割していない普通のカラム)でカラム設定を開くと、今まで通り幅の欄(固定/自動+px)だけが出て、高さ欄は出ないこと。

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/store.svelte.ts frontend/src/ui/ColumnSettings.svelte
git commit -m "feat: カラム設定に縦分割ペインの高さ(%)入力を追加"
```
