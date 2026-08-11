# 入力系ウィジェットのTailwind移行設計(Issue #174 第6バッチ)

## 背景

Issue #174(既存コンポーネントのTailwind移行)は以下の順で進行してきた:

- 第1バッチ(#176): レイアウト系
- 第2〜4バッチ(#177/#178/#180): モーダル群
- 第5バッチ(#181): ノート・通知表示系(`NoteCard.svelte`等)

Issue本文の想定区分のうち「入力系ウィジェット」に着手する。

## 対象

- `frontend/src/input/TqlCompletionField.svelte`(257行) — TQL(from/whereクエリ、フィルタ式)入力欄。textarea/inputと補完ポップアップの位置計算を担う
- `frontend/src/ui/CompletionPopover.svelte`(109行) — 補完候補のポップアップ本体。z-indexのみ#180で`z-[1010]`に修正済み、それ以外は未移行
- `frontend/src/render/Sparkle.svelte`(117行) — MFM `$[sparkle]`装飾のパーティクルアニメーション

対象外: `TqlCompletionField.svelte`が使う`../lib/tqlCompletion`・`../lib/caretPosition`等のロジック層、`../lib/mfmCompletion`の型定義。いずれもスタイルを持たないため対象にならない。

## 設計

### 1. `TqlCompletionField.svelte`

`class:invalid`(`<textarea>`/`<input>`双方)は、ベースの`border: 1px solid var(--border)`と`.invalid`の`border-color: var(--danger)`が同一プロパティ(border-color)を争うため、既存バッチの規約通り三項演算子で解消する:

```svelte
class={invalid
  ? "... border-destructive ..."
  : "... border-border ..."}
```

`<textarea>`と`<input>`はフォントファミリーのみ異なる(textareaはTQL専用の等幅フォント指定、inputは`font-family: inherit`)ため、それぞれ独立したクラス文字列を持つ。`<style>`ブロックは完全に削除する(`color-mix()`パターンなし)。

### 2. `CompletionPopover.svelte`

`.completion-item.selected`は、ベースの`color: var(--text)`と`.selected`の`color: var(--accent)`が同一プロパティを争うため、三項演算子で解消する(`background`の追加もあわせて三項演算子の文字列に含める)。

`.completion-popover`のz-indexに付いている「Modal.svelteより前面に出す必要がある」旨のコメントは、Tailwindクラス化後も`z-[1010]`の直前にJSXコメント(`<!-- -->`)として残す。`<style>`ブロックは完全に削除する(`color-mix()`パターンなし)。

### 3. `Sparkle.svelte`

`.mfm-sparkle`(`position: relative; display: inline-block;`)と`.layer`(`position: absolute; inset: 0; pointer-events: none; overflow: visible;`)はTailwindユーティリティへ変換する。

`.particle`(`position: absolute; width: 64px; height: 64px; margin: -32px 0 0 -32px; transform: scale(0); animation-name: ...`)と`@keyframes mfm-sparkle-particle`は、動的な`--size`カスタムプロパティ(`style`属性でパーティクルごとに設定)に依存するアニメーションであり、Tailwindでは素直に表現できないため、既存バッチの`color-mix()`パターンと同じ理由で`<style>`に残す。

**スコープ外の参考情報**: `Sparkle.svelte`は`prefers-reduced-motion`のみでアニメーションを制御しており、Issue #175で追加した設定→表示の「MFMアニメーション」トグル(`app.ui.mfmAnimationEnabled`)を見ていない(`mfm.ts`の`mfmFn()`とは別経路のため、#175の対応漏れとみられる)。今回のバッチはCSS/マークアップの移行のみが目的で`<script>`ロジックには触れないため、この点は修正せず記録に留める。

### テスト

- `frontend/src/ui/CompletionPopover.test.ts`: `.completion-popover` → `data-testid="completion-popover"`
- `frontend/src/render/Sparkle.test.ts`: `.layer` → `data-testid="sparkle-layer"`
- `TqlCompletionField.svelte`にはテストファイルが存在しないため対応不要

## リスクと対応

- `TqlCompletionField.svelte`/`CompletionPopover.svelte`は`AddColumnModal.svelte`から使われ、キャレット位置計算・矢印キー選択・IME変換中のガード等、繊細な`<script>`ロジックを持つ。今回のバッチではこれらに一切触れない
- 手動確認(`cargo tauri dev`)では、TQLエキスパートモードでの補完ポップアップ表示・選択・確定、無効な入力時の枠線色、MFM `$[sparkle]`装飾のアニメーション表示を確認する
