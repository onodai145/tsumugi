# レイアウト系コンポーネントのTailwind移行 設計 (Issue #174 第1バッチ)

## 背景・目的

Issue #170でTailwind CSS v4 + shadcn-svelteの基盤(`@theme`トークンブリッジ、`.dark`クラス切替)を導入したが、既存39コンポーネントのスタイルはまだ手書きCSS(`<style>`ブロック、`--surface-*`等のCSS変数直接参照)のままで、見た目は移行前と変わっていない。Issue #174はこれを画面領域ごとに分割して段階的にTailwindユーティリティクラスへ移行する取り組みで、本設計はその第1バッチとして、他の全コンポーネントの土台となる**レイアウト系**(`Column.svelte`, `Pane.svelte`, `Backstage.svelte`)を対象にする。

## 対象コンポーネント

- `frontend/src/ui/Column.svelte`(302行) — カラム本体、タブバー、ノート/通知一覧、幅リサイズハンドル
- `frontend/src/ui/Pane.svelte`(90行) — カラムの行/列分割レイアウト(再帰コンポーネント)
- `frontend/src/ui/Backstage.svelte`(237行) — 画面下部の操作ログパネル

## 方針

### 静的スタイル → Tailwindユーティリティクラス

`padding`/`border`/`color`/`flex`/`gap`など、値が固定またはpropsのbool分岐で決まるスタイルは、`class:xxx={cond}`ディレクティブと組み合わせてTailwindユーティリティクラスに置き換える。`<style>`ブロックは、以下の「Tailwindで表現できない部分」だけを残す形に縮小する。

### ランタイム計算値はstyle属性のまま維持

以下は現状通り`style`属性(JS文字列組み立て)で維持し、Tailwindクラスへの変換を試みない:

- `Column.svelte`の`width:${group.width}px`(ドラッグリサイズの結果)、`stretch`/`group.auto`によるflex切り替え
- `Pane.svelte`の`flex:0 0 ${child.size}px`/`flex:0 0 ${child.size}%`(ネストしたSplitのサイズ配分)

理由: Tailwindは静的ユーティリティクラス中心の設計であり、実行時に変化する任意の数値をクラス名として動的生成するのは可読性・保守性を下げる。

### `color-mix`による背景不透明度はstyleに残す(全バッチ共通の例外)

`background: color-mix(in srgb, var(--surface-1) var(--column-opacity, 100%), transparent);`のような、ユーザー設定(`--column-opacity`)に応じた背景の透過表現は、Tailwindに対応するユーティリティが無いため`<style>`に残す。この記法は`Column.svelte`以外にも`NoteCard.svelte`/`ProfileModal.svelte`/`FollowListModal.svelte`/`MediaGrid.svelte`など複数コンポーネントで使われており、今後の全バッチに共通する既知の例外として扱う(バッチごとに再検討しない)。

### `data-*`属性によるスタイル分岐はTailwindのdata-variantに置き換え

`Column.svelte`の`.tab-dot[data-state="connected"] { background: var(--success); }`、`Backstage.svelte`の`.ic[data-level="error"] { color: var(--danger); }`のようなパターンは、Tailwindの`data-[state=connected]:bg-success`のようなアービトラリdata-variant記法に置き換える(`--color-success`等は既存の`@theme`ブリッジで利用可能)。

### shadcn Buttonプリミティブをこのバッチから導入

`Column.svelte`(タブ追加/タブ閉じる/分割)、`Backstage.svelte`(ログ開閉/クリア/再認証)にある素の`<button>`要素を、shadcn-svelteのButtonプリミティブに置き換える。

**導入前の検証手順(必須)**: Issue #170のTask 2で、shadcn-svelte CLIの"vega"プリセットが想定外のハードコードカラーを注入する事故があった。`components.json`の`"style": "vega"`はそのままのため、`shadcn-svelte add button`を実行して生成されるコンポーネントのCSS変数参照(`bg-primary`等)が既存の`@theme`ブリッジ(`--color-primary: var(--accent)`等)と整合するかを確認してから採用する。整合しない参照(未定義の`--color-*`トークン)が見つかった場合は、生成後のコンポーネントコードを手直しするか、`@theme`ブリッジ側にトークンを追加する。

## エラーハンドリング

CSS/レイアウトの置き換えのみのため、エラーハンドリングロジックの変更は無い。ドラッグ&ドロップ、リサイズ、スクロール検知(`onScroll`)などの既存の振る舞いは一切変更しない。

## テスト

- `Column.svelte`/`Pane.svelte`/`Backstage.svelte`に既存の自動テストは無い(store/振る舞い中心のテストのみ)。新規にコンポーネントテストを追加する要求は無し
- `cd frontend && pnpm check`(svelte-check + tsc)が通ることを確認する
- `cd frontend && pnpm build`が通ることを確認する
- 手動確認(`cargo tauri dev`、リポジトリルートから起動):
  - カラムのタブ切替、タブのドラッグ並び替え、タブ追加/削除、下分割
  - カラム幅のドラッグリサイズ、`auto`カラムの均等割り
  - 設定→表示のカラム不透明度を変更し、背景の透過が反映されること
  - フォーカス中カラムのタブバー上端ハイライト
  - タブの接続状態ドット(connected/connecting/reconnecting/error)の色分け
  - Backstageのログ開閉、エラーバッジ表示、ログレベルごとのアイコン色、再認証ボタン
  - 追加したButtonプリミティブの見た目・クリック動作に既存との差異が無いこと

## スコープ外

- `Pane.svelte`が`svelte:self`を使っている点(deprecation警告)は本設計の対象外(別途対応)
- レイアウト系以外のバッチ(モーダル群/ノート・通知表示/入力系/設定画面)は別設計・別実装計画で扱う
