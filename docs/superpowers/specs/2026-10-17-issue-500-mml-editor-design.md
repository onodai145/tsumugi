# MML 入力欄のエディタ化の設計

Issue #500(関連: #75 / #494 / PR #498、#497)

## 目的

設定の通知音「MML（打ち込み）」の入力欄は、等幅の `<textarea>` と、その下の状態行 1 本だけである(`frontend/src/ui/settings/SoundChoicePicker.svelte`)。上限が 90 秒・5000 文字・3000 音になり(PR #498)音色(`@`)も増えて、長い MML を扱う場面が増えた。入力欄をエディタとして使いやすくする。

成功条件:

- 音符・休符・コマンド・音色・数字・繰り返し・トラック区切りが色分けされる。
- 構文エラーのとき、該当位置に下線が引かれ、状態行の操作でその位置へカーソルを移せる。
- 文字数・音数・トラック数・再生時間が、上限と並んで見える。上限に近づくと分かる。
- 編集領域に、スクリーンリーダー向けの名前と状態が付く。
- 既存の保存形式(`mml:<テキスト>`)と、保存・試聴の挙動は変わらない。
- WebKitGTK(Linux)と Android WebView で、入力・IME・ソフトキーボードが壊れない。

## 範囲

今回やる: 構文ハイライト、エラー下線、エラー位置への移動、使用量の表示、a11y(`aria-label` など)。

今回やらない(別 Issue / PR): 音色・サンプルの挿入、生成中の表示と中断、入力補助(音名キーボードなど)。#500 は残項目のためにクローズしない。

## 決定事項

| 決定 | 内容 | 理由 |
|---|---|---|
| 実現方法 | CodeMirror 6 を導入する | ユーザー選択。下線・キー操作・選択範囲の扱いが標準で揃う |
| 依存 | `@codemirror/state` / `@codemirror/view` / `@codemirror/commands` のみ | `@codemirror/language`(Lezer)と `@codemirror/lint` は使い切れない。エラーは 1 件、文法は小さい |
| ハイライト | 自前の字句解析 + `ViewPlugin` + `Decoration.mark` | パーサーの再実装はしない。構文エラーでも途切れない寛容な字句解析にする |
| 読み込み | `MmlEditor` を動的 `import()` する | `tone` と同じ。設定画面で MML モードを選ぶまでバンドルに乗せない |
| 配色 | 新しいトークンは作らない。既存トークンの `color-mix` + 太字で区別する | カスタムテーマ(`ThemeColors`)の対応を増やさない。style-guide §10 により `--success` / `--info` は流用しない。色だけに頼らない |
| 上限の定数 | `lib/mmlLimits.ts` に Rust と同じ値を置く | `mmlTimbres.ts` と同じく、両側のテストが同じ表を持つ |
| 高さ | 最小 3 行、内容に応じて伸びる。内側スクロールなし | モバイルで二重スクロールを避け、`--keyboard-inset` は既存のモーダルに任せる |

## 前提(コードの現状)

- 状態は `SoundChoicePicker.svelte` の `mmlText` と `status`(`idle` / `pending` / `ok` / `error` / `failed`)。入力が止まって 300ms 後に `commands.parseMml(text)` を呼び、古い結果は捨てる。この仕組みは変えない。
- `parseMml` の結果は `parsed { score: { notes, totalSec, trackCount } }` か `invalid { error: { position, message } }`。
- `position` は**入力テキストの文字位置**(Rust の `chars()` の添字、0 始まり、空白を含む)。JS の文字列添字(UTF-16)とは、サロゲートペアがあるとずれる。5000 字超過のエラーは `position = 5000`(文書の長さ)で返る。
- パーサーは**空白を除き、小文字化**してから読む。したがって `@bell c` と `@bellc` は同じ入力になり、音色名は最長一致で読まれる。字句解析も同じ前提で読む。
- `SoundChoicePicker` の利用箇所は `NotifySection` と `AddColumnModal`(2 か所)。`value` / `allowInherit` / `onvalidity` の props は変えない。

## 設計

### 1. 部品

| 部品 | 種別 | 責務 |
|---|---|---|
| `lib/mmlLexer.ts` | 純関数 | テキスト → `{ from, to, kind }[]`。`kind` は `note` / `rest` / `command` / `timbre` / `number` / `repeat` / `separator` / `unknown` |
| `lib/mmlUsage.ts` | 純関数 | テキストと直近の成功結果 → 使用量(値・上限・段階 `normal` / `near` / `over`) |
| `lib/mmlLimits.ts` | 定数 | 5000 文字・3000 音・4 トラック・90 秒 |
| `lib/mmlPosition.ts` | 純関数 | Rust の文字位置 → エディタの文字列添字(UTF-16)。範囲外は文書末尾に丸める |
| `ui/settings/MmlEditor.svelte` | コンポーネント | CM6 を包む。`bind:value`、`errorPosition`、`ariaLabel`、`describedBy`、`invalid` を受け取り、`focusAt(pos)` を公開する |

`SoundChoicePicker.svelte` は `<textarea>` を `MmlEditor` に置き換え、状態行・使用量行・試聴ボタン・記法ヘルプはそのまま持つ。

### 2. 字句解析

- 空白を除いた記号列(文字と元の位置)を作り、それを分類する。トークンの範囲は、最初の記号の位置から最後の記号の位置 + 1 まで。途中の空白もそのトークンに含まれる。
- 分類:

| 種別 | 対象 |
|---|---|
| `note` | `a`〜`g` と直後の `#` `+` `-` |
| `rest` | `r`、タイ `&` |
| `command` | `o` `l` `t` `v` `<` `>` |
| `timbre` | `@` と直後の番号、または既知の音色名(`TIMBRE_LIST` の最長一致) |
| `number` | 上記に続く数字、付点 `.` |
| `repeat` | `[` `]` と、`]` 直後の回数 |
| `separator` | `;` |
| `unknown` | 上のどれでもない文字 |

- 字句解析は構文を検証しない。検証は `parseMml` だけが行う。

### 3. 配色

| 種別 | 見た目 |
|---|---|
| `note` | `--text`(そのまま) |
| `rest` | `--text-dim` |
| `command` | `--accent` |
| `timbre` | `--accent` を `--text` と 50% 混ぜた色、太字 |
| `number` | `--text` と `--accent` の混色(約 35%) |
| `repeat` | `--text`、太字 |
| `separator` | `--text-dim`、太字 |
| `unknown` | 装飾しない(エラー下線が担う) |

エラー下線は `--danger` の波線(`text-decoration: underline wavy`)。`--danger` は既存の `text-destructive` と同じ「エラー」の意味で使う。

### 4. エラー下線と移動

- 下線は `status.kind === "error"` で、かつ解析した時点の文字列と現在の文字列が同じときだけ引く。編集して `pending` に入った瞬間に消す(位置がずれるため)。
- 位置は `mmlPosition` で UTF-16 の添字にする。文書の長さ以上なら最後の 1 文字に引く。空文書には引かない。下線は 1 文字分(Rust 側は「その位置から」しか返さないので、範囲は推測しない)。
- 状態行の「N 文字目: …」を `<button type="button">` にする。押すと `MmlEditor.focusAt(pos)` が、カーソルをその位置に置き、見える位置までスクロールする。見た目はリンク風で、フォーカス表示は `Button` と同じ `focus-visible` のパターン(style-guide §7)。ショートカットキーは付けない。

### 5. 使用量

状態行の下に 1 行: `文字 1234/5000 · 音 120/3000 · トラック 2/4 · 再生 12.3/90 秒`

- 文字数は `[...text].length`(Rust の `chars().count()` と同じ)。トラック数は `;` で分けた空でない区間の数(パーサーと同じ)。どちらも入力のたびに数える。
- 音数と再生時間は `parseMml` 成功時の値。`pending` の間は前回の値を保つ(300ms ごとの点滅を避ける)。`error` と `failed` のときは「—」。
- 段階: 上限の 90% 以上は `near`(`--warning`、太字)、超過は `over`(`--danger`、太字)。
- 状態行(`data-testid="mml-status"`)の既存の文言と挙動は変えない。

### 6. a11y

- 編集領域に `role="textbox"`、`aria-multiline="true"`、`aria-label="MML"` を付ける。
- `aria-describedby` は状態行と使用量行を指す。構文エラーのとき `aria-invalid="true"`。
- `autocapitalize="off"` `autocorrect="off"` `spellcheck="false"`(ソフトキーボードが音符を大文字にしたり自動修正したりするのを防ぐ)。
- Tab キーは編集領域に取り込まず、フォーカスを次へ送る。

## テスト

- `mmlLexer`: 種別ごとの分類、構文エラー入力でも途切れないこと、大文字小文字、空白を挟んだ入力、音色名の最長一致。
- `mmlUsage`: 90% の境界、トラック数の数え方(空区間、末尾の `;`)、`pending` で前回値を保つこと。
- `mmlPosition`: サロゲートペアを含む文字列、範囲外、空文書。
- `mmlLimits`: Rust の定数と同じ値であること(両側のテストが同じ表を持つ)。
- `SoundChoicePicker.test.ts`: `<textarea>` を前提にしているので書き換える。CM6 の `EditorView` に dispatch で入力する。jsdom で補えない操作(キャレット位置、描画された下線の位置)は、実画面の確認に回す。

## 実画面の確認

自動テストでは済ませない。

- Xvfb と隔離した D-Bus 上で `cargo tauri dev` を起動し(ユーザーの実画面には出さない)、入力・IME・下線の位置・エラー位置への移動を確認する。
- 設定画面と `AddColumnModal` の両方で、Escape / Enter がモーダルの操作と衝突しないことを確認する。
- Android のエミュレーターで、`adb shell input tap` の本物のタップによる入力、ソフトキーボードの自動修正、`--keyboard-inset` との相性を確認する。

## リスクと撤退条件

- WebKitGTK か Android WebView で、入力が壊れる、またはキャレットがずれることが確認できたら、そこで止める。実現方法(自前の `<textarea>` 重ね描き)の再検討を提案し、この spec を更新する。
- バンドルの増分は、実装後に `vite build` の出力で測って記録する。

## 影響範囲

- 追加: `lib/mmlLexer.ts` / `mmlUsage.ts` / `mmlLimits.ts` / `mmlPosition.ts`(とそれぞれのテスト)、`ui/settings/MmlEditor.svelte`。
- 変更: `ui/settings/SoundChoicePicker.svelte`(+ テスト)、`frontend/package.json`(依存 3 つ)、`docs/guide/user-guide.md`(入力欄の説明)。
- Rust 側: 挙動の変更なし。`MAX_*` 定数をフロントの表と同じ値に固定するテストを `mml.rs` に 1 つ足すだけ。
- `frontend/vite.config.ts`: CodeMirror の 3 パッケージを `optimizeDeps.include` に足す(動的 import の依存は、実行時の再最適化でフルリロードを起こすため。`tone` と同じ扱い)。

## 実装後の確認結果

2026-10-17、`feat/issue-500-mml-editor` のコミット時点。

### バンドル

`vite build` で `MmlEditor` が独立した chunk になった(`dist/assets/MmlEditor-*.js`、272.54 kB、gzip 89.37 kB)。`cm-content` を含む chunk はこれだけで、メインの chunk には CodeMirror が入っていない(動的 import が効いている)。実装前のメインの chunk の大きさとの比較はしていない。

### デスクトップ(WebKitGTK 2.54.1、Xvfb 上の隔離インスタンス)

ダミーのアカウントを登録した隔離インスタンス(Xvfb + `dbus-run-session`、実画面には出していない)で、設定→通知の MML 入力欄を、`xdotool` の本物のキー入力とクリックで確認した。

確認できたこと:

- 入力欄が表示され、プレースホルダーが出る。入力した文字が色分けされる(コマンド・音色・繰り返し・区切り・数字)。ライトとダーク(`.dark` クラスを付けた状態)の両方で読める。
- 構文エラーの文字に波線が付く(「33 文字目」のエラーで、33 文字目の `&` に付いた)。
- 「N 文字目」を実クリックすると、その位置にカーソルが移る(クリック後に `Z` を打つと、エラーの文字の直前に入った)。
- 5001 文字を入れると、状態行に「5001 文字目: MML は 5000 文字以下にしてください」が出て、使用量の文字数が `over` になり、末尾の文字に波線が付く。入力欄は内側でスクロールせず、外側のモーダルだけがスクロールする。
- 構文が通る 2 トラックの MML で、「再生時間 6.0 秒」と使用量(文字 55/5000、音 14/3000、トラック 2/4、再生 6.0/90 秒)が出る。
- Enter は入力欄の改行になり、モーダルは閉じない。Tab は入力欄に取り込まれず、次の要素(「N 文字目」のボタン)へフォーカスが移る。Ctrl+Z で入力を戻せる。
- Escape(入力欄にフォーカスがある状態。選択の有無によらず)は、1 回でモーダルを閉じる。`<textarea>` のときと同じ。
- 編集領域に `role="textbox"` / `aria-multiline` / `aria-label="MML"` / `aria-describedby`(状態行と使用量行の id)/ `aria-invalid`(エラー時)/ `autocapitalize="off"` / `autocorrect="off"` / `spellcheck="false"` が付く。

分かったこと:

- `document.execCommand("insertText")` で、色の付いた先頭の文字を含む範囲を置き換えると、WebKit が行(`.cm-line`)にその文字の色をインライン style として残し、無装飾の文字がその色になる。`xdotool` の本物の入力(Ctrl+A のあとの置き換えを含む)では起きなかった。確認の道具(`execCommand`)による現象で、通常の入力では再現しない。

### 未確認

- **Android WebView**: 未確認。この環境には AVD が無く、実機も接続されていない。ソフトキーボードの自動大文字化・自動修正の抑止、`--keyboard-inset` との相性(入力中の行がキーボードに隠れないこと)、長い MML での二重スクロールの有無、「N 文字目」のボタンのタップは、実機かエミュレーターで確認が要る。入力が壊れる、またはキャレットがずれる場合は、「リスクと撤退条件」のとおり、自前の `<textarea>` 重ね描きの再検討に戻す。
- **日本語 IME の変換**: 未確認。Xvfb 上では IME を動かしていない。
- `AddColumnModal` 内の入力欄の実画面での確認はしていない(同じコンポーネントで、単体テストで 2 つ並べた場合の id の衝突は確認した)。
