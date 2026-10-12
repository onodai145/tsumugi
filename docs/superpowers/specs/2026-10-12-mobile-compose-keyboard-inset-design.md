# モバイル投稿画面の入力欄をキーボード上端まで広げる

Issue: 未起票(#450 のモバイル全画面化 PR #488 の追加要望。別Issue化は要確認)

前提: PR #488(`feat/issue-450-mobile-fullscreen-modal`)で、モバイルの投稿画面は `Modal` の全画面ページになっている。本仕様はその上に積む。

## 背景・目的

モバイルの投稿画面で、入力欄(textarea)が固定的な高さ(`min-h-24` / `rows=4`)のままで、その下に公開範囲・画像・CW・投票・投稿などのボタン群が並ぶ。キーボードを開くと、ボタン群の下からキーボードの上端までに大きな空白ができる。目的は、**入力欄を下へ広げ、ボタン群をキーボードのすぐ上に置く**こと(チャットアプリと同じ並び)。キーボードを閉じたときは、入力欄が画面下端(セーフエリアの内側)まで広がり、ボタン群が下端に来る。

## 現状の調査結果(Android 14 エミュレーターでの実測)

- キーボード表示中も `window.innerHeight` と `visualViewport.height` が変わらない(842)。**WebView は縮まず、キーボードがコンテンツの上に重なる。** よって、CSS の `100dvh` や `h-full` だけではキーボード上端に合わせられない。
- `navigator.virtualKeyboard` は存在するが、`boundingRect.height` が常に 0(`overlaysContent = true` にしても、キーボードを開き直しても)。`env(keyboard-inset-height)` も 0。
- `<meta viewport>` の `interactive-widget=resizes-content` を実行時に差し替えても効かない。
- アプリは `targetSdk = 36`。Android 15 以降はエッジ・トゥ・エッジが強制され、マニフェストの `windowSoftInputMode=adjustResize` だけではウィンドウが縮まない可能性が高い(要検証)。

よって、キーボードの高さはネイティブ(Kotlin)から取得してフロントへ渡す必要がある。

## スコープ

対象:
- Android(`src-tauri/gen/android`、Git管理下)のキーボード高さの取得と、CSS変数 `--keyboard-inset` への反映
- モバイルの全画面 `Modal` の本文下余白へのキーボード分の反映(全画面モーダル共通)
- モバイル投稿画面(`ComposeBar expanded`)のレイアウト: 入力欄が伸び、ボタン群が下端(キーボード直上)に並ぶ

対象外:
- PC版、およびモバイル以外の `ComposeBar`(`expanded` でない投稿バー)
- iOS(Tauri の iOS 対応はこのリポジトリで未整備)
- Android 8–10(API 26–29)でのキーボード高さの取得。IME のインセット通知が信頼できないため、取れない場合は `--keyboard-inset` が 0 のまま(現状の挙動)になる。

## 設計

### 1. ネイティブ: キーボード高さの通知(`MainActivity.kt`)

- キーボード(IME)がWebViewの下端を覆っている高さを、CSS px で `--keyboard-inset` として WebView の `document.documentElement` に設定する。
  - 意味: 「キーボードが WebView のビューポート下端から何 px 分を覆っているか」。キーボードが無い、またはビューポートに重ならないときは `0px`。
  - 計算: IME の表示領域(画面下端から `WindowInsetsCompat` の `Type.ime()` の bottom ぶん)の上端と、WebView の画面上の下端との**重なり** `max(0, WebView下端 − IME上端)` を求める。システムバー(ナビゲーションバー/ジェスチャーバー)の扱いは、この重なりの定義が環境差(Android 14 以前は WebView がバーの上で終わる/Android 15 以降はバーの裏まで広がる)を吸収するため、場合分けしない。ピクセルから CSS px へは `displayMetrics.density` で割る。
  - 通知手段: `WryActivity.onWebViewCreate(webView)`(生成済みの open メソッド)を `MainActivity` で override し、ルートビューに `ViewCompat.setOnApplyWindowInsetsListener` を付けて IME インセットの変化ごとに `webView.evaluateJavascript("document.documentElement.style.setProperty('--keyboard-inset','<n>px')", null)` を呼ぶ(UIスレッド)。キーボードのアニメーション中の中間値も通知してよい(最終値に収束する)。
- `AndroidManifest.xml` の `MainActivity` に `android:windowSoftInputMode="adjustNothing"` を指定する。Android のバージョンによって WebView が縮む/縮まないが分かれるのをやめ、**どの環境でも縮まない**前提に統一する(`--keyboard-inset` と二重に補正しないため)。
- 既存の `--safe-top/right/bottom/left`(`env(safe-area-inset-*)`)の作りと揃える。

### 2. フロント: CSS変数とモーダル本文の下余白

- `app.css` の `:root` に `--keyboard-inset: 0px;` を追加する(`--safe-*` と同じ並び)。ネイティブが設定しない環境(PC・Android 10 以前)では 0 のまま。
- `Modal.svelte` の全画面本文の下余白を、現在の `pb-[max(1rem,var(--safe-bottom))]` から、キーボードが出ているときは「キーボード高さ + 0.5rem」に置き換える:
  `padding-bottom: max(1rem, var(--safe-bottom), calc(var(--keyboard-inset) + 0.5rem))`(キーボードが無いとき = 0 の場合は従来と同じ)。
- 効果: 全画面モーダルのすべて(検索・カラム追加などの入力フォームを含む)で、スクロール末尾がキーボードの上に出るようになり、下の入力欄がキーボードに隠れて届かなくなる問題も同時に解消する。

### 3. フロント: 投稿画面のレイアウト(`ComposeBar` の `expanded`)

- `App.svelte` のモバイル投稿画面で、`Modal` に `maxHeight="100%"` を渡し(本文を縦並びの枠にする既存の仕組み)、`ComposeBar expanded` が本文の高さいっぱいを使えるようにする。
- `expanded` のときの `ComposeBar` ルートは `h-full min-h-0`。入力欄を包む `relative` ラッパーは `flex-1 min-h-0` とし、textarea は `h-full resize-none`(`min-h-24` を最小高さとして残す)。
- 入力欄より下の要素(添付サムネイル、投票欄、ボタン群)は現在の並びのまま、入力欄が伸びた分だけ下端へ押し下げられる。結果として、ボタン群が `--keyboard-inset` ぶんの下余白(= キーボード直上、0.5rem の隙間)に収まる。ボタン群の並べ替えはしない。
- `expanded` でない `ComposeBar`(PC・投稿バー)は変更しない。
- 内容が最小高さの合計を超える場合(横向きでキーボードが大きい、添付が多い、など)は、本文がスクロールして届くようにする(`overflow-y-auto`)。

### 4. E2E・テスト

- E2E: `e2e/helpers/mobile.ts` に `setKeyboardInset(px)` を追加(`setSafeArea` と同じく CSS変数を上書き)。
- Vitest:
  - `Modal.test.ts`: 全画面本文の下余白のクラス(`calc(var(--keyboard-inset)…)`)を確認する。
  - `ComposeBar.test.ts`: `expanded` のとき textarea が `h-full`・ラッパーが `flex-1` であること、`expanded` でないときは変わらないこと。
- E2E(WebKitGTK、`specs-mobile/`): `--keyboard-inset` を 300px に上書きしたとき、投稿画面で「ボタン群の下端 = ビューポート下端 − 300px − 0.5rem」、「入力欄が最小高さ以上に広がっている」ことを確認する。0px に戻すと下端(セーフエリア内側)まで戻る。

## 検証(Android 実機相当)

エミュレーター(AVD)は既存の `tsumugi-verify`(API 34)と `tsumugi-e2e`(API 36)を使う(`-read-only` で起動)。debug APK(`cargo tauri android build --debug --target x86_64 --apk`)を入れ、CDP(`adb forward localabstract:webview_devtools_remote_<pid>`、`Origin` ヘッダー無しで接続)で次を実測する。

- キーボード表示中: `--keyboard-inset` が 0 でない値になり、**入力欄(textarea)の下端の下にあるボタン群の下端 + 0.5rem = キーボード上端**(誤差 1px 以内)。
- キーボードを閉じる: `--keyboard-inset` が 0 に戻り、ボタン群が下端(セーフエリア内側)に来る。
- API 34(非エッジ・トゥ・エッジ)と API 36(エッジ・トゥ・エッジ強制)の両方。
- 検索モーダルなどのフォームでも、スクロールで末尾の入力欄がキーボードの上に出る。

## 未確認・リスク(実装計画の最初のタスクで確認する)

1. `adjustNothing` でも IME インセットがルートビューのリスナーに届くか(API 30+ の想定。**最初に spike で確認し、届かない場合は代替(`adjustResize` + 自前のインセット処理)を再設計する**)。
2. Android 15 以降のエッジ・トゥ・エッジ環境で、WebView の下端とIME領域の重なり計算が正しいか(API 36 AVD で検証)。
3. キーボード種別(ハードウェアキーボード接続時、フローティング/分割キーボード、折りたたみ端末)での値。フローティング・分割時は IME インセットが 0 や小さい値になりうるが、その場合は余白が小さくなるだけで破綻しない想定。
4. IME の表示アニメーション中に `evaluateJavascript` が大量に呼ばれる負荷(最終値のみに間引く、`requestAnimationFrame` 側で受ける、等は計画で判断)。
5. 日本語入力(変換候補バー)が IME 領域に含まれる高さで通知されるか。

## ドキュメント更新

- `docs/design/style-guide.md` §13(モバイルのモーダル): `--keyboard-inset` の意味と、全画面本文の下余白の式を追記する。
- `docs/guide/user-guide.md`: モバイルの投稿画面で、入力欄が画面いっぱいに広がりキーボードの上にボタンが並ぶことを追記する。
- `CLAUDE.md` の Android 節: `adjustNothing` と `--keyboard-inset`(ネイティブ → CSS変数の通知)を、実装後に1行追記する。
