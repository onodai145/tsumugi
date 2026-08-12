# AccountSelect.svelte Tailwind移行 設計

## 背景

Issue #174(既存コンポーネントのTailwindクラスへの移行)の一環。`frontend/src/ui/AccountSelect.svelte`(アカウント切替ドロップダウン)を手書きCSSからTailwindユーティリティクラスへ移行する。

## 対象コンポーネント

`frontend/src/ui/AccountSelect.svelte`(131行、`<style>`ブロック除く)。

使用箇所は2箇所:
- `AddColumnModal.svelte:388` — `<AccountSelect bind:value={accountId} accounts={app.accounts} showLabel disabled={isEdit} />`(`showLabel`=true、`large`未指定=false。フォーム内の全幅トリガー)
- `ComposeBar.svelte:547` — `large={!expanded}`(`showLabel`未指定=false。投稿欄横の目立つトリガー)

`showLabel`と`large`が同時にtrueになる組み合わせは現状の呼び出し元に存在しない。

## 方針

### トリガーボタン

既存の`<button class="trigger" class:full={showLabel} class:large>`を`Button`プリミティブ(`frontend/src/lib/components/ui/button/button.svelte`)に変換する。

- `variant="outline"`
- `size={large ? "lg" : "sm"}`(largeはButtonの最大トークン`lg`=40pxに寄せる。ただしButtonは`border`(1px)を持つためコンテンツ領域は38px相当になる。元の44pxアバターはコンテンツ領域に収まる36px相当`size-9`に縮小する)
- `showLabel`時は`class="w-full justify-start"`を追加(Buttonの`class`propは`cn()`でマージされるため、`variant`のデフォルトクラスと衝突せず安全に上書きされる)
- `bind:ref={trigger}`(`trigger`は既に`$state<HTMLElement | null>(null)`で正しく初期化済み — ComposeBarで踏んだ`bind:ref={undefined}`バグの対象外)
- disabled時のhover打ち消しは個別に再現しない。Buttonの`disabled:pointer-events-none`によりdisabled中はそもそもhoverイベントが発火しないため(VisibilitySelect/Dropdownの移行時と同じ扱い)

キャレット(`▾`)は`{#if !large}`のまま維持(largeモードでは非表示)。`showLabel`時のみ`ml-auto`を追加してキャレットを右端に押し出す。

### アバター/プレースホルダー

`large`の真偽で三項演算子によりクラス出し分け(通常22px相当 `size-[22px]` / large 40px相当 `size-10`)。角丸・フォントサイズも同様に出し分ける。

### ドロップダウンメニュー

- オーバーレイ: `fixed inset-0 z-[1010]`(既存の統一ルールに合わせて`1000`→`1010`)
- メニュー本体: 位置は`style`のインラインスタイル(動的な`left`/`top`/`min-width`)のまま、外観のみTailwindクラス化
- メニュー項目(`.item`)は既存パターン(CompletionPopover/Dropdown/VisibilitySelect)通り、raw `<button>`のまま維持(フル幅リスト行のため)。`active`状態は三項演算子で単一クラス文字列を出し分け、`color-mix(in srgb, var(--accent) 16%, transparent)`による背景色のみ`<style>`ブロックに残す(`active`というフックのクラス名は維持)

### 対象外/変更なし

- `portal`関数、`toggle`/`choose`のロジック、`pos`計算ロジックは変更しない
- コンポーネントの公開props(`value`/`accounts`/`showLabel`/`showHost`/`disabled`/`large`)は変更しない

## テスト方針

このコンポーネントに既存の自動テストはない(Svelteコンポーネントの見た目のみの変更で、プロジェクト全体でも同様のUIコンポーネントに単体テストは書かれていない)。`pnpm check`(svelte-check+tsc)がグリーンであることを確認する。

過去の教訓より、`pnpm check`のグリーンだけでは`bind:ref`の`undefined`初期化バグのようなランタイム限定の不具合を検出できないため、実装後に該当箇所(`trigger`変数の型・初期値)を目視で再確認する。
