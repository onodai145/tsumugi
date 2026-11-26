# 個別ダイアログを、フォーカスが外れた後も Escape で閉じる(Issue #700)

## 目的

次の4つのダイアログで、フォーカスが `body` に移った後も Escape が効くようにする。

- `frontend/src/ui/ConfirmDialog.svelte`
- `frontend/src/ui/ReportUserDialog.svelte`
- `frontend/src/ui/settings/LicenseConsentDialog.svelte`
- `frontend/src/ui/settings/SpeechCoreTermsDialog.svelte`

成功の条件:

- フォーカスが `body` にあるとき、Escape で一番手前のダイアログのキャンセルが呼ばれる。
- 下にある `Modal` は同じ Escape で閉じない。
- 通報の送信中は、Escape で閉じない(下の `Modal` も閉じない)。

## 原因

4つとも Escape を、外側の要素の `onkeydown` だけで受けている。フォーカスが `body` にあるキー入力はこの要素に届かない。下の `Modal`(#698 で window 側の受け口を持った)は、手前に `aria-modal` の要素があるので反応しない。

コード上でフォーカスが外れうる経路:

| ダイアログ | 経路 |
|---|---|
| `ReportUserDialog` | 「通報する」が送信中に `disabled` になる。失敗するとダイアログは開いたまま残る |
| `LicenseConsentDialog` / `SpeechCoreTermsDialog` | 同意ボタンが開始処理中に `disabled` になる。成否どちらでも閉じるので、処理中の間だけ |
| `ConfirmDialog` | 消える要素・無効になる要素がなく、経路は見つかっていない |

`ConfirmDialog` も、変更が小さく挙動がそろうので対象に含める。

WebKitGTK がボタンの無効化でフォーカスを `body` に移すかは未確認。実画面の確認で確かめる。

## 設計

### 共通処理

`frontend/src/lib/escapeOnLostFocus.ts` に Svelte アクションを置く。

```ts
export function escapeOnLostFocus(node: HTMLElement, close: () => void)
```

- `node` は `role="dialog" aria-modal="true"` の要素。一番手前かどうかの比較に、この要素を使う。
- `window` の `keydown` を受け、次をすべて満たすときだけ `preventDefault` して `close` を呼ぶ。条件は `Modal.svelte` の `onWindowKeydown` と同じ。
  - キーが Escape
  - `defaultPrevented` でない
  - イベントの対象が `document.body`
  - `[aria-modal="true"], [data-escape-layer]` のうち、DOM 上で最後の要素が `node`
- アクションの破棄でリスナーを外す。

呼び出し側は `use:escapeOnLostFocus={() => onCancel()}` のようにアロー関数で渡し、呼ぶ時点の props を使う。

重なった画面の `window` リスナーは、どちらが先に呼ばれても結果が同じになる。下の画面が先なら、手前でないので何もしない。手前が先なら `preventDefault` するので、下は何もしない。

### 適用先

| コンポーネント | 渡す処理 |
|---|---|
| `Modal` | `onclose`(今の `onWindowKeydown` と `<svelte:window>` を置き換える。`bind:this` の `modalEl` はフォーカス処理で使うので残す) |
| `ConfirmDialog` | `onCancel` |
| `ReportUserDialog` | `cancel`(送信中は何もしない) |
| `LicenseConsentDialog` | `onCancel` |
| `SpeechCoreTermsDialog` | `onCancel` |

各コンポーネントの外側の `onkeydown` は残す。こちらは内側にフォーカスがあるときの経路で、対象が `body` のイベントは届かないので、二重には呼ばれない。

同意ダイアログの開始処理中の Escape は、キャンセルになる。キャンセルボタンも処理中に押せるので、同じ挙動。

### 文書

`docs/design/style-guide.md` §13 の「`ConfirmDialog` 等の個別ダイアログはこの受け口を持たない」を、アクション `escapeOnLostFocus` を使う規約に書き換える。

## 検討して採らなかった案

- 関数に切り出して各コンポーネントに `<svelte:window>` と `bind:this` を書く: 同じ記述が5回増える。
- 4つの土台になる共通ダイアログコンポーネントを作る: この Issue には大きすぎる。
- フォーカスをダイアログに戻す: フォーカスが外れるタイミングに左右される(#698 で不採用)。

## 範囲外

- フォーカスが `body` 以外のダイアログ外の要素にある場合(フォーカストラップはしない)。

## 検証

単体テスト(jsdom はボタンの無効化でフォーカスを移さないので、テストで明示的に外す):

- 4ダイアログそれぞれ: `body` での Escape でキャンセルが1回呼ばれる。`Modal` に重ねたときは `Modal` が閉じない。
- `ReportUserDialog`: 送信中は `oncancel` も `Modal` の `onclose` も呼ばれない。
- `Modal`: 既存の #698 のテストがそのまま通る。
- `pnpm test` と `pnpm check`。

実画面(Linux のデバッグ版を Xvfb で、使い捨ての Misskey と、コミットしない使い捨て spec で):

- 通報の送信を失敗させた後の Escape を、修正前後で比べる。そのときの `document.activeElement` も記録する。
