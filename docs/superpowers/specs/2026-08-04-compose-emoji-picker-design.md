# 投稿欄への絵文字ピッカー追加 (Issue #20)

## 背景・目的

現在、絵文字ピッカー (`frontend/src/input/ReactionPicker.svelte`) はノートへのリアクション追加時にしか使えない。投稿欄 (`ComposeBar.svelte`) では `:` を打って候補をインクリメンタル検索する補完機能 (`mfmCompletion.ts` + `CompletionPopover.svelte`) しかなく、絵文字名が分からないユーザーがブラウズして選ぶ手段がない。本Issueでは、投稿欄でも既存のピッカーUIを使って絵文字を選択・挿入できるようにする。

## スコープ

- `ComposeBar.svelte` のツールバーに絵文字ピッカーボタンを追加する。
- 新規コンポーネントは作らず、既存の `ReactionPicker.svelte` をそのまま再利用する。
- 対象は `ComposeBar.svelte` を使う全箇所（デスクトップ投稿欄、モバイル投稿モーダル、返信、引用）。個別対応は不要（共通コンポーネントのため自動的に反映される）。
- カラム設定・ノートへのリアクション付与など、他の絵文字ピッカー呼び出し箇所には変更を加えない。

## UI

- ツールバー左側、画像添付ボタン (`ImagePlus`) の隣に絵文字ピッカーボタンを追加する。アイコンは `SmilePlus`（`NoteCard.svelte` のリアクションボタンと同じアイコンを流用）。
- クリックでポップオーバーとして `ReactionPicker` を表示する。表示パターンは既存の添付メニュー (`showAttachMenu` / `.attach-overlay` / `.attach-menu`) と同じ構成を踏襲する：
  - `lib/portal.ts` の `portal` アクションで `document.body` 直下へ移動
  - オーバーレイ (`role="presentation"`, `onclick`で閉じる) の上に配置し、ポップオーバー自体のクリックは `stopPropagation`
  - ボタンの位置 (`getBoundingClientRect`) を起点に `left`/`top` を計算
- `ReactionPicker` の `showPinned` はデフォルト値 (`true`) のまま使う（ピン留め・最近使った絵文字も投稿欄から使えるようにする）。

## 選択後の挙動

- 絵文字を選んでもピッカーは閉じない（連続入力向け）。閉じるのはボタン再クリックまたは外側クリックのみ。
- 選んだ絵文字はテキストエリアの現在のカーソル位置 (`cursorPos`) に挿入する。選択範囲の置換は行わない（常に単純挿入）。
- 挿入後はテキストエリアにフォーカスを戻し、カーソル位置を挿入したテキストの直後に更新する（`confirmCompletion` と同様のパターン）。

## テキスト変換ロジック

`ReactionPicker.onpick` はリアクション用のキー形式で値を返す：
- Unicode絵文字: 文字そのもの (例: `😀`)
- カスタム絵文字: `:name@.:` 形式 (`emojiKey.ts` の `customEmojiKey`)

投稿本文にはMFMショートコードとして自然な形で挿入したいため、以下の変換を行う：
- `isCustomEmojiKey(key)` が真の場合、`parseCustomEmojiPinKey(key)` で `name` を取り出し `:name:` に変換する
- それ以外（Unicode文字）はそのまま挿入する

この変換は `mfmCompletion.ts` の `buildCompletionItems` が絵文字補完で生成する `insertText` (カスタムは `:name:`、Unicodeは文字そのもの) と同じ結果になる。変換ロジックはComposeBar内に小さなヘルパー関数として実装する（他箇所で再利用する予定がないため、共通モジュール化はしない）。

## テスト方針

- 変換ヘルパー関数（reaction key → 挿入テキスト）の単体テストを追加する。
- 既存の `ComposeBar` 関連テストがあれば、ピッカーボタンの表示・トグル動作を軽く確認する（実際のUI操作確認は `cargo tauri dev` で手動実施）。

## 対象外

- リアクションピッカー側 (`ReactionPicker.svelte` 自体) の変更は行わない。
- ノート内の絵文字ピッカー呼び出し箇所（リアクション、カラム設定）への影響はない。
