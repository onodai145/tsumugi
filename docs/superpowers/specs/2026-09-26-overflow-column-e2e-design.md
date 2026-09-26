# モバイルE2E overflow検査のカラム内容対応 設計（Issue #383）

## 背景・目的

`e2e/specs-mobile/overflow.e2e.ts`（#259/#382）は `[data-columns-scroll]` の子孫を丸ごと除外して右端はみ出しを検査している。`.column-root` はその内側にあるため、ノート本文・長いURL・コードブロック・メディアグリッドなど、横はみ出しが実際に起きやすい箇所が未検査で、カラムを1つも追加しないためカラム0件の空状態しか測っていない。

ノート内容を含むカラムでも、意図しない横はみ出しを自動検出できるようにする。設計の前提・手法は `2026-09-26-mobile-e2e-design.md` / `2026-09-26-mobile-safe-area-e2e-phase2-design.md` を踏襲する（`uiMode=mobile` + 390x844、`getBoundingClientRect()`/`scrollWidth` 比較）。

## 1. 新規 `e2e/specs-mobile/overflow-column.e2e.ts`

既存 `overflow.e2e.ts`（クロム・モーダルの検査）は変更しない。データ準備の失敗を既存specに波及させないため、別ファイルにする（#385のメディアspecと同じ考え方）。

- `before`: シード管理者トークン（`signInAsSeededUser`）で以下のノートを投稿する。
  - 改行の無い長いURL
  - 改行の無い長い単語
  - 長い1行を含むコードブロック（MFMのコードフェンス）
  - 画像付きノート（`uploadImage` + `createNote(token, text, [fileId])`、#385で追加済み）
  - 長いURL・単語・コード行の長さは `LEN=200`。`NoteCard` は本文が300文字を超えると折りたたみ（overflow-hidden）ではみ出しを隠すため、閾値未満に収めつつカラム幅（約320px）を十分超える長さにしている。
- そのあとアカウントを追加してモバイルモードにし（`addAccountAndEnableMobile`）、`addHomeColumn()` を呼ぶ。
- 4件のノートがカラムに描画されるまで待つ（ノートが無いと検査が空振りするため）。

## 2. 検出方式

各カラムのノート一覧は `Column.svelte` の slot（`h-full w-full overflow-y-auto`）単位のスクロール要素で、`overflow-y-auto` により x 方向も auto 扱いになる。中身が幅を超えると意図しない横スクロールとして現れる。

- アクティブslot（`.column-root .mobile-scroll-snap > div`。本番コード変更は不要）について次を確認する。
  - `scrollWidth <= clientWidth + 1`（中身が幅を超えて横スクロールが生じていない）
  - slot自身の右端が viewport 内
- slotに加えて、slot内の各ノート（`article`）自身も `scrollWidth <= clientWidth + 1` を確認する（必須）。`NoteCard` の article は `content-visibility:auto`（paint containment）で、はみ出しを自身でクリップするため slot の `scrollWidth` が増えず、slotだけの検査ではGREENのままになる。
- 失敗時は、幅を超えている要素の一覧（タグ・class・right）と、クリップしているノート（`E2E-OVF-*` マーカー付き）をメッセージに出し、どのノートが原因か特定できるようにする。
- コードブロックなど、自身で `overflow` を持つ要素は slot の `scrollWidth` に影響しないため、意図したスクロールは自然に除外される。

## 3. 検出力の担保

各ケースについて、対応する本番CSSを一時的に外して RED を確認し、戻して GREEN を確認する（本番ファイルの変更はコミットしない）。

- `NoteCard.svelte` の本文コンテナの `min-w-0 flex-1` を `flex-1` にする → 長い単語・URL・コードのケース
- `app.css` のコードブロック `pre` を `overflow: visible` にする → コードのケース（`overflow-x: visible` だけでは `overflow-y:hidden` により auto になり効かない）
- `MediaGrid.svelte` のセルの `overflow-hidden` を外し、`img` を `w-[600px] max-w-none` にして壊しを強める → 画像のケース

## 4. 実際のバグが見つかった場合

新しいチェックが本番の実際のはみ出しバグを見つけた場合は、そのケースを別Issueとして起票し、該当テストを `it.skip`（Issue番号を含むコメント付き）にして、残りのケースだけ有効にする。本番の修正はこのIssueのスコープ外。

## 5. ドキュメント・CI

- `e2e/README.md` と `2026-09-26-mobile-e2e-design.md` に、overflow検査がカラム内容も対象にしたこと、およびその検査方式を追記する。
- CI は変更しない（`pnpm e2e:mobile` は `specs-mobile/` 全体を実行する）。

## 6. 限界

デスクトップWebKitGTK上の近似である点は #382 と同じ。ノートの見た目が多様なため、テストデータで再現できるのは上記4種に限る。
