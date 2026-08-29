# ノート本文コピー機能 設計 (Issue #255)

## 背景

現状、ノート本文は選択範囲コピー（`handleNyaizeCopy` / `frontend/src/lib/nyaizeCopy.ts`）で
プレーンテキストとしてコピーできる。ただし選択コピーはレンダリング結果（DOM）から
テキストを再構成するため、MFM記法（`**bold**` や `$[tada ...]` など）はレンダリング後の
見た目（太字表示や変形後の文字）になり、MFM記法そのものはコピーできない。

Issue #255 はこれを解消し、MFM記法込みの生テキストをコピーできるようにする要望。

## 方針

`NoteMenu.svelte`（ノートカードの「…」メニュー）に「内容をコピー」項目を追加し、
`note.text`（サーバーから受け取った生のMFM原文）を `navigator.clipboard.writeText()` で
クリップボードに書き込む。

- 対象テキストは `note.text` のみ。CW (`note.cw`) は含めない（本家Misskeyの
  「テキストをコピー」と同じ挙動に合わせる）。
- `note.text` が `null` または空文字列の場合（メディアのみ投稿など）はメニュー項目自体を
  表示しない。
- メニュー項目のラベルは「内容をコピー」。
- nyaize（猫語変換）はフロントエンドのレンダリング時処理（`Mfm` コンポーネントへの
  `nyaize={inner.user.isCat}` 指定）であり、`note.text` 自体は変換前の原文なので、
  猫アカウントの投稿でも変換前のテキストがそのままコピーされる。既存の選択範囲コピー
  （nyaize復元）と結果的に一貫する。
- Rust側の変更は不要。`navigator.clipboard.writeText` はTauri WebView上でも動作する。
- 既存の選択範囲コピー機能（`handleNyaizeCopy`）はそのまま維持し、独立した機能として
  追加する。

## UI配置

`NoteMenu.svelte` 内、既存の「お気に入り登録」ボタンの上に追加する。
アイコンは `@lucide/svelte` の `Copy` を使用する。

## テスト

Vitest で以下を検証する:
- `note.text` がある場合にメニュー項目が表示されること
- `note.text` が `null`/空文字列の場合にメニュー項目が表示されないこと
- クリック時に `navigator.clipboard.writeText` が `note.text` の値で呼ばれること
