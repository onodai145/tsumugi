# 設定画面のTailwind移行設計(Issue #174 第7バッチ)

## 背景

Issue #174(既存コンポーネントのTailwind移行)は以下の順で進行してきた:

- 第1バッチ(#176): レイアウト系
- 第2〜4バッチ(#177/#178/#180): モーダル群
- 第5バッチ(#181): ノート・通知表示系
- 第6バッチ(#184): 入力系ウィジェット

Issue本文の想定区分のうち最後に残った「設定画面」に着手する。

## 対象

`frontend/src/ui/Settings.svelte`(設定モーダルのシェル本体、156行)と、`frontend/src/ui/settings/`配下の8ファイル(合計2097行):

- `AboutSection.svelte`(103行)
- `AccountsSection.svelte`(199行)
- `DataSection.svelte`(131行)
- `DisplaySection.svelte`(905行、突出して大きい)
- `KeysSection.svelte`(247行)
- `MuteSection.svelte`(117行)
- `NotifySection.svelte`(175行)
- `ReactionSection.svelte`(220行)

いずれのファイルにも既存テストファイルは無いため、`data-testid`移行は不要。

## 設計

### 0. `Settings.svelte`(モーダルシェル本体)

幅640px・サイドナビ(160px)+ペインの2列レイアウト(それぞれ独立スクロール)・高さ上限84vhという、共通`Modal.svelte`(幅480px固定・単一コンテンツ領域・パディング固定)とは構造がかなり異なる。以下の方針で`Modal.svelte`を最小限拡張し、既存4呼び出し元(`AddColumnModal`/`ProfileModal`/`FollowListModal`/`ComposeBar`)には一切影響を与えない形で統合する:

- `Modal.svelte`に任意の`width` prop(既定`"480px"`)を追加する。モーダル本体のクラスは`` `w-[min(${width},92vw)] rounded-[14px] border border-border bg-background p-4` ``のようにテンプレート文字列へ埋め込む。これは条件によって複数のクラス文字列を切り替えるものではなく、単一の値をテンプレートに埋め込むだけなので、条件付きクラスの禁止パターン(同じCSSプロパティを争う複数クラスの個別トグル)には該当しない。既存呼び出し元は`width`未指定のため従来通り480pxのまま
- `Settings.svelte`は`<Modal title="設定" {onclose} width="640px">`とし、中身(サイドナビ+ペイン)は`children`として渡す。`Modal.svelte`の`p-4`パディングは、Settings側のchildren直下で`-m-4`により相殺し、2列レイアウトを自前で組む:
  ```svelte
  <div class="-m-4 flex max-h-[84vh] flex-col overflow-hidden">
    <div class="flex min-h-0 flex-1 border-t border-border">
      <nav class="w-40 flex-none overflow-y-auto border-r border-border bg-muted p-2">...</nav>
      <section class="min-w-0 flex-1 overflow-y-auto p-5">...</section>
    </div>
  </div>
  ```
  `border-t`は、元CSSで`.head`(タイトル行)と`.body`(サイドナビ+ペイン)の間にあった`border-bottom`と視覚的に同じ区切り線を、`Modal.svelte`のヘッダー自体には手を入れずに再現するもの
- `.nav-item.active`(`background`/`color`衝突)は既存バッチの規約通り三項演算子で解消する
- `color-mix()`パターンなし。`<style>`ブロックは完全削除

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
