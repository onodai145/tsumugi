# MFM補完(ComposeBar) 設計書

関連: Issue #22「MFMが補完されるようにする」(Phase 1)

## スコープ

`ComposeBar.svelte` の本文textarea(投稿・返信・引用の共通入力欄)に、入力中のMFM構文をその場で補完するポップアップを追加する。

対象トリガー(Phase 1):

- 絵文字コード `:name:` — カスタム絵文字 + Unicode絵文字ショートコード
- MFM関数名 `$[name ...]`
- MFM関数の引数名 `$[name.arg ...]`
- MFM関数の引数値(列挙型のみ) `$[border.style=... ]`

対象外(Phase 2、別issue #23とは無関係の別スコープとして後日brainstorming):

- メンション `@user` 補完 — ユーザ検索APIが未実装のため
- ハッシュタグ `#tag` 補完 — ハッシュタグ検索APIが未実装のため

適用範囲は `ComposeBar.svelte` の本文textareaのみ。CW入力欄はMFMが解釈されないため対象外。他のtextarea(TQLフィルタ入力、ミュート設定)はスコープ外。

## データソース(すべて既存、新規追加なし)

- カスタム絵文字: `app.loadEmojis(accountId)` → `EmojiDef[]`(`name`, `url`, `aliases`, `category`)
- Unicode絵文字: `UNICODE_EMOJIS`(`frontend/src/lib/unicodeEmojiList.ts`、`@misskey-dev/emoji-data` 由来)
- MFM関数名: `KNOWN_FN`(`frontend/src/lib/mfm.ts`) — export追加が必要
- MFM関数の引数スキーマ: `FN_ARGS`(新規、`frontend/src/lib/mfm.ts` に追加。`mfmFn` の実装と1対1対応させる)

```ts
type MfmArgSpec = { name: string; hasValue: boolean; enum?: string[] };

const FN_ARGS: Record<string, MfmArgSpec[]> = {
  tada:    [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  jelly:   [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  twitch:  [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  shake:   [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  jump:    [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  bounce:  [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  rainbow: [{ name: "speed", hasValue: true }, { name: "delay", hasValue: true }],
  spin: [
    { name: "speed", hasValue: true }, { name: "delay", hasValue: true },
    { name: "x", hasValue: false }, { name: "y", hasValue: false },
    { name: "left", hasValue: false }, { name: "alternate", hasValue: false },
  ],
  flip: [{ name: "h", hasValue: false }, { name: "v", hasValue: false }],
  x2: [], x3: [], x4: [], blur: [],
  font: [
    { name: "serif", hasValue: false }, { name: "monospace", hasValue: false },
    { name: "cursive", hasValue: false }, { name: "fantasy", hasValue: false },
    { name: "emoji", hasValue: false }, { name: "math", hasValue: false },
  ],
  rotate: [{ name: "deg", hasValue: true }],
  position: [{ name: "x", hasValue: true }, { name: "y", hasValue: true }],
  scale: [{ name: "x", hasValue: true }, { name: "y", hasValue: true }],
  fg: [{ name: "color", hasValue: true }],
  bg: [{ name: "color", hasValue: true }],
  border: [
    { name: "color", hasValue: true },
    { name: "style", hasValue: true, enum: ["hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset", "outset"] },
    { name: "width", hasValue: true },
    { name: "radius", hasValue: true },
    { name: "noclip", hasValue: false },
  ],
};
```

`enum` を持たない値引数(`color`, `speed`, `delay`, `deg`, `x`, `y`, `width`, `radius`)は自由入力のため値補完はしない。

## トリガー検出

新規モジュール `frontend/src/lib/mfmCompletion.ts` に純粋関数として実装する(DOM非依存、単体テスト容易)。

### `detectTrigger(text: string, cursor: number): Trigger | null`

`text.slice(0, cursor)` を対象に後方から解析する。

```ts
type Trigger =
  | { kind: "emoji"; query: string; start: number; end: number }
  | { kind: "fnName"; query: string; start: number; end: number }
  | { kind: "argName"; fnName: string; query: string; start: number; end: number }
  | { kind: "argValue"; fnName: string; argName: string; query: string; start: number; end: number };
```

1. カーソル直前から後方に `$[` を探す。ただし、その `$[` とカーソルの間に `]` が現れる場合はすでに閉じられているので対象外(＝直近の未クローズ `$[` のみを見る)。
2. 未クローズの `$[` が見つかった場合、`$[` 直後からカーソルまでの区間を `seg` とする。
   - `seg` に空白(半角スペース/タブ/改行)が含まれる場合 → 本文コンテンツに入っているため fn系トリガーなし。3.へ(絵文字トリガーの判定のみ行う)。
   - `seg` に空白が含まれない場合:
     - `seg` に `.` が無い → `{ kind: "fnName", query: seg, start: $[直後, end: cursor }`。ただし `seg` が識別子文字(`[a-zA-Z0-9_]`)以外を含む場合は `null`。
     - `seg` に `.` がある → 最後の `,` 以降の部分文字列 `argSeg` を取る(`,` が無ければ `.` 以降全体)。fnName は `.` より前の部分。
       - `argSeg` に `=` を含まない → `{ kind: "argName", fnName, query: argSeg, start, end }`
       - `argSeg` に `=` を含む → `argName = argSeg.split("=")[0]`, `valueQuery = argSeg.split("=")[1]`。`FN_ARGS[fnName]` から `argName` の spec を引き、`enum` があれば `{ kind: "argValue", fnName, argName, query: valueQuery, start, end }`、無ければ `null`。
3. 未クローズの `$[` が見つからない場合、またはfn区間だが空白到達後の場合: カーソル直前の絵文字コード `:` を正規表現 `/(?:^|[\s([{"'>])(:[a-zA-Z0-9_+-]*)$/` で後方一致させる。マッチすれば `{ kind: "emoji", query: マッチ部分(":"除く), start, end: cursor }`。

fn区間内でも絵文字トリガーは独立して評価する(コンテンツ中の絵文字も補完対象のため、`$[tada :sm` のようなケースでも動く)。

### マッチング関数

- `matchEmojis(query, customEmojis): EmojiMatch[]` — カスタム絵文字(`name`/`aliases` 前方一致)を先に、Unicode絵文字(`name` 前方一致)を後に、それぞれ名前順。合計最大10件。
- `matchFnNames(query): string[]` — `KNOWN_FN` を前方一致・名前順、最大10件。
- `matchArgNames(fnName, query): MfmArgSpec[]` — `FN_ARGS[fnName]` を前方一致・定義順、最大10件。
- `matchArgValues(fnName, argName, query): string[]` — 該当 `enum` を前方一致、定義順、最大10件。

## UI: `CompletionPopover.svelte`(新規)

絵文字/関数名/引数名/引数値で共通のドロップダウン。

- textarea内のカーソル位置にポップアップを追従表示する。座標計算は非表示ミラーdiv方式(textareaのpadding/font/whiteSpace等をコピーしたdivにトリガー開始位置までのテキストを流し込み、末尾に置いたマーカー要素の位置を取得する定番手法)。
- 表示は最大10件、縦リスト。
  - 絵文字行: サムネイル(カスタムは `<img>`、Unicodeは既存の `UnicodeEmoji` コンポーネント)+ 名前
  - fn名/引数名/引数値の行: 名前テキストのみ
- キー操作: `↑`/`↓` で選択移動(端でループしない)、`Tab`/`Enter` で確定、`Escape` で閉じる。`Ctrl+Enter`(投稿ショートカット)はポップアップの有無に関わらず常に投稿を優先する。
- マウスクリックでも確定可能。
- 候補が0件になったら自動的に閉じる。textareaがフォーカスを失ったら閉じる。

## `ComposeBar.svelte` への配線

- `oninput` および矢印キー等によるカーソル移動時に `detectTrigger(text, textarea.selectionStart)` を呼び、結果に応じて候補を計算しポップアップの表示状態を更新する。
- 絵文字候補は `app.loadEmojis(accountId)` を都度参照(既にロード済みならキャッシュから同期的に取れる、未ロードならロード完了後に再評価)。
- 確定時の置換(`trigger.start`〜`trigger.end` を対象にテキストを組み立て直す):
  - `emoji`: `:name:` に置換、カーソルは置換後の直後
  - `fnName`: `name` に置換(挿入前後の `$[` はそのまま活かす)、カーソルは直後。**引数を続けたい場合はユーザが `.` を続けて入力する**(自動でスペースは付与しない — `$[tada]` のような引数無し関数も多いため)
  - `argName`(`hasValue: true`): `name=` に置換、カーソルは `=` の直後
  - `argName`(`hasValue: false`): `name` に置換、カーソルは直後
  - `argValue`: `name` (enum値そのもの)に置換、カーソルは直後
- `onkeydown` で、ポップアップ表示中は `ArrowUp`/`ArrowDown`/`Tab`/`Enter`(Ctrl/Meta無し)/`Escape` をpreventDefaultしてポップアップ側の操作にルーティングする。それ以外のキーは通常通りtextareaに渡す。

## テスト方針

- `mfmCompletion.ts` の単体テスト(Vitest): `detectTrigger` の境界ケース(空白到達で停止、ネストした `$[` 非対応の確認、絵文字トリガーの単語境界判定)、`matchEmojis`/`matchFnNames`/`matchArgNames`/`matchArgValues` の前方一致・優先順位・件数上限。
- `CompletionPopover.svelte` のコンポーネントテスト(Testing Library、直近導入した方針に準拠): 候補表示、キーボード操作での選択移動・確定・Escで閉じる、クリック確定。
- `ComposeBar.svelte` への配線は既存のcomposeBarテスト(あれば)に統合、無ければ最小限のcase追加(トリガー文字入力→ポップアップ表示→確定→本文置換、の一連の流れ)。

## 非対応・既知の制約

- ネストした `$[` (`$[tada $[jelly ...]]`)は、外側のfn区間検出が「直近の未クローズ `$[`」を見るため、内側の `$[` に入った時点でそちらが対象になる。外側への戻り補完はサポートしない(通常の入力順序であれば問題にならない)。
- 引数値の自由入力項目(色コード・時間・数値)は補完しない。
- IME変換中(`compositionstart`〜`compositionend`)の挙動: `compositionend` 後に再評価する(変換中に候補を出すとIMEと競合するため、`isComposing` 中はトリガー検出をスキップする)。
