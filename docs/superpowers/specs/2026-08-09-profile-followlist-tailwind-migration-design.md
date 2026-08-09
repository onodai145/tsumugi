# ProfileModal/FollowListModalのTailwind移行 設計 (Issue #174 第3バッチ)

## 背景・目的

Issue #174の第2バッチ(#177、モーダル基盤: Modal.svelte/ConfirmDialog.svelte)に続き、`Modal.svelte`を使う`ProfileModal.svelte`/`FollowListModal.svelte`のTailwind移行を進める。

## 対象コンポーネント

- `frontend/src/ui/ProfileModal.svelte`(358行) — プロフィール表示モーダル。バナー/アバター/フォローボタン/統計/投稿一覧を持つ
- `frontend/src/ui/FollowListModal.svelte`(207行) — フォロー中/フォロワー一覧モーダル

両者とも`Modal.svelte`(第2バッチで移行済み)を`title`/`onclose`のpropsで使う。`ProfileModal.svelte`は`FollowListModal.svelte`をネストして使う(フォロー中/フォロワー数タップ時)。

## 方針

### `Modal`の使い方・データ取得ロジックは変更しない

`<Modal title="..." {onclose}>`の呼び出し方、`load()`/`loadMore()`/世代番号(`requestGen`)によるレース対策、`$effect`でのトリガー、`untrack`の使い方は一切変更しない。今回のタスクはスタイル(CSS)の置き換えのみを対象とする。

### ボタンのButton化方針

4種類のボタンがあり、性質によって扱いを分ける:

- **`mini-btn`(再試行ボタン、両ファイルで重複)**: `Button variant="outline" size="sm"`
- **`stat-btn`(フォロー中/フォロワー数、ProfileModalのみ)**: `Button variant="ghost" size="xs"`(元のpadding 3px/6pxに近い最小サイズ)
- **`follow-btn`(フォロー/フォロー解除のピル型トグル、ProfileModalのみ)**: `Button size="sm" class="rounded-full ..."`。`variant={isFollowing ? "outline" : "default"}`で塗りつぶし/枠線を切り替え、フォロー中はホバーで解除色(赤系)になる挙動を`class`の追加クラスで再現する
- **`row`(FollowListModalのユーザー一覧行)**: Button化しない。40pxアバター画像+2行テキストという構造がButtonプリミティブの固定高さ(`h-8`/`h-9`等)前提と合わないため、生`<button>`をTailwindユーティリティクラスで再現する

### `color-mix`パターンは`<style>`に残す(既存バッチと同じ例外)

`avatar.placeholder`、`stat-btn:hover`、`row:hover`が使う`color-mix(in srgb, var(--surface-*) var(--column-opacity, 100%), transparent)`は、レイアウトバッチ(#176)で確立した通りTailwindに変換せず`<style>`に残す。

### バナー画像の負マージンはTailwind標準ユーティリティで表現

`width: calc(100% + 32px); margin: 0 -16px;`(モーダルの`padding: 16px`を突き破って画像を左右いっぱいに広げる書き方)は、`w-[calc(100%+32px)] -mx-4`に変換する。`-mx-4`はTailwindの標準の負マージンユーティリティで、16pxと厳密に一致する(アービトラリ値ではない)。

## エラーハンドリング

CSS/レイアウトの置き換えのみのため、エラーハンドリングロジック(`profileState`のerror分岐、`notesErr`/`followErr`/`err`の表示と再試行ボタン)は一切変更しない。

## テスト

- `ProfileModal.svelte`/`FollowListModal.svelte`に既存の自動テストは無い
- `cd frontend && pnpm check`/`pnpm build`が通ることを確認する
- 手動確認(`cargo tauri dev`、リポジトリルートから起動):
  - プロフィール表示(バナーあり/なし、アバターあり/なし)
  - フォロー/フォロー解除ボタンの色切り替え(通常時の塗りつぶし、フォロー中の枠線のみ、フォロー中ホバー時の赤系ハイライト)
  - フォロー中/フォロワー数タップでの`FollowListModal`表示
  - フォロー一覧のアバター行タップでのプロフィール遷移(`openProfile`)
  - 投稿一覧の無限スクロール、再試行ボタン

## スコープ外

- `AddColumnModal.svelte`(706行、独立構造)は別バッチ
- shadcn Dialogプリミティブへの置き換えは行わない(#177の方針を継続)
