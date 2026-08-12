# ComposeBar.svelteのTailwind移行設計(Issue #174 継続バッチ)

## 背景

Issue #174(既存コンポーネントのTailwind移行)は本文に明示された5区分(レイアウト系/モーダル群/ノート・通知表示系/入力系ウィジェット/設定画面)を全7バッチで完了した。本文の「(など)」の通り、他にも手書きCSSが残るコンポーネントがあり、その筆頭である`ComposeBar.svelte`(投稿バー、1175行、うちCSS 375行)に着手する。

## 対象

`frontend/src/ui/ComposeBar.svelte`のみ。テストファイルなし。

対象外(このファイルが依存する他コンポーネント、いずれも未移行のまま):
- `AccountSelect.svelte`/`VisibilitySelect.svelte`/`Dropdown.svelte`/`DrivePicker.svelte` — 別バッチで対応
- `ReactionPicker.svelte`/`CompletionPopover.svelte`/`Modal.svelte` — 既に移行済み(第5・第6・第7バッチ)、そのまま使用

## 設計

### 条件付きクラスの衝突箇所(三項演算子で解消)

- **`.text`(投稿本文欄) + `class:compact` + `class:expanded`**: `min-height`/`resize`が3状態(通常/コンパクト/展開)で異なる。`compact`派生値は既に`!expanded`を含むため、`expanded`props → コンパクト考慮不要の3分岐三項演算子(`expanded ? ... : compact ? ... : ...`)で解消する
- **`.emoji-trigger` + `class:active`**: `color`衝突。三項演算子で解消
- **`.mini` + `class:active`**(CW/投票/チャンネルトグル、期限モードループ): `border-color`/`color`衝突。三項演算子で解消(このバッチではButtonプリミティブ化とあわせて対応、詳細は後述)

### Buttonプリミティブ化

ユーザー確認の結果、以下の13箇所をshadcn `Button`に置き換える(いずれも過去バッチで確立した「テキストラベルの単発アクション」「小さな独立したアイコンボタン」に該当):

- 投稿ボタン、エラーモーダルの「わかった」ボタン(`variant`既定)
- CW/投票/チャンネルの各トグルpill、投票期限モードの3ボタン(`variant="outline" size="xs"`、アクティブ時は`class="border-primary text-primary"`をcn()経由で上書き)
- ＋選択肢ボタン(`variant="outline" size="xs"`)
- 画像添付アイコンボタン(`variant="outline" size="icon-xs"`)
- 絵文字挿入トリガー(`variant="ghost" size="icon-xs"`、アクティブ時のクラス上書きは三項演算子。`position:absolute`配置と`onmousedown`の`preventDefault`はそのまま維持)
- 返信/引用キャンセルの×ボタン(`variant="ghost" size="icon-xs"`)
- サムネイル削除の×ボタン(`variant="ghost" size="icon-xs"`、`class`上書きで元の14px円形バッジの見た目を維持: `class="absolute -top-1 -right-1 h-3.5 w-3.5 rounded-full bg-black/60 text-white hover:bg-black/60"`)
- 投票選択肢削除の×ボタン(`variant="ghost" size="icon-xs"`)

生`<button>`のまま残す(過去バッチで確立した除外パターン、複合UIに埋め込まれたフル幅リスト行):
- 添付メニュー項目(「ローカルから選択」「ドライブから選択」) — `CompletionPopover.svelte`の項目と同じ理由でフル幅リスト行のためButton化しない

### `color-mix()`パターン

このファイルには`color-mix()`は使われていない(全て素の`var()`参照)。

### `:global()`セレクタ(維持が必要)

`.channel-select :global(.trigger)`は、まだ移行していない`Dropdown.svelte`内部の`.trigger`クラスへの外部からの上書き(padding/font-size/gapの縮小)であり、`Dropdown.svelte`自体を今回のバッチで変更しないため、この`:global()`ルールは`<style>`に残す(他バッチのフィールドと同様、クロスコンポーネント依存として維持)。

### `<style>`ブロックの最終形

上記の三項演算子化・Buttonプリミティブ化により大部分のCSSはTailwindユーティリティクラスへ変換されるが、`.channel-select :global(.trigger)`ルールのみ`<style>`に残る。

## リスクと対応

- `<script>`ロジック(メンション/ハッシュタグ補完のデバウンス検索、添付ファイルアップロード、投票フォーム、返信/引用コンテキスト同期、キーボードナビゲーション等)は一切変更しない
- ファイルが大きいため、実装計画ではマークアップ全体を一括で置き換える1タスクとして扱う(過去バッチのDisplaySection.svelteと同様の規模)
- 手動確認(`cargo tauri dev`)では、投稿本文欄のコンパクト/展開/通常の3状態切り替え、絵文字挿入ポップアップ、CW/投票/チャンネル各トグルの表示、投票フォーム(選択肢追加・削除、期限モード切替)、画像添付(ローカル/ドライブ)、返信/引用コンテキスト表示とキャンセル、投稿エラーモーダルを確認する
