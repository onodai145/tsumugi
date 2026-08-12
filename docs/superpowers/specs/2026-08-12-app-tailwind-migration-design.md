# App.svelte Tailwind移行 設計

## 背景

Issue #174(既存コンポーネントのTailwindクラスへの移行)の一環。`frontend/src/App.svelte`(アプリのルートシェル、ヘッダー・メインカラム領域・各種モーダルのマウント)を手書きCSSからTailwindユーティリティクラスへ移行する。

## 対象

`frontend/src/App.svelte`(102行、`<style>`ブロック除く)。`class:`ディレクティブや状態依存の複雑な条件分岐は無く、既存の移行パターンをほぼ機械的に適用できる。

## 方針

### レイアウト系(そのままTailwindクラス化)

- `.app` → `flex h-screen flex-col overflow-hidden`
- `.main` → `min-h-0 min-w-0 flex-1`
- `.columns` → `flex h-full overflow-x-auto`
- `.center-msg` → `grid h-full place-items-center p-6 text-center text-muted-foreground`

### `.appbar`

`bg-muted border-b border-border`。`env(safe-area-inset-*)`を使う`padding`は動的値を含まないため、任意値クラス`p-[max(6px,env(safe-area-inset-top))_max(10px,env(safe-area-inset-right))_6px_max(10px,env(safe-area-inset-left))]`で表現する(アンダースコアではなくスペースそのままでも任意値内は角括弧内なので問題ないが、Tailwindの慣例に合わせアンダースコア区切りにする)。`.spacer`は`flex-1`。

### `.bar-btn`(＋カラム / 設定)

`Button`プリミティブに変換する(`variant="outline" size="sm"`)。ツールバー標準サイズの既存規約(VisibilitySelect/Dropdown等)に合わせる。アイコン(`SettingsIcon`)は明示`size`指定をそのまま残す(Buttonのbaseクラスにより16px相当に統一されるが、視覚差は軽微なため許容)。

**意図的な見た目の変化:** 元の`.bar-btn`は`padding: 5px 10px`・固定高さなしで実測26〜28px相当だったが、`size="sm"`は`h-8`(32px)固定になる。ヘッダーが`items-start`のため、`ComposeBar`のツールバー(前回のPRで`sm`/32pxに統一済み)との高さの整合性はむしろ向上する想定。「見た目は移行前と完全に同一」ではない点をPR本文に明記する。

### `.global-err`

`color-mix(in srgb, var(--danger) 15%, var(--surface-1))`は、Buttonのdestructiveバリアント(`bg-destructive/10 hover:bg-destructive/20`)と同じ考え方で`bg-destructive/15 text-destructive`に置き換える。この置き換えにより本ファイルの`<style>`ブロックは不要になる見込み。

**意図的な見た目の変化:** 元は不透明な`--surface-1`上に15%の`--danger`を混ぜた不透明色。`bg-destructive/15`は背景の透過15%アルファであり、かつ`--destructive`と`--danger`は理論上別トークン(近似だが同一ではない)。視覚的にはほぼ同じだが厳密には同一ではない点をPR本文に明記する。

### `.compose-fab`

56px円形のフローティングアクションボタン。Buttonのサイズトークン上限(`icon-lg`=40px)を超えるため、DisplaySectionの非標準要素と同様、rawな`<button>`のまま維持し外観のみTailwind化する(`fixed`配置、`rounded-full`、`bg-primary`、影は`shadow-[0_3px_10px_rgba(0,0,0,0.3)]`)。

### `.compose-overlay` / `.compose-modal`

動的な状態依存クラスはないため直接Tailwindクラス化する。z-indexは`z-50`のまま維持する(Modal/ConfirmDialogが使う`z-[1000]`系とは別の文脈で、同時に重なる想定がないため据え置く)。

## テスト方針

このコンポーネントに既存の自動テストはない。`pnpm check`(svelte-check+tsc)と`pnpm build`がグリーンであることを確認し、`pnpm build`後のコンパイル済みCSSに想定クラス(`z-50`, `bg-destructive/15`等)が実際に生成されていることをgrepで確認する。
