# UIスタイルガイド

- 作成日: 2026-08-14
- 経緯: Issue #112「UIがダサい」で挙がった不統一（角丸のばらつき、カラム周りのボタンの多さ、整理されていない設定画面など）に対応するため、Issue #171でトークン・コンポーネント指針を明文化する。
- 位置づけ: 本書は**フロントエンド（`frontend/`）のビジュアルデザイン規約**。アーキテクチャ全般は `misskey-multicolumn-client-design.md` が上位ドキュメント。両者が矛盾する場合はそちらが優先。

このドキュメントはガイドラインの明文化のみが目的で、既存コンポーネントの一括置き換えは範囲外。新規実装・改修時にここへ寄せていく。

---

## 1. トークンの参照元

色は `frontend/src/app.css` の CSS変数（`--surface-1/2/3`, `--border`, `--text`, `--text-dim`, `--accent`, `--success`, `--info`, `--danger`, `--warning`）とテーマ機能（`lib/theme.ts`）がすでに一元管理している。ユーザーカスタムテーマ・プリセットが存在するため、**コンポーネント側で色を直書きしない**。Tailwindユーティリティを使う場合も `bg-background` / `text-foreground` / `border-border` など `@theme`（app.css）でCSS変数にマップ済みのクラスを使う。

角丸・タイポグラフィ・間隔は現状トークン化されておらず、コンポーネントごとにバラバラの値が使われている（下記2, 3節）。本書ではこれらのスケールを定義する。

## 2. 角丸（border-radius）

### 現状の問題

`rounded-md`（68箇所）、`rounded-lg`（47箇所）、`rounded`（27箇所）に加え、`rounded-[5px]` `rounded-[14px]` `rounded-[10px]` `rounded-[3px]` のような即値指定が並存しており、同じ役割の要素（アバター、ポップオーバー、ダイアログ等）でも箇所によって値が違う。

例:
- アバター: `AccountSelect.svelte` は `rounded-[5px]`、`NoteCard.svelte` も `rounded-[5px]` だが `ProfileModal.svelte` は `rounded-[10px]`
- ポップオーバー/ドロップダウン: `Dropdown.svelte` / `AccountSelect.svelte` は `rounded-[10px]`、`ReactionPicker.svelte` も `rounded-[10px]`
- モーダル/ダイアログ: `ConfirmDialog.svelte` / `App.svelte`（設定画面ダイアログ）は `rounded-[14px]`
- リアクションチップ・可視性バッジ（`NoteCard.svelte`）: `rounded-[3px]`

### スケール

Tailwindの既定スケール（`--radius-*`、`frontend/src/lib/components/ui/button/button.svelte` がすでに `rounded-md` を基準にしている）に統一する。即値の `rounded-[Npx]` は新規に増やさない。

| トークン | Tailwindクラス | 実測値目安 | 用途 |
|---|---|---|---|
| xs | `rounded-sm` | 2px | チップ・小バッジ（可視性アイコン、リアクション枠など。現状 `rounded-[3px]` の置き換え先） |
| sm | `rounded` | 4px | インライン要素（`kbd`、コード表示など） |
| md | `rounded-md` | 6px | ボタン、フォームコントロール、アバター（現状 `rounded-[5px]` の置き換え先）。`Button` プリミティブの既定値と揃える |
| lg | `rounded-lg` | 8px | カード、ポップオーバー・ドロップダウン（現状 `rounded-[10px]` の置き換え先） |
| xl | `rounded-xl` | 12px | モーダル・ダイアログの外枠（現状 `rounded-[14px]` の置き換え先） |
| full | `rounded-full` | 999px | アバターの丸型表示、ピル型バッジ・ボタン |

即値が必要な特殊事情（例: 親要素の border-width 分を差し引く計算がいる、など）がある場合はコメントで理由を残す。理由なく `rounded-[Npx]` を新設しない。

## 3. 間隔（spacing）

Tailwindの4px刻みスケール（`gap-1`=4px, `gap-2`=8px, `gap-3`=12px, `gap-4`=16px …）をそのまま使う。`px-[7px]` のような半端な即値パディング（例: `NoteCard.svelte` のリアクションチップ）は新規に増やさず、4px刻みの近似値（`px-2`=8px 等）に寄せる。見た目が僅かに変わっても崩れない箇所から寄せていけばよい。

## 4. ボタン

`frontend/src/lib/components/ui/button/button.svelte`（shadcn-svelte由来）が variant（`default` / `outline` / `secondary` / `ghost` / `destructive` / `link`）と size（`default` / `xs` / `sm` / `lg` / `icon` / `icon-xs` / `icon-sm` / `icon-lg`）をすでに定義している。**新しいボタンをHTML `<button>` + 個別クラス書き下ろしで作らず、まずこのプリミティブで表現できないか検討する。**

- アイコンのみのボタン → `size="icon"` 系（`icon-xs`/`icon-sm`/`icon-lg`）
- 破壊的操作（ミュート解除、アカウント削除等） → `variant="destructive"`
- 目立たせない操作（カラムヘッダのメニュー等） → `variant="ghost"`

### カラム周りのボタン過多について（Issue #112）

