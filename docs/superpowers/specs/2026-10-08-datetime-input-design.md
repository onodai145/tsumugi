# 日時入力(DateTimeInput)の共有化 設計

Issue: #60(予約投稿)の不具合修正。ブランチ `feat/issue-60-scheduled-post` 上で、PR 前に直す。

## 背景 / 目的

Linux(WebKitGTK)で、予約投稿の日時入力(`<input type="datetime-local">`)を押してもカレンダーしか出ず、時刻を選べない。

### 原因

WebKitGTK のネイティブ日付/時刻入力は UI が未成熟で、時刻を操作できない。これは Issue #430(検索モーダルの日時範囲)で既に診断されており、`SearchModal.svelte` は `flatpickr`(自前描画で OS ウィジェットに依存しない)に置き換えて対処している。予約投稿の実装(Task 5)は、この既存の診断を確認せずネイティブ input を使った。

同じ欠陥が、予約より前から `ComposeBar.svelte` の投票締切「日時を指定」(`type="datetime-local"`)にも残っている。

ネイティブの `datetime-local` / `date` / `time` を使っている箇所は、現状この 2 つで全部(`grep 'type="date\|type="time\|type="datetime'` で確認)。

### 目的

- ComposeBar の 2 か所を、Linux でも日付と時刻を選べる入力に置き換える。
- `SearchModal` が持つ flatpickr の呼び出しと、約 110 行のテーマ CSS を共有ファイルへ移し、`SearchModal` と新しい `DateTimeInput` が同じ実装を使う。WebKitGTK の描画の癖への対処(`appearance: none` 等)を 2 か所に持たない。

## 合意済みの前提

- 共有部品に切り出す(`SearchModal` の内部にも触る)。
- 日時入力はネイティブ input を使わず flatpickr で統一する。

## 仕様

### 共有ファイル

- `frontend/src/lib/flatpickrDatePicker.ts`(新規): `SearchModal` の `datePicker` アクション(flatpickr を素の input に被せる)を、挙動を変えずに移す。
  - 設定は現状のまま: `enableTime: true`、`time_24hr: true`、`dateFormat: "Y-m-d H:i"`、日本語ロケール、`onReady` での年→月の入れ替え。
  - `opts.defaultMinute` を任意で足す。未指定のときは現状の規則(`defaultHour === 0` なら 0、それ以外は 59)にするので、`SearchModal` の呼び出しは変わらない。
  - `flatpickr/dist/flatpickr.min.css` と、下記のテーマ CSS を、このモジュールから import する(import 元を 1 か所に集める)。
  - `disableMobile` は指定しない(現状どおり)。モバイル端末では flatpickr がネイティブ入力へ自動で戻る。
- `frontend/src/lib/flatpickrTheme.css`(新規): `SearchModal.svelte` の `<style>` にある `:global(.flatpickr-*)` ルールをそのまま移す。値(カラートークン、`appearance: none` の理由コメント)は変えない。

### `DateTimeInput.svelte`(新規、`frontend/src/ui/`)

`datetime-local` と同じ文字列形式を値に持つ、flatpickr のラッパ。

- props:
  - `value`(`$bindable`、`string`): `"YYYY-MM-DDTHH:mm"`(ローカルタイムゾーン)。空なら `""`。`<input type="datetime-local">` の値と同じ形式なので、`localInputToEpochSec` / `epochSecToLocalInput` / `computePollExpiresAt` など ComposeBar の既存ロジックは変えない。
  - `defaultHour`(`number`、既定 9)/ `defaultMinute`(`number`、既定 0): 日付だけ選んで時刻を触らなかったときの時刻。
  - `placeholder`、`class`、`disabled` と、`data-testid`(入力欄の要素に付く)。
- 表示される入力欄は `type="text"` + `readonly`(`SearchModal` と同じ)。表示形式は `Y-m-d H:i`。
- 同期:
  - flatpickr の `onChange` → `value` を `T` 区切りの文字列にする(選択解除なら `""`)。
  - `value` が外から変わったとき(予約の成功後の消去、「作成欄に戻す」での復元、投票締切の下書き復元) → `fp.setDate(..., false)` で表示を合わせる(`onChange` を発火させず、ループさせない)。現在の表示と同じ値なら何もしない。
