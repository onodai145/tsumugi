# nyaize前の文字列をコピーできるようにする（Issue #168）

## 背景・目的

`isCat` なユーザの投稿は `nyaize()`（`frontend/src/lib/nyaize.ts`）により本文が「にゃん語化」されて表示される（例: 「こんな」→「こんにゃ」）。
現状、画面上でこのテキストを選択してコピーすると、にゃん語化後の文字列がクリップボードに入る。ユーザは元の（変換前の）文字列をコピーしたい。

表示は現状どおりにゃん語化したまま、選択範囲コピー（Ctrl+C／右クリックコピー）だけを透過的に元テキストへ差し替える。

## スコープ

- 対象: `NoteCard.svelte` で `nyaize` が有効な2箇所（CWテキスト、本文テキスト）。
- nyaize は `MfmNode.svelte` 内でテキストノード（`node.type === "text"`）と `ruby` の base/rt にのみ適用されている。他のノード種別（link/quote/plain の中身、mention、hashtag、code等）はそもそも `disableNyaize` でnyaize対象外＝表示文字列＝元文字列なので、追加対応不要。
- 対象外（将来の拡張候補として明示するが今回は実装しない）:
  - リアクションツールチップ等、Mfmを介さない箇所でのnyaize表示（現状nyaizeを適用していないため対象外）。
  - コンテナ境界をまたぐ選択（例: 複数ノートを跨いだドラッグ選択）はブラウザデフォルトの挙動にフォールバックする。

## 設計

### 1. 元テキストの保持（`MfmNode.svelte`）

テキストノード描画部分（62-64行目）と ruby base/rt（48-51, 80行目）を、nyaize適用時のみ `data-original-text` 属性を持つ `<span>` でラップする。`display`はデフォルトの`inline`のままなので、レイアウト・改行(`<br>`)挙動に影響しない。

```svelte
{#if node.type === "text"}
  {@const original = String(p.text ?? "")}
  {@const text = shouldNyaize ? nyaize(original) : original}
  {#if shouldNyaize}
    <span data-original-text={original}>{#each text.split("\n") as line, i}{#if i > 0}<br />{/if}{line}{/each}</span>
  {:else}
    {#each text.split("\n") as line, i}{#if i > 0}<br />{/if}{line}{/each}
  {/if}
```

ruby base/rt も同様に、`shouldNyaize` が true のときだけ `data-original-text` でラップする。

### 2. にゃん語化前後の文字対応表（`frontend/src/lib/nyaizeCopy.ts`、新規）

```ts
export function buildNyaizeCharMap(original: string, nyaized: string): number[]
```

`nyaized` の各文字が `original` の何文字目に対応するかを表す配列（長さ = `nyaized.length`）を、編集距離のDP（Wagner-Fischer、`match / substitute / insert` の3操作、`delete` は基本発生しない想定だが安全のため許容）で構築する。`nyaize()` の変換は原文字の削除や並べ替えを行わない（`な→にゃ`のような1→2文字展開、`다→다냥`のような末尾追加のみ）ため、このDPで実用上ほぼ確実に正しい対応が取れる。

対応する値は単調非減少（元テキストの文字順は保持される）。

```ts
export function mapNyaizedRangeToOriginal(
  map: number[],
  original: string,
  start: number,
  end: number,
): string
```

`nyaized` 側の `[start, end)` 範囲を、`map` を使って `original` の対応区間にマッピングし、その部分文字列を返す。空範囲・境界外は空文字列を返す。

### 3. copyイベントハンドラ（`frontend/src/lib/nyaizeCopy.ts`）

```ts
export function handleNyaizeCopy(event: ClipboardEvent): void
```

- `event.currentTarget` をコンテナ要素とし、`window.getSelection()` の `Range` を取得。選択が空、またはコンテナ外にまたがる場合は何もせず終了（デフォルトのコピー動作に委ねる）。
- コンテナ配下を `TreeWalker`（`SHOW_TEXT | SHOW_ELEMENT`）で走査し、`Range` と交差するノードのみ処理:
  - テキストノード: 祖先に `[data-original-text]` があれば、そのノードの選択済みオフセット範囲を `buildNyaizeCharMap` + `mapNyaizedRangeToOriginal` で元文字列に変換して結果に追加。祖先が無ければ（nyaize対象外）選択済み部分文字列をそのまま追加。
  - `<br>` 要素: 結果に `"\n"` を追加。
- 結果を結合し `event.clipboardData?.setData("text/plain", result)` してから `event.preventDefault()`。

### 4. 呼び出し側（`NoteCard.svelte`）

CWテキストのspan（329行目）と本文テキストのdiv（338行目）に `oncopy={handleNyaizeCopy}` を追加する。

## テスト

- `frontend/src/lib/nyaizeCopy.test.ts`（新規）:
  - `buildNyaizeCharMap` / `mapNyaizedRangeToOriginal`: 「こんな」→「こんにゃ」のような1→2文字展開を含むケースで、にゃん語化後の任意の部分選択が正しい元テキスト区間にマップされることを検証。
  - `handleNyaizeCopy`: `@testing-library/svelte` の `fireEvent.copy` + `clipboardData` をモックしたClipboardEventで、`Mfm.svelte`（`nyaize: true`）をレンダーし、テキストを選択してコピーした際に元の文字列が `setData` に渡ることを検証。
- 既存の `Mfm.test.ts` / `nyaize.test.ts` は変更不要（表示上のにゃん語化ロジック自体は変更しない）。

## 影響範囲

- `frontend/src/render/MfmNode.svelte`: text/ruby分岐にラップ用spanを追加（表示・既存テストへの影響なし、DOM構造にinline spanが1つ増えるのみ）。
- `frontend/src/lib/nyaizeCopy.ts`: 新規ファイル。
- `frontend/src/ui/NoteCard.svelte`: `oncopy` ハンドラを2箇所に追加。