カラムヘッダ・投稿欄まわりのボタンが多いという指摘に対しては、本書は「新規に並べるボタンは `ghost` + `icon-sm` で統一し視覚的な主張を揃える」「頻度の低い操作は個別ボタンではなくメニュー（`Dropdown.svelte` 等）に畳む」ことを指針とする。個々の配置見直しはIssue #96等の別issueで扱う。

## 5. タイポグラフィ（font-size）

### 現状の問題

`text-[0.6rem]` 〜 `text-[1.2rem]` の任意値指定が乱立しており、確認できただけで17種類以上の異なるrem値が使われている（`text-[0.82rem]` 81箇所、`text-[0.85rem]` 58箇所、`text-[0.78rem]` 30箇所、`text-[0.8rem]` 25箇所、`text-[0.76rem]` 22箇所…とほぼ隣接する値が別々に指定されている）。Tailwindの `text-xs` / `text-sm` / `text-base` はほとんど使われていない（計9箇所のみ）。角丸よりも粒度が細かく、統一の余地が大きい。

### スケール

以下の4段階に統一する。既存の分布に近い値を採用しているため、視覚的な変化を最小限に抑えつつ寄せていける。

| トークン | Tailwindクラス | 値 | 用途 |
|---|---|---|---|
| xs | `text-xs` | 0.75rem (12px) | メタ情報（日時、可視性ラベルなど。現状 `text-[0.7rem]`〜`text-[0.76rem]` 相当） |
| sm | `text-sm` | 0.875rem (14px) | 本文の補助テキスト、ボタンラベル（現状 `text-[0.78rem]`〜`text-[0.85rem]` の大半） |
| base | `text-base` | 1rem (16px) | ノート本文など主要テキスト |
| lg | `text-lg` | 1.125rem (18px)、既存の `text-[1.1rem]`〜`text-[1.2rem]` の置き換え先 | 見出し、強調表示 |

即値の `text-[Nrem]` は新規に増やさない。上記4段階のどれにも当てはまらない特殊事情がある場合のみ、コメント付きで例外を許容する。

## 6. アイコンサイズ

lucideアイコン（`size={N}` prop）の指定値が `10/12/13/14/15/16/20` の7段階に分散している。以下の2段階に統一する（`Button` プリミティブの `[&_svg:not([class*='size-'])]:size-4`＝16pxがデフォルト、`size="xs"`系の `size-3`＝12pxと整合させる）。

| トークン | 値 | 用途 |
|---|---|---|
| sm | `size={12}` | インラインの小アイコン、密なUI（リアクションチップ内など） |
| default | `size={16}` | 通常のボタン・ヘッダーアイコン |

大きめのアイコン（`size={20}` 等）が必要な場合は例外として許容するが、13/14/15pxのような1px刻みの中間値は新規に増やさない。

## 7. フォーカス・アクセシビリティ

`focus-visible:` によるキーボードフォーカスの視覚表示は、現状 `Button` プリミティブ（`lib/components/ui/button/button.svelte`）にしか実装されていない。`app.css` の `button` リセット（Preflight未導入のため素のブラウザボタンの見た目を最小限リセットしている）はアウトラインを除去する効果を持たないが、`Button` を経由しないカスタムの対話要素（`role="button"` のdiv、独自クラスの `<button>` など）は `focus-visible:` を明示的に持たない限りキーボード操作時の視認性が担保されない。

新しく対話要素を作る際は、`Button` プリミティブを使うか、使えない場合は `Button` の `focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50` パターンを踏襲する。

## 8. シャドウ

ポップオーバー・ドロップダウン・モーダルの浮遊要素には `shadow-[0_8px_24px_rgba(0,0,0,0.25)]` を標準値とする（`Dropdown.svelte` / `AccountSelect.svelte` / `ReactionPicker.svelte` など9箇所で採用済み）。これと異なるぼかし・不透明度の即値（`shadow-[0_4px_16px_rgba(0,0,0,0.25)]` 等）を新規に増やさない。

## 9. 設定画面（`ui/settings/*Section.svelte`）

`Settings.svelte` はセクション分割済み（`AccountsSection` / `DisplaySection` / `NotifySection` / `MuteSection` / `ReactionSection` / `KeysSection` / `DataSection` / `AboutSection`）。新しい設定項目を追加する際は既存のいずれかのセクションに載せるか、性質が明確に異なる場合のみ新規セクションを切る。セクション内の見出し・説明文・コントロールの縦間隔は既存セクションの実装（例: `DisplaySection.svelte`）に揃える。

## 10. 色の意味づけ

`--success` / `--info` / `--danger` / `--warning` は用途が固定されている（例: Renoteバナー = success、リプライバナー = info）。新しい用途にこれらを流用する場合、既存の意味づけと衝突しないか確認する。装飾目的の色選びには `--accent` を使い、意味づけ色を装飾用途に転用しない。

## 11. 今後

本書はコンポーネント未移行の指針整備のみ。既存コンポーネントの一括修正は行っていないため、触れたファイルから本書のスケールに寄せていく。大規模な一括置換が必要と判断した場合は別issueを切る。