- アンマウント時に `fp.destroy()` する(`body` 直下のカレンダー要素が残らないようにする)。

### 置き換え

- `ComposeBar.svelte`:
  - 予約の日時入力(`compose-schedule-input`)を `DateTimeInput` にする。`data-testid="compose-schedule-input"` は入力欄の要素に付ける。`defaultHour` は 9。
  - 投票締切「日時を指定」(`pollExpiresAt`)を `DateTimeInput` にする。
  - ほかのロジック(`scheduleAt` / `pollExpiresAt` が `T` 区切りの文字列、検証、送信)は変えない。
- `SearchModal.svelte`: ローカルの `datePicker` アクション、flatpickr の import、`<style>` 内の `:global(.flatpickr-*)` ルールを削除し、`lib/flatpickrDatePicker.ts` から import する。呼び出し側のマークアップと挙動は変えない。

### ドキュメント

- `docs/design/style-guide.md` に「日時入力」を足す: ネイティブの `datetime-local` / `date` / `time` を使わず `DateTimeInput`(検索モーダルは `datePicker` アクション)を使う。理由は WebKitGTK のネイティブ UI が未成熟なこと。
- `CLAUDE.md` の frontend の節に、同じ 1 行を足す(今回のように既存の診断を見落とさないため)。

## テスト

- `DateTimeInput.test.ts`(新規):
  - 入力欄が `type="text"` で、`datetime-local` ではない。
  - `_flatpickr.setDate(date, true)` で `value` が `"YYYY-MM-DDTHH:mm"` になる(ローカル成分から組み立てる。UTC ではない)。
  - `value` を外から変えると、表示(`Y-m-d H:i`)が追従し、`onChange` は発火しない(ループしない)。
  - `value` を `""` にすると表示が空になる。
  - アンマウントで `.flatpickr-calendar` が `body` から消える。
- `ComposeBar.test.ts`: 予約の日時を `fireEvent.input` で文字列として入れていた箇所を、flatpickr の `_flatpickr.setDate` を呼ぶテスト用ヘルパに置き換える。表示値を読む箇所(`.value` が `T` 区切りだった期待)は `Y-m-d H:i` に直す。投票締切の `input[type=datetime-local]` を探している箇所も同様に直す。テストの意図(過去日時の拒否、期間指定の基準、復元など)は変えない。
- `SearchModal.test.ts`: 変更しない。リファクタが挙動を変えていないことの安全網になる(変更が必要になったら、それ自体が回帰の兆候)。
- 実機確認:
  - 原因の確認(実装前): Xvfb 越しに、WebKitGTK 2.52(この環境の `webkit2gtk-4.1`)で最小の HTML(ネイティブ `datetime-local`)を開き、ポップアップに時刻を操作する手段があるかを確かめる。再現しない場合は、原因の仮説を見直す。
  - 修正後: 同じ最小 HTML と、実アプリ(あなたの環境)で、日付と時刻の両方を選べることを確認する。

## 対象外

- Android のネイティブピッカー(`disableMobile` を指定しないので、flatpickr が自動でネイティブ入力に戻る挙動のまま)。モバイルの E2E は無いため、Android での動作は未確認のまま残る。
- `SearchModal` の日時範囲を `DateTimeInput` に載せ替えること(`Date` オブジェクトで受ける現状のアクション API のまま)。
- 日時入力の `minDate`(過去日の無効化)。検証は送信時にすでにある。
- フォーム全般のネイティブ部品の見直し。

## 未確定事項(実装計画で確定させる)

- `value` の同期の細部(flatpickr の `setDate` と `onChange` の発火条件、`clear()` の扱い)は、実装時にテストで固定する。
- 共有モジュールを import したときの CSS の読み込み順(`flatpickr.min.css` → テーマ CSS)が、`SearchModal` の現状と同じになること。
