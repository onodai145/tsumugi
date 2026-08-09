# AddColumnModal.svelte Tailwind移行設計(Issue #174 第4バッチ)

## 背景

Issue #174(既存コンポーネントのTailwind移行)は以下の順で進行してきた:

- 第1バッチ(#176): レイアウト系(`Column.svelte`/`Pane.svelte`/`Backstage.svelte`)
- 第2バッチ(#177): モーダル基盤(`Modal.svelte`/`ConfirmDialog.svelte`)
- 第3バッチ(#178): `ProfileModal.svelte`/`FollowListModal.svelte`

残るコンポーネントは `AddColumnModal.svelte`(706行、カラム/タブの追加・編集フォーム)のみ。加えて、第3バッチの最終レビューで発覚した `ConfirmDialog.svelte` の余白バグ(未修正のまま残置)を本バッチで解消する。

## 対象と対象外

- **対象**: `frontend/src/ui/AddColumnModal.svelte`(script部分は変更なし、テンプレート/`<style>`のみTailwind化)、`frontend/src/ui/ConfirmDialog.svelte`(1点の余白修正のみ)
- **対象外**: `Dropdown.svelte`/`AccountSelect.svelte`/`input/TqlCompletionField.svelte`(子コンポーネント、独自スタイルのまま維持。過去バッチと同じ方針)
- テストファイル: `AddColumnModal.test.ts` は存在しないため、`data-testid` 追加やテスト修正は本バッチでは発生しない見込み。移行中に既存テスト([`ui/*.test.ts`]内でこのファイルの内部構造を参照するもの)を壊していないか `pnpm test` で確認する。

## 設計

### 1. 共通 `Modal.svelte` への統合

現状 `AddColumnModal.svelte` は overlay/modal/header/portal/closeボタンを自前実装しており、以下の点で `Modal.svelte`(第2バッチで整備済み)と乖離している:

- `z-50` vs `Modal.svelte` の `z-[1000]`(他モーダルとの重なり順が理論上不整合になりうる)
- portalなし(深くネストされた場所から開いても `content-visibility`/`contain` の包含ブロックを脱出できない可能性。ただし `AddColumnModal` は現状トップレベルからしか開かれないため実害は薄いが、将来の呼び出し元変更に対して脆い)
- closeボタンが生の `<button class="x">` で `Modal.svelte` の `Button variant="ghost" size="icon-xs"` と見た目が微妙に異なる

このバッチで `Modal.svelte` を使う形にリファクタする:

```svelte
<Modal title={isEdit ? "タブを編集" : groupId ? "タブを追加" : "カラムを追加"} {onclose}>
  <!-- 既存のフィールド群(.field 相当)をそのまま children として配置 -->
</Modal>
```

`Modal.svelte` は幅固定 `w-[min(480px,92vw)]` を内蔵しており、`AddColumnModal.svelte` の現行幅(`480px`)と一致するためそのまま使える。ヘッダーの `<span>{title}</span>` + closeボタンの構造も `Modal.svelte` 側で完結するため、`AddColumnModal.svelte` 側の `<header class="head">...</header>` ブロックは丸ごと削除する。

### 2. フィールド群のTailwind化

既存の `<style>` ブロックのCSSプロパティを、確立済みトークンブリッジ(`app.css` の `@theme`)に従って対応するTailwindユーティリティへ置き換える:

| 旧CSS変数 | Tailwindユーティリティ |
|---|---|
| `--surface-1`(背景) | `bg-background` |
| `--surface-2`(input/seg-btn背景) | `bg-muted` |
| `--surface-3`(code背景) | `bg-accent` |
| `--border` | `border-border` |
| `--text` | `text-foreground` |
| `--text-dim` | `text-muted-foreground` |
| `--accent`(強調背景) | `bg-primary` |
| `--danger` | `text-destructive` |

対象クラス(現行 `<style>` の各セレクタ):

- `.field`(`flex flex-col gap-1 mb-2.5 text-[0.85rem]`)、`.field > span:first-child`(`text-muted-foreground`)
- `.check-row`(`flex items-center gap-1.5 text-[0.85rem]`)
- `input`(`px-2.5 py-2 border border-border rounded-lg bg-muted text-foreground font-[inherit]`) — `<input>` 要素自体への直接適用なので、既存の `Modal.svelte` 系コンポーネントと違い element selectorではなく各 `<input>` に `class` を付与する形に変える(Preflight除外環境でも影響しないよう明示的に指定)
- `.seg` / `.seg-btn` / `.seg-btn.active` — 「簡単/エキスパート」切替。`Column.svelte` の衝突バグ修正パターンに倣い、`class:active={...}` の混在ではなく単一クラス選択(三項演算子)にする:
  ```svelte
  class={uiMode === "guided"
    ? "border-r border-border bg-primary px-3.5 py-1.5 text-[0.82rem] text-primary-foreground"
    : "border-r border-border bg-muted px-3.5 py-1.5 text-[0.82rem] text-foreground"}
  ```
- `.bg-row`(`flex items-center gap-2.5`)
- `.mini-btn` / `.mini-btn:hover` / `.mini-btn:disabled` — hover/disabledはTailwindの `hover:`/`disabled:` variantで表現
- `.hint` / `.hint code` — `<p class="mb-2 mt-0 text-[0.75rem] text-muted-foreground">` (Preflight除外環境でのUAデフォルトmargin対策として `mt-0` を明示。第3バッチで踏んだ罠の再発防止)
- `.actions`(`flex justify-end mt-1.5`)
- `.submit` — `Modal.svelte`/`ProfileModal.svelte` 同様、生の `<button>` ではなく shadcn `Button`(`variant="default"`)に置き換える。`disabled` propはそのまま渡す
- `.err` — `<p class="mt-2 mb-0 text-[0.82rem] text-destructive break-words">`(同じくUAデフォルトmargin対策で `mt-0` 相当を明示。ここは既存が `margin: 8px 0 0` で上のみなので `mt-2 mb-0` で足りる)

`.mini-btn` も同様に `Button variant="outline" size="sm"` へ置き換え候補だが、「試聴」「音声ファイルを選択」ボタンは横並びで小さいテキストボタンなので `Button variant="outline" size="xs"` を使う(第1バッチで導入済みの `size="xs"` を再利用)。

### 3. `ConfirmDialog.svelte` の余白修正

```diff
- <p class="mb-4 whitespace-pre-wrap text-[0.85rem] text-foreground">{message}</p>
+ <p class="mb-4 mt-0 whitespace-pre-wrap text-[0.85rem] text-foreground">{message}</p>
```

Preflight除外環境でUAデフォルトの `margin-block-start: 1em` が乗ってしまうバグを解消する(第3バッチ最終レビューで発見、このバッチで直す約束済み)。

### 4. テスト

- `pnpm test` を実行し、既存の全テストスイート(特に `AddColumnModal` を間接的に参照するものがないか)が壊れていないことを確認する。
- `pnpm check` で型エラーがないことを確認する。
- 手動確認: 「カラムを追加」「タブを追加」「タブを編集」の3パターンで開き、簡単/エキスパート切替、各ソース種別のフィールド出し分け、通知設定、送信エラー表示が視覚的に壊れていないことを確認する。

## リスクと対応

- `<input>` 要素はグローバルセレクタでスタイルされていたため、`class` 未付与の `<input>` が今後追加されると見た目が崩れる。今回対象の3箇所(`name`/`userAcct`/`tagText`/`searchQuery`用input、計4箇所)全てに明示的に `class` を付与することで対応する。
- `Modal.svelte` へ移行することで `groupId`/`isEdit` 分岐によるtitle文言のロジックは維持されるが、`Modal.svelte` の `title` propは `string` 型固定のため動的文字列をそのまま渡せることを確認済み(型シグネチャ上問題なし)。
