# タブ切替時の意図しないスクロール修正（Issue #363）

## 背景・問題

タブを切り替えて元のタブに戻ると、そのタブで最後に（矢印キー操作等で）選択していたノートの位置までスクロールしてしまう。原則として常に最新のノートを見たいユーザー体験に反するため、この自動スクロールは望ましくない。

## 原因

`frontend/src/ui/NoteCard.svelte` の `$effect`（270行目付近）が `selected && el` の場合に無条件で `el.scrollIntoView({ block: "nearest" })` を呼んでいる。

デスクトップUIでは `frontend/src/ui/Column.svelte` がアクティブタブを `{#each slots as slot (slot.tab.id)}` でキー管理しており、非アクティブになったタブのDOMサブツリーは破棄される。タブに戻ると `NoteCard` が再マウントされ、既に `tab.selectedNoteId`（矢印キー選択状態）が残っていれば `selected=true` かつ `el` が新規バインドされることで effect が発火し、意図せず選択位置までスクロールする。

## 修正方針

`NoteCard` のマウント直後（`selected` が真の状態での初回 effect 実行）はスクロールをスキップし、マウント後に `selected` が新たに真になった場合（同一マウント中の矢印キーでの選択移動など）にのみ `scrollIntoView` する。

```ts
let scrolledOnce = false;
$effect(() => {
  if (selected && el) {
    if (scrolledOnce) el.scrollIntoView({ block: "nearest" });
    scrolledOnce = true;
  } else if (!selected) {
    scrolledOnce = false;
  }
});
```

`scrolledOnce` はコンポーネントインスタンスのローカル変数のため、タブ切替による再マウントのたびに `false` にリセットされ、再マウント直後の1回だけスクロールをスキップする。同一タブ内で矢印キー選択移動が起きた場合は既存の `NoteCard` インスタンスが生き続けるため、従来通りスクロール追従が働く。

## テスト

`frontend/src/ui/NoteCard.test.ts` に以下を追加する（`el.scrollIntoView` をスタブして呼び出し有無を確認）：

1. `selected=true` で初回マウントした場合、`scrollIntoView` が呼ばれないこと。
2. マウント後に `selected` が `false → true` に変化した場合、`scrollIntoView` が呼ばれること。

## スコープ外

- スクロール位置を記憶するオプション（Issueに記載あり）は別Issueとして切り出す。本設計・実装の対象外。
