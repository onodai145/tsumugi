# NoteCard周辺のTailwind移行設計(Issue #174 第5バッチ)

## 背景

Issue #174(既存コンポーネントのTailwind移行)は以下の順で進行してきた:

- 第1バッチ(#176): レイアウト系(`Column.svelte`/`Pane.svelte`/`Backstage.svelte`)
- 第2バッチ(#177): モーダル基盤(`Modal.svelte`/`ConfirmDialog.svelte`)
- 第3バッチ(#178): `ProfileModal.svelte`/`FollowListModal.svelte`
- 第4バッチ(#180): `AddColumnModal.svelte`(モーダル群完了)

Issue本文の想定区分のうち「ノート・通知表示系」が未着手として残っている。本バッチではこれに着手する。

## 対象

- `frontend/src/ui/NoteCard.svelte`(722行) — タイムラインに表示されるノート本体。最も表示頻度が高く、Issue #112の「デザインに統一感がない」体感への影響が最大
- `frontend/src/ui/NotificationCard.svelte`(162行) — 通知一覧の1行。内部で`NoteCard`をネスト利用
- `frontend/src/input/ReactionPicker.svelte`(245行) — リアクション選択ポップオーバー(NoteCardからポータルで呼ばれる)
- `frontend/src/ui/NoteMenu.svelte`(200行) — ノートの「その他」メニュー(お気に入り/クリップ追加、NoteCardからポータルで呼ばれる)
- `frontend/src/ui/ReactionUsersPopover.svelte`(179行) — リアクション/Renoteの「誰が」ホバーポップオーバー(NoteCardからポータルで呼ばれる)

対象外: `MediaGrid.svelte`/`Mfm.svelte`/`CustomEmoji.svelte`/`UnicodeEmoji.svelte`等、NoteCardが内部で使うさらに下位の描画コンポーネント。これらは別のバッチ(入力系ウィジェット・レンダリング系)で扱う。

## 設計

### 1. `NoteCard.svelte`

#### 条件付きクラスの解消方針

過去バッチ(#176/#178/#180)で確立した規約通り、「同じCSSプロパティを設定する複数のクラスを`class:`ディレクティブや`class={[...]}`配列で個別にON/OFFする」書き方は禁止し、1つの完全なクラス文字列を選ぶ三項演算子にする。本ファイルで該当する箇所:

- **`<article class="note" class:quoted class:selected={...}>`**: `.note`の基本`padding`/`content-visibility`と`.quoted`の上書き(`padding`/`border`/`margin-top`/`content-visibility`)が同一プロパティで衝突するため、`quoted`の真偽で完結したクラス文字列を選ぶ三項演算子にする。`selected`(`background`/`box-shadow`のみ追加、`quoted`とは非衝突)は`quoted`側の文字列に対してさらに三項演算子で連結する(`selected && !quoted`の条件を維持)
- **`.poll-choice` + `class:voted`**: `voted`は`outline`のみの追加(他プロパティと非衝突)だが、規約に従い三項演算子で統一する
- **`.reaction` + `class:mine`**: `mine`は`border-color`/`background`の追加。三項演算子で統一する
- **`.actions button` + `class:busy`/`class:on`**: `busy`(`opacity`)と`on`(`color`/`background`、hoverと共有)はボタンごとに独立した状態なので、それぞれ三項演算子で統一する

#### `color-mix()`パターン

以下は既存バッチと同じくTailwindユーティリティに変換せず`<style>`に残す(`--column-opacity`によるカラム背景不透明度、Tailwindに色混合ユーティリティが無いため):
- `.avatar.placeholder`
- `.cw-toggle`
- `.poll-choice` / `.poll-choice:hover`
- `.reaction` / `.reaction.mine`

#### z-index

`.picker-overlay`(リアクションピッカー/ノートメニューの背景オーバーレイ)は現状`z-index: 1000`で、`Modal.svelte`(第2バッチ、`z-[1000]`)と同値。#180で発見したCompletionPopoverの教訓(同値だとDOM追加順に依存する不安定な重なりになる)を踏まえ、**`z-index: 1010`に引き上げる**。`ProfileModal`はノート一覧に`NoteCard`をアクション付きで表示するため、モーダルを開いたままリアクションピッカーを開く組み合わせが実際に起こりうる。

#### `-webkit-user-select`

ドラッグ選択防止(`.note`)とテキスト選択許可(`.text`/`.cw-text`)は、WebKitGTKが無印字プロパティを反映しない既知の制約(CLAUDE.mdにも記載)のため、`-webkit-user-select`と`user-select`を両方指定する必要がある。Tailwindの`select-none`/`select-text`ユーティリティは無印字の`user-select`のみを生成するため、任意値`[-webkit-user-select:none]`/`[-webkit-user-select:text]`を併記する。

### 2. `NotificationCard.svelte`

条件付きクラスの衝突なし。静的クラスをそのままTailwindユーティリティへ変換する。

### 3. `ReactionPicker.svelte`

条件付きクラスの衝突なし(`showPinned`等の分岐は`{#if}`による要素の出し分けであり、クラスの切替ではない)。静的クラスをそのままTailwindユーティリティへ変換する。

### 4. `NoteMenu.svelte`

- **`.submenu` + `class:submenu-left`**: `left`/`right`の上書きが衝突するため、三項演算子で統一する
- `.item :global(.chevron)` (Lucideアイコンへの`margin-left: auto`) は、アイコンコンポーネントに直接`class="ml-auto"`を渡す形に変換する(`:global()`セレクタは不要になる)

### 5. `ReactionUsersPopover.svelte`

- `.avatar.placeholder`は`background`のみの追加で非衝突、そのまま安全にTailwind化できる(三項演算子は不要、通常のクラス文字列選択で対応)
- `.popover`(`z-index: 1000`)も同じ理由で**`z-index: 1010`に引き上げる**

### 6. テスト

既存テストがスタイル用クラス名をDOMセレクタに使っている箇所を`data-testid`属性に切り替える(#178で確立したパターン):

- `frontend/src/ui/NoteCard.test.ts`:
  - `.reaction-wrap` → `data-testid="note-reaction-wrap"`
  - `.avatar`(2箇所、プレースホルダーとimgタグ) → `data-testid="note-avatar"`
  - `.name` → `data-testid="note-name"`
  - `.acct` → `data-testid="note-acct"`
- `frontend/src/ui/NotificationCard.test.ts`:
  - `.note-preview` → `data-testid="notification-note-preview"`
  - `.avatar` → `data-testid="notification-avatar"`(NoteCard由来の`note-avatar`と別名にし、テストの意図を明確にする)
  - `.actor` → `data-testid="notification-actor"`

`ReactionPicker.svelte`/`NoteMenu.svelte`/`ReactionUsersPopover.svelte`にはテストファイルが存在しないため、この3ファイルはテスト修正不要。

`NoteCard`は`<Self>`で自己再帰(引用Renoteのネスト表示)するため、`data-testid`が同一ページ内で複数回出現しうる。現状の`container.querySelector(".avatar")`等も同様に最初の一致を拾う挙動であり、`data-testid`化後も`querySelector`は同じセマンティクス(最初の一致)を維持するため、既存テストの前提を壊さない。

## リスクと対応

- 5ファイルにまたがる大きめのバッチだが、各ファイルは独立してレンダリングされる別コンポーネントであり、相互依存はポータル経由の呼び出し関係のみ(ロジック変更なし)。実装は1ファイルずつ順に進める
- `NoteCard.svelte`の`<script>`ロジック(ポータル/ホバー遅延/クールダウン/位置計算等の`$effect`群)は一切変更しない
- 手動確認(`cargo tauri dev`)ではタイムライン表示・引用ネスト表示・リアクション付与/取消・投票・Renote・ノートメニュー(クリップ追加)・ホバーポップオーバー・ダーク/ライト両テーマを確認する
