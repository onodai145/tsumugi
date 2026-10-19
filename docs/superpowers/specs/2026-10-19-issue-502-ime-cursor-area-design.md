# IME 変換候補ウィンドウの位置補正の設計

Issue #502(関連: #10 / PR #18、tauri-apps/tauri#11412)

## 目的

Linux の Wayland 環境で、IME(fcitx5)の変換候補ウィンドウが入力位置と違う場所に出る。入力位置の近くに出るようにする。

成功条件:

- `<input>` / `<textarea>` / contenteditable(CodeMirror)のどれでも、候補ウィンドウが入力位置のすぐ下に出る。
- 既存の入力欄コンポーネントは改修しない。入力欄が増えても自動で効く。
- Linux 以外、および Wayland 以外では挙動が変わらない。
- 入力(タイピング・変換・確定)の挙動を壊さない。

## 範囲

今回やる: 候補ウィンドウの位置の補正。

今回やらない:

- 変換中の下線が単語の位置に出ない問題(#502 の症状②)。WebKitGTK 側の描画の問題で、座標を渡す方法では直らない(tauri#11412 の報告も同じ)。#502 は症状②のためにクローズしない。
- 設定画面での ON/OFF の切替口(YAGNI)。問題が出たら環境変数でのオプトアウトを後から足す。
- wry / WebKitGTK / GTK への修正。

## 調査の結論(根拠)

- `yad --html`(素の WebKitGTK 2.54.1 / GTK3)でも、候補位置・下線の両方が再現した。tsumugi / wry 固有の問題ではない。`GTK_IM_MODULE` を外しても変わらない。モニターの scale は全て 1。
- WebKitGTK は、編集中のキャレット位置を `webkit_input_method_context_notify_cursor_area` 経由で IM に渡している(`InputMethodFilter::notifyCursorRect`)。アプリ側から同じ関数を呼べば、座標を上書きできる。
- 素の GTK + WebKit(python-gi、`ime_probe.py fixed`)で、ページ上に赤い印を描き、その座標を `notify_cursor_area` で通知したところ、候補ウィンドウは印の直下に出た。上書きは効く。何もしない場合(base)は位置がずれる。
- wry 0.57.0 でも `set_enable_preedit(false)` は残り、設定可能化の PR #1724 は未マージ。wry を上げても直らない。

## 決定事項

| 決定 | 内容 | 理由 |
|---|---|---|
| 方式 | フロントで実キャレット座標を求め、Rust コマンド経由で `notify_cursor_area` に渡す | 既存の入力欄を改修せず、アプリ全体の入力欄に効く |
| 対象 | `document` レベルのイベントで、全ての入力欄を共通処理する | #10 で、症状がアプリ全体で共通と確認済み |
| 有効条件 | Linux かつ Wayland セッションのときだけ上書きする | 症状を確認できたのは Wayland のみ。X11 は WebKit 本来の座標が正しい可能性があり、未検証 |
| コマンド | 全 OS に存在させ、Linux 以外は no-op | 生成される TS バインディングを OS で変えない |
| 失敗時 | 握りつぶす(`vibrate` と同じ流儀) | 位置補正の失敗でタイピングを妨げない |

## 構成と責務

### 新規ファイル

| ファイル | 責務 |
|---|---|
| `frontend/src/lib/caretRect.ts` | 純関数。アクティブ要素から viewport 座標のキャレット矩形 `{x, y, width, height}` を返す。textarea / input / contenteditable に対応し、取れなければ要素の左下にフォールバックする |
| `frontend/src/lib/imeCursorArea.ts` | `installImeCursorArea()`。`document` にリスナーを張り、矩形を求めて Rust に送る。間引き・遅延・重複排除を持つ |
| `src-tauri/src/commands/ime.rs` | `set_ime_cursor_area(x, y, width, height)` コマンド |

### 既存ファイルの変更(最小)

- `frontend/src/lib/caretPosition.ts`: `<input>` にも使えるよう汎用化する(折り返しなし・横スクロール考慮)。textarea の挙動は変えない。
- `frontend/src/lib/platform.ts`: Linux 判定 `isLinuxPlatform` を追加する。
- `src-tauri/src/lib.rs`: `specta_builder()` にコマンドを登録する。
- アプリ起動の入口: `installImeCursorArea()` を 1 回呼ぶ。
- 既存の入力欄コンポーネントと、PR #18 の preedit 再有効化(`lib.rs`)は変更しない。

### 境界

- フロントの責務は、「どこに出したいか」を viewport 座標(CSS px)で伝えるところまで。
- Rust の責務は、受け取った座標に webview の `zoom_level()` を掛けて GTK のウィジェット座標に換算し、IM に渡すこと。UI スケール(Issue #40、`Webview::set_zoom`)で CSS px とウィジェット座標が食い違うため、換算は Rust 側で行う。

## キャレット矩形の求め方

座標は viewport 座標(CSS px)。UI スケールが 100% のときは webview ウィジェットの座標と一致する。100% 以外のときは、Rust 側で `zoom_level()` を掛けて換算する(上の「境界」を参照)。

| 入力欄 | 求め方 |
|---|---|
| textarea | 既存の `getCaretCoordinates` に、要素の `getBoundingClientRect()` を足す(補完ポップアップと同じ換算) |
| `<input>` | 同じミラー方式で、折り返しなし・横スクロール考慮にする |
| contenteditable(CodeMirror) | `Selection` の collapsed Range から `getClientRects()` を取る |
| 取れない場合 | 入力欄の左下を、高さ 1 の矩形にする |

アンカー位置:

- 変換中は、`compositionstart` で記録した変換開始位置を基準にする。
- それ以外は、現在のキャレット位置を使う。
- 変換中の DOM 上のキャレット位置は未検証(下の検証項目を参照)。

## 送信のタイミング

WebKit は編集状態が変わるたびに、自分の(不正確な)座標を送り直す。こちらは、それより後に上書きする必要がある。

流れ:

1. 対象イベントを受ける。
2. `requestAnimationFrame` で DOM の反映を待つ。
3. 待ち時間 0(rAF の直後)で、その時点のキャレット矩形を求めて送信する。待機中に来た追加のイベントは、待機を延長せず 1 回の送信にまとめる(押しっぱなしのキー連打で送信が先送りされ続けないようにする)。
4. 送信の 40ms 後にもう一度、その時点のキャレット矩形を求めて送信する(再送)。WebKit の送信がこちらより遅れて着いた場合に、もう一度上書きし直すため。

送信済みの矩形と同じでも、毎回送る。WebKit は編集状態が変わるたびに自分の座標を送り直すため、「前回と同じだから送らない」とすると WebKit の不正確な座標が残ってしまう。

対象イベント: `focusin`、`input`、`compositionstart` / `compositionupdate` / `compositionend`、`keyup`、`mouseup`、`selectionchange`、`scroll`(capture)、`resize`。

対象要素: `input`(type が text / search / url / email / password / tel / number)、`textarea`、contenteditable。それ以外にフォーカスがあるときは何もしない。

待ち時間 0 + 再送 40ms は、実機(Hyprland + fcitx5)で待ち時間と再送の組み合わせを比べて、最もちらつきが少なかったもの(初期案は待ち時間 30ms・再送なし)。WebKit は変換中に 1 文字ごとに自前の座標を送り直すため、毎文字、候補が一瞬ずれた位置に出てから正しい位置に戻る現象は残る。

## プラットフォーム分岐とエラー処理

- フロント: `isLinuxPlatform` のときだけ `installImeCursorArea()` を実行する。
- Rust: 実処理は `#[cfg(target_os = "linux")]` の中だけに置く。Linux 以外は引数を捨てて `Ok(())` を返す。Android は `target_os = "android"` なので `webkit2gtk` の依存に巻き込まれない。
- Wayland 判定: `WAYLAND_DISPLAY` が設定済みで、かつ `GDK_BACKEND` が `x11` でないとき。起動時に 1 回だけ判定し、結果をキャッシュする。環境変数値を引数に取る純関数に切り出す。Wayland でなければ上書きせず、WebKit 本来の挙動に任せる。
- フロントのリスナーは DOM を読むだけ。`preventDefault` は呼ばず、passive にする。全体を `try/catch` で囲み、失敗してもタイピングを妨げない。
- NaN・無限大の矩形は送らない。値は整数に丸める。負の値は許容する。
- `invoke` の失敗は握りつぶす。IM コンテキストが取れない場合は、Rust が黙って `Ok(())` を返す。
- コマンドが受け取るのは座標の数値 4 つだけ。トークンも URL も扱わない。

## テスト

Vitest の環境は jsdom で、`offsetLeft` や `getBoundingClientRect` は 0 を返す。座標の正確さは単体テストで検証できないため、振り分けと制御を検証する。

| 対象 | 内容 |
|---|---|
| `caretRect.ts` | 要素種別の振り分け、フォールバック、NaN・無限大の除外。座標計算は `getBoundingClientRect` やヘルパーをモックして、足し算だけを検証する |
| `imeCursorArea.ts` | `invoke` をモックし、フェイクタイマーで検証する。連続イベントが 1 回の送信にまとまること、同一矩形でも毎回送ること、対象外要素で送らないこと、`invoke` が失敗しても例外を出さないこと |
| Rust | Wayland 判定の純関数の単体テスト(`WAYLAND_DISPLAY` の有無 × `GDK_BACKEND`)、zoom 換算の純関数の単体テスト |
| バインディング | 既存の `generates_frontend_bindings` テストで、コマンド登録と TS 生成を確認する |
| 他 OS | Linux 以外の no-op 分岐のコンパイルは、CI の既存ジョブ(Android ビルドなど)で確認する |

E2E は追加しない。実 Wayland + fcitx5 が必要で、既存の E2E 基盤(Xvfb)では再現できない。

## 実機検証(Hyprland + fcitx5)

自動化できないため、手動のチェックリストとして残す。

実装後に確認する、未検証の項目:

1. WebKit の座標が一瞬見えて、候補がちらつかないか。ちらつくなら、遅延や再送の戦略を見直す。
2. UI スケールが 100% 以外(50〜200%)のとき、`zoom_level()` による換算で候補が入力位置に追従するか。

変換中のアンカーは、`compositionstart` で記録した位置を使うので、DOM 上のキャレットが変換中にどこにあるかには依存しない。

実装後の確認(上の 2 項目に加えて):

- ComposeBar: 単行、折り返した複数行、スクロール後。
- 文章の途中で変換した場合。
- 設定画面の `<input>`。
- MML エディタ(CodeMirror)。
- ウィンドウを動かした後。
- X11 セッション、または `GDK_BACKEND=x11` で、挙動が変わっていないこと。

## 既知の制限

- 変換中の下線の位置ズレは直らない。
- 変換中は毎文字、候補が一瞬ずれた位置に出てから正しい位置に戻るちらつきが残る(WebKit が自前の座標を送り直すため)。根絶するには、WebKit の `notify_cursor_area` を横取りして WebKit 自身の座標を捨てる必要がある(`WebKitInputMethodContextClass` の vtable の書き換えなど)。unsafe な GObject クラスの書き換えになるため、今回の範囲では採らない。
- 要素内の実キャレットを使うが、transform・iframe・shadow DOM 内の入力欄では近似になる。
