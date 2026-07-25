# 外部リンクアイコン付与 (Issue #97)

## 目的

ノート本文中のリンクが外部(ブラウザ/別ウィンドウ)に遷移することを視覚的に明示する。

## 対象範囲

`frontend/src/render/MfmNode.svelte` の MFM `url` ノード・`link` ノードのみ。
`mention`・`hashtag` ノードは対象外。

## 変更内容

- `@lucide/svelte` の `ExternalLink` アイコンを `url`/`link` ノードの `<a>` 末尾に付与する。
- `frontend/src/app.css` に `.mfm-link`(inline-flex, align-items:center, gap)と
  `.mfm-link-icon` の最小限のスタイルを追加する(現状 `.mfm-link` に専用スタイルなし)。

## テスト

- ロジック分岐の追加はないため `pnpm check` のみで十分。
- `cargo tauri dev` でURLを含むノートを表示し目視確認する。
