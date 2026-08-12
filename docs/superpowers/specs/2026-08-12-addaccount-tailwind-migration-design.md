# AddAccount.svelte Tailwind移行 設計

## 背景

Issue #174(既存コンポーネントのTailwindクラスへの移行)の一環。`frontend/src/ui/AddAccount.svelte`(アカウント追加/再認証フォーム)を手書きCSSからTailwindユーティリティクラスへ移行する。

## 対象

`frontend/src/ui/AddAccount.svelte`(75行、`<style>`ブロック除く)。`class:`ディレクティブや状態依存の複雑な条件分岐は無い。

## 方針

### レイアウト系

`.add-account` → `mx-auto my-12 max-w-[420px] rounded-[14px] border border-border bg-background p-6`
`.head` → `mb-2 flex items-center justify-between`
`h2` → `m-0 text-[1.1rem]`
`.hint` → `mb-3.5 text-[0.86rem] text-muted-foreground`
`.form` → `flex gap-2`
`.err` → `mt-3 text-[0.85rem] break-words text-destructive`

### `.close`(戻る×)

`Button variant="ghost" size="icon-xs"`に変換する(`Modal.svelte`の閉じるボタンと統一)。

### ホスト名入力欄

既存規約(`DataSection.svelte`)に合わせ`rounded-lg border border-border bg-muted px-2.5 py-2 font-[inherit] text-foreground`とし、`flex-1`を追加してフォーム行内で伸縮させる。

### アクションボタン

「認可ページを開く」「認可を完了した」→ `Button variant="default" size="default"`(元は`background: var(--accent); color: white`で`Button`のdefaultバリアントと一致)。`size`は`default`(`h-9`=36px)を選ぶ — 「認可ページを開く」は隣のホスト名入力欄(`px-2.5 py-2`+ボーダーで実測36〜38px相当)と高さを揃える必要があり、`sm`(`h-8`=32px)では入力欄より低くなってしまうため。

「もう一度試す」「やり直す」(元`.link`)→ `Button variant="ghost" size="sm"`。元は`color: var(--text-dim)`で、hoverでの色変化は無かった。Buttonの`class`propに`text-muted-foreground hover:text-muted-foreground`を追加する — `text-muted-foreground`だけでは`ghost`バリアントの`hover:text-foreground`がtailwind-mergeで打ち消されずhover時にコントラストが変わってしまうため、hover状態も明示的に上書きする。

## テスト方針

既存の自動テストはない。`pnpm check`と`pnpm build`のグリーンを確認する。このファイルは`--column-opacity`等の可変CSS変数を使わないため、コンパイル後CSSのgrep確認は変数依存の任意値クラス(`max-w-[420px]`等)についてのみ行う。
