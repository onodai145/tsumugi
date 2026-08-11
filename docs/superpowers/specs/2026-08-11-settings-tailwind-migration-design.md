# 設定画面のTailwind移行設計(Issue #174 第7バッチ)

## 背景

Issue #174(既存コンポーネントのTailwind移行)は以下の順で進行してきた:

- 第1バッチ(#176): レイアウト系
- 第2〜4バッチ(#177/#178/#180): モーダル群
- 第5バッチ(#181): ノート・通知表示系
- 第6バッチ(#184): 入力系ウィジェット

Issue本文の想定区分のうち最後に残った「設定画面」に着手する。

## 対象

`frontend/src/ui/settings/`配下の8ファイル(合計2097行):

- `AboutSection.svelte`(103行)
- `AccountsSection.svelte`(199行)
- `DataSection.svelte`(131行)
- `DisplaySection.svelte`(905行、突出して大きい)
- `KeysSection.svelte`(247行)
- `MuteSection.svelte`(117行)
- `NotifySection.svelte`(175行)
- `ReactionSection.svelte`(220行)

**対象外**: `frontend/src/ui/Settings.svelte`(設定モーダルのシェル本体)。幅640px・サイドナビ+ペインの2列レイアウト(それぞれ独立スクロール)・高さ上限84vhという、共通`Modal.svelte`(幅480px固定・単一コンテンツ領域)とは構造がかなり異なるため、`Modal.svelte`の拡張要否を含めて別バッチで検討する。今回は8ファイルとも`Settings.svelte`から`<... Section />`として呼ばれる形は変えず、各セクション内部のマークアップ/スタイルのみを移行する。

いずれのファイルにも既存テストファイルは無いため、`data-testid`移行は不要。

## 設計

### 共通パターン

8ファイルのうち6ファイル(About/Accounts/Data/Keys/Mute/Notify)は、既存バッチ(Mute/Data比較で確認済み)と同じ「フォーム+保存ボタン+エラー表示」の定型パターンを共有している: `.field`(ラベル+入力のグループ)/`.hint`(説明文)/`.actions`(右寄せの保存ボタン行)/`.ok`(保存成功メッセージ、`--success`)/`.err`(エラーメッセージ、`--danger`)/`.save`(塗りつぶしボタン)。これらは全ファイルで同じCSS値(padding/radius/font-size等)を使っているため、Tailwindクラスもファイル間で完全に同一の文字列になる。

### 1. `AboutSection.svelte`

条件付きクラスの衝突なし。`.update-banner`の`background: color-mix(in srgb, var(--accent) 15%, transparent)`のみ`<style>`に残す(既存バッチと同じ`color-mix()`パターン)。

### 2. `AccountsSection.svelte`

条件付きクラスの衝突なし。`.default-badge`の`background: color-mix(in srgb, var(--accent) 22%, transparent)`のみ`<style>`に残す。

### 3. `DataSection.svelte`

条件付きクラスの衝突なし、`color-mix()`パターンなし。`<style>`ブロックは完全削除。

### 4. `KeysSection.svelte`

条件付きクラスの衝突なし、`color-mix()`パターンなし。`<style>`ブロックは完全削除。`kbd`要素の`font-family: ui-monospace, monospace`は`font-[ui-monospace,monospace]`のアービトラリ値で表現する。

### 5. `MuteSection.svelte`

条件付きクラスの衝突なし、`color-mix()`パターンなし。`<style>`ブロックは完全削除。

### 6. `NotifySection.svelte`

条件付きクラスの衝突なし、`color-mix()`パターンなし(`.mini-btn:hover { border-color: var(--accent) }`は素の`var()`参照)。`<style>`ブロックは完全削除。

### 7. `ReactionSection.svelte`

`.chip.dragging`(`opacity: 0.4`のみの追加)は既存バッチの規約通り、単一の完全なクラス文字列を選ぶ三項演算子で解消する。`color-mix()`パターンなし、`<style>`ブロックはこの1箇所を除いて完全削除(三項演算子化するため`<style>`自体は不要になる)。

### 8. `DisplaySection.svelte`(最大・最重要)

条件付きクラス`class:active`が11箇所に登場する(いずれも過去バッチと同じ理由で三項演算子への変換が必須):

- `.seg-btn.active`(`background`/`color`衝突): UIモード切替・テーマ切替・絵文字スタイル切替・フォントプリセット切替・背景配置方法切替、計5箇所の`{#each}`ループ内
- `.theme-card.active`(`border-color`/`box-shadow`衝突): プリセットテーマ・カスタムテーマ・コードハイライトテーマ(自動+バンドル済み+カスタム)、計4箇所
- `.pos-btn.active`(`background`/`border-color`衝突): 背景画像の基準点(9点グリッド)、1箇所

いずれも「アクティブ時の完全なクラス文字列」と「非アクティブ時の完全なクラス文字列」を選ぶ三項演算子にする。`{#each}`ループ内の判定条件(`theme === p.id`等)はそのまま維持する。

`color-mix()`パターンはこのファイルには無い(スウォッチの動的な色は`style={`background:${colors[...]}`}`のようにカスタムテーマの16進カラー値を直接埋め込む**インラインstyle属性**であり、Tailwindクラスへの変換対象ではない。これは元々`<style>`のCSSルールではなく、`<script>`側で計算した値の描画なので、今回のバッチでは一切変更しない)。

`kbd`同様、`.hex-input`の`font-family: ui-monospace, monospace`は`font-[ui-monospace,monospace]`で表現する。`fontPresets`のプレビュー行(`<p class="hint" style={fontFamily ? \`font-family: ${fontFamily}\` : undefined}>`)もインラインstyleであり変更しない。

`.theme-card-name :global(.theme-card-check)`(Lucideの`Check`アイコンへの`color: var(--accent)`)は、アイコンコンポーネントに直接`class="text-primary"`を渡す形に置き換え、`:global()`セレクタを解消する(過去バッチのNoteMenu.svelteでの`:global(.chevron)`解消と同じパターン)。

## リスクと対応

- 8ファイルはそれぞれ独立してレンダリングされる別コンポーネントであり、相互依存はない(いずれも`Settings.svelte`から並列に呼ばれるだけ)
- `DisplaySection.svelte`はカスタムテーマ/カスタムシンタックステーマの編集フォーム、背景画像設定など多機能だが、`<script>`ロジック(保存処理・バリデーション・ドラッグ&ドロップ等)は一切変更しない
- 手動確認(`cargo tauri dev`)では、各セクションの表示・保存動作に加え、`DisplaySection`のセグメントボタン/テーマカード/基準点グリッドのアクティブ状態表示、`ReactionSection`のドラッグ中の半透明表示を確認する
