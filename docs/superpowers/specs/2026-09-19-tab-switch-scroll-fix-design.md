# タブ切替時の意図しないスクロール修正（Issue #363）

## 背景・問題

タブを切り替えて元のタブに戻ると、そのタブで最後に（矢印キー操作等で）選択していたノートの位置までスクロールしてしまう。原則として常に最新のノートを見たいユーザー体験に反するため、この自動スクロールは望ましくない。

## 原因

`frontend/src/ui/NoteCard.svelte` の `$effect`（270行目付近、コメント「キーボード選択中はスクロールで見える位置へ」）が `selected && el` の場合に無条件で `el.scrollIntoView({ block: "nearest" })` を呼んでいる。本来これは矢印キーでの選択移動専用の挙動だが、`selected` が真になる経路はそれ以外にもあり、タブが（再）表示されただけで発火してしまう。

- **デスクトップUI**: `frontend/src/ui/Column.svelte` がアクティブタブを `{#each slots as slot (slot.tab.id)}` でキー管理しており、非アクティブになったタブのDOMサブツリーは破棄される。タブに戻ると `NoteCard` が再マウントされ、既に `tab.selectedNoteId`（前回の矢印キー選択状態）が残っていれば `selected=true` かつ `el` が新規バインドされることで effect が発火する。
- **モバイルUI**（`computeTabSlots`、Issue #296のScroll Snap方式）: prev/active/nextの最大3タブを同時にDOM上に保持し、隣接タブへのスワイプでは `NoteCard` は再マウントされず、`role` prop が `"prev"`/`"next"` → `"active"` に変わるだけ。`selected` は `role === "active" && note.id === tab.selectedNoteId` で決まるため、role変化だけで `selected` が `false→true` になり、既存インスタンスの effect が発火する。「初回マウントかどうか」で判定する方式ではこのケースを検知できない（マウント自体は既に完了しているため）。

## 修正方針

「`NoteCard` の再マウント/再表示」ではなく、「矢印キーによる明示的な選択移動」だけをスクロールのトリガーにする。ストア側に世代カウンタを持たせ、矢印キー操作の経路だけでインクリメントする。

`frontend/src/lib/store.svelte.ts`:

```ts
// TabView に追加
selectionMoveSeq: number; // 初期値 0

// #moveSelection（矢印キー = note.next / note.prev）の中でのみインクリメント
#moveSelection(delta: number) {
  const t = this.#focusedTab();
  if (!t || t.kind.type === "notifications" || t.notes.length === 0) return;
  const cur = t.notes.findIndex((n) => n.id === t.selectedNoteId);
  let next = cur < 0 ? 0 : cur + delta;
  next = Math.max(0, Math.min(t.notes.length - 1, next));
  t.selectedNoteId = t.notes[next].id;
  t.selectionMoveSeq++;
}
```

`selectNote`（クリック/タップ選択）では `selectionMoveSeq` をインクリメントしない。クリックした要素は操作時点で画面内に見えているはずで、スクロール補助は不要かつ、部分的にしか見えていない場合にクリック直後へ意図しない微スクロールが起きるのを避けるため。

`Column.svelte` の `tabBody` snippet から `NoteCard` へ `selectionMoveSeq={tab.selectionMoveSeq}` を渡す。

`frontend/src/ui/NoteCard.svelte`:

```ts
let { selected = false, selectionMoveSeq = 0, /* ...既存props */ } = $props();

let el = $state<HTMLElement | null>(null);
let lastSeenSeq = selectionMoveSeq; // マウント時点の値を初期値にして、以後の変化だけを検知する
$effect(() => {
  if (selected && el && selectionMoveSeq !== lastSeenSeq) {
    el.scrollIntoView({ block: "nearest" });
  }
  lastSeenSeq = selectionMoveSeq;
});
```

マウント時（デスクトップの再マウント、モバイルのrole変化どちらでも）は `lastSeenSeq` の初期値が現在の `selectionMoveSeq` と一致するためスクロールしない。マウント後に矢印キー操作で `selectionMoveSeq` が変化した場合のみ、`selected` な `NoteCard` がスクロールする。

## テスト

`frontend/src/ui/NoteCard.test.ts` に以下を追加する（`el.scrollIntoView` をスタブして呼び出し有無を確認）：

1. `selected=true` で初回マウントした場合（`selectionMoveSeq` は任意の固定値）、`scrollIntoView` が呼ばれないこと。
2. マウント後に `selectionMoveSeq` が変化した場合、`scrollIntoView` が呼ばれること。
3. マウント後に `selectionMoveSeq` が変化せず `selected` だけが `false→true` になった場合（role変化を模した再現）、`scrollIntoView` が呼ばれないこと。

`frontend/src/lib/store.svelte.test.ts` に以下を追加する：

4. `runKeyAction("note.next"/"note.prev")` で `selectionMoveSeq` がインクリメントされること。
5. `selectNote` では `selectionMoveSeq` が変化しないこと。

## スコープ外

- スクロール位置を記憶するオプション（Issueに記載あり）は別Issueとして切り出す。本設計・実装の対象外。
