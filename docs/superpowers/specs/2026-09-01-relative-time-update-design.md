# 相対時刻表示の自動更新（Issue #256）

## 背景・課題

`frontend/src/lib/time.ts` の `relativeTime(epochSec)` は「3秒前」のような相対時刻文字列をその場で計算する純粋関数だが、呼び出し側（`NoteCard.svelte` / `NotificationCard.svelte`）はSvelteのテンプレート内で直接呼んでいるだけで、時間経過そのものをトリガーにした再評価が起きない。そのため、他の要因（新規ノート受信によるリスト更新など）で再描画がかからない限り、表示は投稿直後の値のまま止まって見える。

## 対象範囲

- `frontend/src/ui/NoteCard.svelte`
- `frontend/src/ui/NotificationCard.svelte`

`relativeTime` を呼んでいるのはこの2箇所のみ。`relativeTime()` 自体のフォーマットロジックはスコープ外（変更しない）。

## 設計

### 共有tickストア

`AppStore`（`frontend/src/lib/store.svelte.ts`）に `now` を追加する。

```ts
now = $state(Date.now());
#clockTimer: ReturnType<typeof setInterval> | null = null;
```

`boot()` で既存の `#statsTimer` 等と同じパターンで起動する:

```ts
if (this.#clockTimer !== null) clearInterval(this.#clockTimer);
this.#clockTimer = setInterval(() => (this.now = Date.now()), 5_000);
```

`teardown()` で解除する（dev HMR時の多重登録防止も既存の仕組みをそのまま流用できる）:

```ts
if (this.#clockTimer !== null) {
  clearInterval(this.#clockTimer);
  this.#clockTimer = null;
}
```

更新間隔は **5秒**。60秒未満は秒単位（`Ns`）表示のため最大5秒のズレが生じるが、体感上問題ないレベルと判断。カラムには数百件のNoteCardが同時表示されうるため、カードごとにタイマーを持たせず、アプリ全体で共有する1本のタイマーに統一する。

### コンポーネント側

`NoteCard.svelte` / `NotificationCard.svelte` で、テンプレートに直書きしていた `relativeTime(...)` 呼び出しを `$derived.by` に置き換え、`app.now` を依存として読ませる:

```ts
const displayTime = $derived.by(() => {
  app.now; // 依存関係のトリガーとして読む
  return relativeTime(inner.createdAt); // NotificationCardは n.createdAt
});
```

テンプレート側は `{relativeTime(inner.createdAt)}` → `{displayTime}` に置き換える。

### スケーラビリティ（表示から外れたノートの扱い）

`Column.svelte` は `{#each activeTab.notes as note}` でアクティブタブのノートのみをレンダリングしており、非アクティブタブのノートはそもそもDOMに存在しない。さらに `store.svelte.ts` には既に

```ts
const MAX_NOTES = 300; // タブあたり DOM に保持する上限（仮想化-lite）
```

という上限があり、タブごとの保持件数は300件に切り詰められる（Issue #239の経緯により、ライブ配信の先頭側はこの上限で切り詰めるが、バックフィル取得時は末尾側を切り詰めない実装になっている——詳細は該当コード参照）。

Svelteの `$derived`/エフェクトは、そのコンポーネントが実際にマウントされている間だけ `app.now` を購読する。ノートがスクロールアウト・タブ切り替え・削除等でDOMから外れてコンポーネントがunmountされると、そのeffectはSvelteが自動的に破棄し、以降 `app.now` の更新を受け取らなくなる。したがって「一度表示したノートが時間経過で表示されなくなる」ケースでも、購読が残り続けて蓄積するようなリークは発生しない。5秒tickごとに再計算されるNoteCardの数は「現在DOMにマウントされているカード数」（グループ数 × 最大300件程度）に自然に収まる。

## テスト

- `time.ts` 自体の純体変換ロジックは既存テストでカバー済み（変更なし）。
- 今回の変更は「`app.now` 更新をトリガーに表示が再計算される」という結合部分が新規なので、NoteCard/NotificationCardのコンポーネントテストで `app.now` を進めた後に表示文字列が変わることを検証するテストを追加する。

## スコープ外

- `relativeTime()` のフォーマット仕様変更（境界値の丸め方等）
- 仮想化（DOM上限）の仕組み自体の変更
