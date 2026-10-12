# モバイル投稿画面の入力欄をキーボード上端まで広げる Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Android のソフトキーボードが WebView の下端を覆う高さを CSS 変数 `--keyboard-inset` としてネイティブからフロントへ渡し、モバイルの投稿画面で入力欄がキーボード上端まで広がり、ボタン群がキーボード直上に並ぶようにする。

**Architecture:** `MainActivity.kt` が IME インセットを監視し、WebView 下端との重なり(CSS px)を `document.documentElement` の `--keyboard-inset` に設定する。マニフェストは `adjustNothing` で WebView が縮まない前提に統一する。フロントは全画面 `Modal` の本文下余白にこの値を足し、`ComposeBar expanded` は入力欄を `flex-1` で伸ばす。

**Tech Stack:** Kotlin(AndroidX `WindowInsetsCompat`)、Svelte 5、Tailwind v4、Vitest、WebdriverIO(WebKitGTK E2E)、Android エミュレーター + CDP

**Spec:** `docs/superpowers/specs/2026-10-12-mobile-compose-keyboard-inset-design.md`

## Global Constraints

- PC版・モバイル以外の `ComposeBar`(`expanded` でない投稿バー)は変更しない。
- `src-tauri/gen/android/app/src/main/java/com/onodai/tsumugi/generated/` は Tauri が再生成するため**編集しない**。`MainActivity.kt` とマニフェストだけを編集する。
- `env(safe-area-inset-*)` を直接書かず `var(--safe-*)` を使う。同様に `--keyboard-inset` は `app.css` の `:root` に既定値 `0px` を置く(`docs/design/style-guide.md` §11)。
- 角丸・フォントサイズ・アイコンサイズは style-guide のスケールに従う。
- コミットメッセージは件名のみ(本文・箇条書き禁止)。Co-Authored-By トレーラーは別途付与される。コミットが失敗/タイムアウトしたらリトライせず報告する。`--no-verify` 禁止。
- ブランチは `feat/mobile-compose-keyboard-inset`(作成済み。#450 の PR #488 のブランチから分岐している)。`main` に直接コミットしない。**プッシュ・PR 作成は、ユーザーの指示があるまで行わない。**
- エミュレーターを起動したら、タスク完了前に**自分で PID を指定して**停止する(`pkill`/`killall` 禁止)。実画面(Wayland)に影響させないため `env -u WAYLAND_DISPLAY` と `-no-window` で起動する。既存 AVD(`tsumugi-verify` = API 34、`tsumugi-e2e` = API 36)は `-read-only` で使い、変更しない。
- UI の実画面確認(WebKitGTK)は必ず `xvfb-run` 越しに行う。起動した使い捨て Misskey(`docker compose`)・spec・設定ファイルは確認後に片付ける。

## Review Focus

- キーボードの表示→非表示→表示を繰り返しても、`--keyboard-inset` が必ず 0 に戻る/再び正しい値になる(値の取りこぼし・残留)。→ Task 1 のエミュレーター測定。
- 入力欄にフォーカスがあるがキーボードが出ていない(ハードウェアキーボード接続時など): `--keyboard-inset` は 0 のまま、入力欄が下端まで広がる。→ Task 1 の測定(キーボード非表示)。
- 添付サムネイルや投票欄が増えて内容が最小高さを超える(横向き+大きいキーボード等): 本文がスクロールして、ボタン群に届く。→ Task 3 のテスト(`overflow-y-auto`)と Task 5 の測定。
- 全画面モーダルの上に `compact` のエラー表示を出しても、余白の計算が壊れない。→ Task 2 のテスト。
- モバイルUIを PC で強制した場合(デスクトップ上のモバイルUIモード): `--keyboard-inset` は 0 のまま、入力欄が下端まで広がるだけで破綻しない。→ Task 4 の E2E(0px に戻す検証)。

---

### Task 1: ネイティブ: キーボード高さを `--keyboard-inset` として WebView に渡す(spike を兼ねる)

最大のリスク(`adjustNothing` でも IME インセットがリスナーに届くか)を最初に確かめる。**API 34 と API 36 のどちらかで `--keyboard-inset` が 0 のままなら、そこで止めてユーザーに報告する**(代替の再設計が要る)。

**Files:**
- Modify: `src-tauri/gen/android/app/src/main/AndroidManifest.xml`(`MainActivity` の `<activity>` に属性を1つ追加)
- Modify: `src-tauri/gen/android/app/src/main/java/com/onodai/tsumugi/MainActivity.kt`(`onWebViewCreate` の override と補助メソッドを追加)

**Interfaces:**
- Consumes: `WryActivity.onWebViewCreate(webView: WebView)`(生成済みの open メソッド)
- Produces: WebView の `document.documentElement.style` の `--keyboard-inset`(例: `"312.4px"`、キーボードが無い/重ならないときは `"0px"`)。以降のタスクのフロントはこのCSS変数だけを参照する。

- [ ] **Step 1: マニフェストに `adjustNothing` を追加**

`AndroidManifest.xml` の `MainActivity` の `<activity ...>` に、`android:launchMode="singleTask"` の次の行として追加する:

```xml
            android:windowSoftInputMode="adjustNothing"
```

結果(該当部分):

```xml
        <activity
            android:configChanges="orientation|keyboardHidden|keyboard|screenSize|locale|smallestScreenSize|screenLayout|uiMode"
            android:launchMode="singleTask"
            android:windowSoftInputMode="adjustNothing"
            android:label="@string/main_activity_title"
            android:name=".MainActivity"
            android:exported="true">
```

- [ ] **Step 2: `MainActivity.kt` に IME インセットの監視を追加**

import に次を追加する(既存の import 群にアルファベット順で混ぜる):

```kotlin
import android.webkit.WebView
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import java.util.Locale
import kotlin.math.max
```

`class MainActivity : TauriActivity() {` の本体内、`private val shareInvocationCounter = ...` の直後に次を追加する:

```kotlin
    /// 直前に WebView へ渡した --keyboard-inset(px)。同じ値を何度も evaluateJavascript しないための記録。
    private var lastKeyboardInsetCssPx = -1f

    /// ソフトキーボード(IME)が WebView の下端を覆う高さを CSS 変数 --keyboard-inset としてフロントへ渡す。
    /// WebView はキーボードで縮まず(adjustNothing)キーボードが重なるため、フロントは CSS だけでは
    /// キーボード上端を知れない。ルートビューのインセットリスナーで IME の高さを受け取り、WebView 下端と
    /// の重なりを CSS px にして設定する。
    override fun onWebViewCreate(webView: WebView) {
        super.onWebViewCreate(webView)
        val root = window.decorView
        ViewCompat.setOnApplyWindowInsetsListener(root) { view, insets ->
            publishKeyboardInset(webView, insets)
            // 既定のインセット処理(システムバーの確保等)は変えない。リスナーを付けると既定処理が
            // 置き換わるため、ここで明示的に既定処理へ委ねて、その結果を返す。
            ViewCompat.onApplyWindowInsets(view, insets)
        }
        ViewCompat.requestApplyInsets(root)
    }

    private fun publishKeyboardInset(webView: WebView, insets: WindowInsetsCompat) {
        val imeVisible = insets.isVisible(WindowInsetsCompat.Type.ime())
        val imeBottomPx = if (imeVisible) insets.getInsets(WindowInsetsCompat.Type.ime()).bottom else 0
        // 重なり = max(0, WebView下端 − IME上端)。座標はどちらもウィンドウ内の座標。
        val windowHeight = webView.rootView.height
        val location = IntArray(2)
        webView.getLocationInWindow(location)
        val webViewBottomPx = location[1] + webView.height
        val imeTopPx = windowHeight - imeBottomPx
        val overlapPx = if (imeVisible) max(0, webViewBottomPx - imeTopPx) else 0
        val cssPx = overlapPx / resources.displayMetrics.density
        if (cssPx == lastKeyboardInsetCssPx) return
        lastKeyboardInsetCssPx = cssPx
        val value = String.format(Locale.US, "%.1fpx", cssPx)
        webView.evaluateJavascript(
            "document.documentElement.style.setProperty('--keyboard-inset','$value')",
            null,
        )
    }
```

- [ ] **Step 3: APK をビルドする**

Run:

```bash
cd src-tauri
export ANDROID_HOME=$HOME/Android/Sdk NDK_HOME=$HOME/Android/Sdk/ndk/27.2.12479018 JAVA_HOME=/usr/lib/jvm/java-21-openjdk
cargo tauri android build --debug --target x86_64 --apk > "$SCRATCH/android-build.log" 2>&1; tail -3 "$SCRATCH/android-build.log"
```

(`$SCRATCH` はセッションのスクラッチパッドのディレクトリ。)
Expected: ログ末尾に `Finished 1 APK at: …/app-universal-debug.apk`。Kotlin のコンパイルエラーが出たら、メッセージに従って import や型を直す(`resources` は `Activity.getResources()`、`window` は `Activity.getWindow()`)。

- [ ] **Step 4: API 34 のエミュレーターで `--keyboard-inset` を測る**

(以降のコマンドは `src-tauri/` で実行する。`SCRATCH` にはセッションのスクラッチパッドのディレクトリを入れる: `export SCRATCH=<スクラッチパッドのパス>`。)

```bash
export ANDROID_HOME=$HOME/Android/Sdk ANDROID_AVD_HOME=$HOME/.config/.android/avd
env -u WAYLAND_DISPLAY nohup $ANDROID_HOME/emulator/emulator -avd tsumugi-verify -read-only -no-window -no-audio -no-boot-anim -no-metrics -gpu swiftshader_indirect -port 5570 > "$SCRATCH/emulator.log" 2>&1 &
echo $! > "$SCRATCH/emulator.pid"
for i in $(seq 1 50); do [ "$(adb -s emulator-5570 shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && break; sleep 4; done
adb -s emulator-5570 install -r gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
adb -s emulator-5570 shell am start -n com.onodai.tsumugi.debug/com.onodai.tsumugi.MainActivity
sleep 12
```

CDP で評価するための補助スクリプトを `$SCRATCH/cdp.py` に作る(Chromium は `Origin` ヘッダー付きの接続を拒否するため `suppress_origin=True` が必須):

```python
import json, sys, urllib.request, websocket
tabs = json.load(urllib.request.urlopen("http://127.0.0.1:9333/json"))
page = [t for t in tabs if t.get("type") == "page"][0]
ws = websocket.create_connection(page["webSocketDebuggerUrl"], timeout=10, suppress_origin=True)
ws.send(json.dumps({"id": 1, "method": "Runtime.evaluate", "params": {"expression": sys.argv[1], "returnByValue": True}}))
while True:
    m = json.loads(ws.recv())
    if m.get("id") == 1:
        print(json.dumps(m["result"].get("result", {}).get("value"), ensure_ascii=False)); break
```

測定(投稿画面を開いてキーボードを出す→閉じる→出す):

```bash
A() { adb -s emulator-5570 "$@"; }
PID=$(A shell pidof com.onodai.tsumugi.debug | tr -d '\r')
A forward tcp:9333 localabstract:webview_devtools_remote_$PID
KB='getComputedStyle(document.documentElement).getPropertyValue("--keyboard-inset").trim() + " innerH=" + innerHeight'
python3 "$SCRATCH/cdp.py" 'document.querySelector("button[title=\"投稿\"]").click(), "opened"'; sleep 3
echo "keyboard shown:"; A shell dumpsys input_method | grep -m1 mInputShown; python3 "$SCRATCH/cdp.py" "$KB"
A shell input keyevent KEYCODE_BACK; sleep 2
echo "keyboard hidden:"; A shell dumpsys input_method | grep -m1 mInputShown; python3 "$SCRATCH/cdp.py" "$KB"
A shell input tap 600 420; sleep 2.5
echo "keyboard shown again:"; A shell dumpsys input_method | grep -m1 mInputShown; python3 "$SCRATCH/cdp.py" "$KB"
```

Expected(解像度 1080x2400・density 2.625 の `pixel_6` AVD の場合):
- shown: `mInputShown=true`、`--keyboard-inset` が **おおよそ 280〜330px**(キーボード高さ ≒ 約 780〜870 物理px ÷ 2.625)。`innerH` は変わらない(842)。
- hidden: `mInputShown=false`、`--keyboard-inset` が `0.0px`(または `0px`)。
- shown again: 再び 280〜330px。

`--keyboard-inset` が空文字または常に 0 の場合は、**ここで止めて**ユーザーに報告する(IME インセットが届いていない。`adjustNothing` の見直しや `ViewCompat.setWindowInsetsAnimationCallback` の併用など、仕様書の「未確認・リスク 1」の再設計が必要)。

- [ ] **Step 5: 同じ測定を API 36 のエミュレーターで行う**

API 34 のエミュレーターを PID 指定で止める(`adb -s emulator-5570 emu kill` の後、`kill -0 $(cat "$SCRATCH/emulator.pid")` で停止を確認)。`-avd tsumugi-e2e` で Step 4 と同じ手順を繰り返す(AVD 名以外は同じ)。Expected は Step 4 と同じ。API 36 は `targetSdk=36` のエッジ・トゥ・エッジ強制環境なので、`--keyboard-inset` が **キーボード高さ+ナビゲーションバー分を含む過大な値**になっていないかも確認する(スクリーンショットで、キーボード上端と WebView 下端が一致するかを目視する: `adb exec-out screencap -p > "$SCRATCH/api36.png"`)。ずれる場合は、`publishKeyboardInset` の重なり計算を直す(ナビゲーションバー分の扱いは「WebView 下端との重なり」で吸収する設計)。

- [ ] **Step 6: 後片付けとコミット**

エミュレーターを PID 指定で停止し、`adb -s emulator-5570 uninstall com.onodai.tsumugi.debug`、`adb forward --remove tcp:9333` を行う。

```bash
git add src-tauri/gen/android/app/src/main/AndroidManifest.xml src-tauri/gen/android/app/src/main/java/com/onodai/tsumugi/MainActivity.kt
git commit -m "feat: AndroidのキーボードをWebViewのCSS変数--keyboard-insetとして通知する"
```

---

### Task 2: フロント: `--keyboard-inset` の既定値と全画面モーダルの下余白

**Files:**
- Modify: `frontend/src/app.css`(`:root` に1行追加)
- Modify: `frontend/src/ui/Modal.svelte`(`bodyClass` の全画面側2行)
- Test: `frontend/src/ui/Modal.test.ts`

**Interfaces:**
- Consumes: Task 1 の `--keyboard-inset`(未設定の環境では `app.css` の `0px`)
- Produces: 全画面 `Modal` の本文の下余白 = `max(1rem, var(--safe-bottom), calc(var(--keyboard-inset) + 0.5rem))`。`maxHeight` 指定の全画面本文は `overflow-y-auto`(内容が収まらないときだけスクロール)。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/Modal.test.ts` の `describe("Modal モバイル全画面(Issue #450)", ...)` の末尾(閉じ括弧 `});` の直前)に追加する:

```ts
  it("全画面本文の下余白にセーフエリアとキーボード高さ(--keyboard-inset)を含める", async () => {
    mobile();
    const { getByText } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi") },
    });
    await tick();
    const body = getByText("hi").closest("div") as HTMLElement;
    expect(body.className).toContain("var(--safe-bottom)");
    expect(body.className).toContain("var(--keyboard-inset)");
  });

  it("maxHeight指定の全画面本文もキーボード高さを含み、内容が収まらないときはスクロールする", async () => {
    mobile();
    const { getByText } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi"), maxHeight: "100%" },
    });
    await tick();
    const body = getByText("hi").closest("div") as HTMLElement;
    expect(body.className).toContain("var(--keyboard-inset)");
    expect(body.className).toMatch(/overflow-y-auto/);
    expect(body.className).not.toMatch(/overflow-hidden/);
  });

  it("PC版(箱型)の本文はキーボード高さに影響されない", async () => {
    const { getByText } = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("hi") },
    });
    await tick();
    const body = getByText("hi").closest("div") as HTMLElement;
    expect(body.className).not.toContain("--keyboard-inset");
  });

  it("全画面モーダルの上でcompactを開いても全画面側の余白式は変わらない", async () => {
    mobile();
    const base = render(Modal, {
      props: { title: "設定", onclose: () => {}, children: textSnippet("base") },
    });
    await tick();
    const before = (base.getByText("base").closest("div") as HTMLElement).className;
    const err = render(Modal, {
      props: { title: "エラー", onclose: () => {}, children: textSnippet("err"), compact: true },
    });
    await tick();
    expect((base.getByText("base").closest("div") as HTMLElement).className).toBe(before);
    expect(before).toContain("var(--keyboard-inset)");
    err.unmount();
  });
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && npx vitest run src/ui/Modal.test.ts`
Expected: 新規4件のうち、1・2・4件目が FAIL(クラスに `--keyboard-inset` が無い)。3件目(PC版)は PASS。

- [ ] **Step 3: 実装する**

`frontend/src/app.css` の `:root` で `--safe-left: env(safe-area-inset-left, 0px);` の次の行に追加する:

```css
  /* ソフトキーボードがWebViewの下端を覆う高さ。Androidのネイティブ(MainActivity.kt)が実行時に
     上書きする。キーボードが無い/ネイティブが設定しない環境(PC等)では0。 */
  --keyboard-inset: 0px;
```

`frontend/src/ui/Modal.svelte` の `bodyClass` の全画面側を次に置き換える(`maxHeight` あり=`overflow-hidden` を `overflow-y-auto` にし、両方の下余白にキーボード分を足す):

```ts
  const bodyClass = $derived(
    fullscreen
      ? maxHeight
        ? "flex min-h-0 flex-1 flex-col overflow-y-auto p-4 pb-[max(1rem,var(--safe-bottom),calc(var(--keyboard-inset)_+_0.5rem))]"
        : "min-h-0 flex-1 overflow-y-auto p-4 pb-[max(1rem,var(--safe-bottom),calc(var(--keyboard-inset)_+_0.5rem))]"
      : maxHeight
        ? "flex flex-1 flex-col min-h-0"
        : "contents",
  );
```

(Tailwind の任意値では空白を `_` で書く。`calc(a_+_b)` は `calc(a + b)` として出力される。)

- [ ] **Step 4: テストが通ることを確認**

Run: `cd frontend && npx vitest run src/ui/Modal.test.ts`
Expected: 全件 PASS。

- [ ] **Step 5: 全体テストと型チェック**

Run: `cd frontend && pnpm check && pnpm test`
Expected: エラー0件・全件 PASS。`ReplyTargetCard.test.ts` が「30s/31s」で落ちることがあるが既存の不安定なテストで本変更と無関係(再実行して確認し、報告に名前を残す)。

- [ ] **Step 6: Commit**

```bash
git add frontend/src/app.css frontend/src/ui/Modal.svelte frontend/src/ui/Modal.test.ts
git commit -m "feat: 全画面モーダルの下余白にキーボード高さを含める"
```

---

### Task 3: フロント: 投稿画面の入力欄を伸ばし、ボタン群を下端に寄せる

**Files:**
- Modify: `frontend/src/ui/ComposeBar.svelte`(列・入力欄ラッパー・textarea のクラス。`expanded` のときだけ)
- Modify: `frontend/src/App.svelte`(投稿画面の `Modal` に `maxHeight="100%"`)
- Test: `frontend/src/ui/ComposeBar.test.ts`

**Interfaces:**
- Consumes: Task 2 の全画面 `Modal` 本文(`maxHeight` 指定時は `flex min-h-0 flex-1 flex-col overflow-y-auto`)。
- Produces: `ComposeBar expanded` の textarea が本文の余り高さを使って広がる(ボタン群は常に入力欄の下)。`expanded` でない投稿バーは不変。

- [ ] **Step 1: 失敗するテストを書く**

`frontend/src/ui/ComposeBar.test.ts` の末尾に追加する:

```ts
describe("ComposeBar expanded のレイアウト(モバイル投稿画面で入力欄をキーボード上端まで広げる)", () => {
  function parts(textarea: HTMLElement) {
    const wrapper = textarea.parentElement as HTMLElement;
    const column = wrapper.parentElement as HTMLElement;
    return { wrapper, column };
  }

  it("expandedのとき、入力欄ラッパーが余り高さを取り(flex-1)、入力欄は伸びてサイズ変更できない", () => {
    const { getByTestId } = render(ComposeBar, { props: { expanded: true } });
    const textarea = getByTestId("compose-textarea");
    const { wrapper, column } = parts(textarea);
    expect(wrapper.className).toMatch(/(^|\s)flex-1(\s|$)/);
    expect(wrapper.className).toMatch(/(^|\s)flex(\s|$)/);
    expect(wrapper.className).toMatch(/min-h-24/);
    expect(column.className).toMatch(/self-stretch/);
    expect(textarea.className).toMatch(/resize-none/);
    expect(textarea.className).not.toMatch(/resize-y/);
    expect(textarea.className).toMatch(/min-h-24/);
  });

  it("expandedでないとき(PCの投稿バー)は従来のレイアウトのまま", () => {
    const { getByTestId } = render(ComposeBar);
    const textarea = getByTestId("compose-textarea");
    const { wrapper, column } = parts(textarea);
    expect(wrapper.className).toBe("relative");
    expect(column.className).not.toMatch(/self-stretch/);
    // 非expandedの入力欄は、未フォーカス時のコンパクト表示で resize-none になる(従来どおり)ため、
    // クラス全体ではなく「伸びる指定(flex-1)が無い」ことだけを確認する。
    expect(textarea.className).not.toMatch(/(^|\s)flex-1(\s|$)/);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cd frontend && npx vitest run src/ui/ComposeBar.test.ts -t "expanded のレイアウト"`
Expected: 「expandedのとき…」が FAIL(`flex-1` 無し・`resize-y` のまま)。「expandedでないとき…」は PASS。

- [ ] **Step 3: `ComposeBar.svelte` を変更する**

(a) 列(`<div class="flex flex-1 min-w-0 flex-col gap-1">`、`{#if replyTo || quoteOf}` の直前の行)を次に置き換える:

```svelte
  <div class={expanded ? "flex flex-1 min-w-0 flex-col gap-1 self-stretch" : "flex flex-1 min-w-0 flex-col gap-1"}>
```

(b) 入力欄ラッパー(`<div class="relative">`、`<textarea` の直前の行)を次に置き換える:

```svelte
  <div class={expanded ? "relative flex flex-1 min-h-24" : "relative"}>
```

(c) textarea の `expanded` 側のクラス文字列(`class={expanded ? "w-full box-border resize-y …` の最初の分岐)を次に置き換える。`resize-y` を `resize-none` にし、`w-full min-w-0` とする(他の2つの分岐は変更しない):

```
"w-full min-w-0 box-border resize-none rounded-md border border-border bg-muted py-1.5 pr-[34px] pl-2 font-[inherit] text-sm leading-[1.4] text-foreground min-h-24 [transition:min-height_0.12s_ease]"
```

(textarea はラッパー(flex の行)の子なので、`align-items: stretch` の既定で高さが埋まる。)

- [ ] **Step 4: `App.svelte` で投稿画面の本文を縦並びの枠にする**

```svelte
    <Modal title="投稿" onclose={() => (app.showComposeModal = false)} maxHeight="100%">
```

(`maxHeight` はモバイルでは値が無視され、「本文を `flex flex-col` の枠にする」切り替えとしてだけ働く。`SearchModal` / `FollowListModal` / `ProfileModal` と同じ使い方。)

- [ ] **Step 5: テストが通ることを確認**

Run: `cd frontend && npx vitest run src/ui/ComposeBar.test.ts src/ui/Modal.test.ts`
Expected: 全件 PASS。

- [ ] **Step 6: 全体テストと型チェック**

Run: `cd frontend && pnpm check && pnpm test`
Expected: エラー0件・全件 PASS(`ReplyTargetCard.test.ts` の不安定な失敗は Task 2 と同様に扱う)。

- [ ] **Step 7: Commit**

```bash
git add frontend/src/ui/ComposeBar.svelte frontend/src/ui/ComposeBar.test.ts frontend/src/App.svelte
git commit -m "feat: モバイルの投稿画面で入力欄を伸ばしボタン群を下端に寄せる"
```

---

### Task 4: E2E(WebKitGTK)とドキュメント

**Files:**
- Modify: `e2e/helpers/mobile.ts`(`setKeyboardInset` を追加)
- Create: `e2e/specs-mobile/keyboard-inset.e2e.ts`
- Modify: `docs/design/style-guide.md`(§13 に追記)
- Modify: `docs/guide/user-guide.md`
- Modify: `CLAUDE.md`(Android 節に1行)

**Interfaces:**
- Consumes: Task 2・3 の挙動。`rect`・`SAFE_AREA`・`addAccountAndEnableMobile` は既存のヘルパー(`e2e/helpers/mobile.ts`)。
- Produces: `setKeyboardInset(px: number): Promise<void>`。

- [ ] **Step 1: ヘルパーを追加**

`e2e/helpers/mobile.ts` の `setSafeArea` の直後に追加する:

```ts
// ソフトキーボードがWebView下端を覆う高さ(Androidではネイティブが設定する)をCSS変数で上書きする。
export async function setKeyboardInset(px: number): Promise<void> {
  await browser.execute((v) => {
    document.documentElement.style.setProperty("--keyboard-inset", `${v}px`);
  }, px);
}
```

- [ ] **Step 2: E2E の spec を書く**

`e2e/specs-mobile/keyboard-inset.e2e.ts`:

```ts
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { addAccountAndEnableMobile, rect, SAFE_AREA, setKeyboardInset } from "../helpers/mobile";

describe("mobile compose page with the soft keyboard", () => {
  let bridge: MiauthBridge;

  before(async function () {
    this.timeout(120000);
    bridge = await startMiauthBridge();
    await addAccountAndEnableMobile(bridge, "mobileKeyboardInset");
  });

  after(async () => {
    await setKeyboardInset(0);
    await bridge.teardown();
  });

  it("grows the input down to the keyboard top and keeps the buttons above the keyboard", async () => {
    const KB = 300;
    await $('button[title="投稿"]').click();
    const textarea = await $('[data-testid="compose-textarea"]');
    await textarea.waitForDisplayed({ timeout: 15000 });
    const vh = await browser.execute(() => window.innerHeight);

    // キーボード無し: 入力欄は最小の高さ(96px)を大きく超えて広がる。
    await setKeyboardInset(0);
    const closed = await rect('[data-testid="compose-textarea"]');
    expect(closed!.height).toBeGreaterThan(200);

    // キーボード300px: 本文の下余白が max(セーフエリア, 300+8px) になり、入力欄がその分だけ縮む。
    await setKeyboardInset(KB);
    const open = await rect('[data-testid="compose-textarea"]');
    const submit = await rect('[data-testid="compose-submit"]');
    expect(submit!.bottom).toBeLessThanOrEqual(vh - KB);
    const expectedShrink = KB + 8 - SAFE_AREA.bottom;
    expect(Math.abs(closed!.height - open!.height - expectedShrink)).toBeLessThanOrEqual(2);

    // キーボードを閉じると元の高さに戻る。
    await setKeyboardInset(0);
    const back = await rect('[data-testid="compose-textarea"]');
    expect(Math.abs(back!.height - closed!.height)).toBeLessThanOrEqual(1);
    await browser.back();
    await textarea.waitForDisplayed({ reverse: true, timeout: 15000 });
  });
});
```

- [ ] **Step 3: フロントをビルドして E2E を実行する(Xvfb 越し)**

`e2e/README.md` の手順に従う:

```bash
cd frontend && pnpm build
cd ../src-tauri && cargo build
cd ../e2e && ./scripts/gen-ca.sh && docker compose up -d --wait && pnpm seed
NODE_EXTRA_CA_CERTS=./certs/ca.pem xvfb-run -a pnpm exec wdio run wdio.mobile.conf.ts
```

Expected: `specs-mobile/` の全 spec が PASS(既存15件 + 新規1件)。失敗した場合はメッセージの実測値(`closed.height`/`open.height`/`submit.bottom`)から原因を調べる。終了後 `docker compose down -v`。

- [ ] **Step 4: ドキュメントを更新する**

`docs/design/style-guide.md` §13「モーダル(モバイル)」の末尾(`sticky` の項目の後)に追記する:

```markdown
- 全画面本文の下余白は `max(1rem, var(--safe-bottom), calc(var(--keyboard-inset) + 0.5rem))`。`--keyboard-inset` はソフトキーボードが WebView 下端を覆う高さ(CSS px)で、Android のネイティブ(`MainActivity.kt`)が実行時に設定する。WebView はキーボードで縮まない(`adjustNothing`)ため、`100dvh` では合わせられない。キーボードが無い/PC では `app.css` の既定 `0px`。
- 本文いっぱいに伸ばしたい要素(モバイルの投稿画面の入力欄)は、`Modal` に `maxHeight` を渡して本文を縦並びの枠にし、要素を `flex-1`(最小高さ付き)にする。内容が収まらないときは本文がスクロールする。
```

`docs/guide/user-guide.md` の投稿の節(「投稿バーは画面上部にあります…」の段落の後)に追記する:

```markdown
- **モバイル版の投稿画面**: 入力欄が画面いっぱいに広がり、公開範囲・画像・CW・投稿などのボタンがキーボードのすぐ上に並びます。
```

`CLAUDE.md` の「### Android」節の末尾(`IllegalArgumentException: 26.0.2` の段落の後)に追記する:

```markdown
`MainActivity.kt` が IME(ソフトキーボード)のインセットから、キーボードが WebView 下端を覆う高さを CSS 変数 `--keyboard-inset` として WebView に設定する(マニフェストは `adjustNothing` で WebView は縮まない)。フロントはこの変数だけを参照する。`gen/android/.../generated/` は Tauri が再生成するため編集せず、`MainActivity.kt` とマニフェストだけを編集する。
```

- [ ] **Step 5: Commit**

```bash
git add e2e/helpers/mobile.ts e2e/specs-mobile/keyboard-inset.e2e.ts docs/design/style-guide.md docs/guide/user-guide.md CLAUDE.md
git commit -m "test: モバイル投稿画面のキーボード高さのE2Eとドキュメントを追加する"
```

---

### Task 5: Android エミュレーターでの通し検証(API 34 と API 36)

コードの変更は原則なし(不具合が見つかった場合のみ、その修正を TDD で行い別コミットにする)。

**Files:** なし(測定と報告)

**Interfaces:**
- Consumes: Task 1〜3 の成果(ネイティブ + フロントを含む APK)

- [ ] **Step 1: 最新のコードで APK をビルドして API 34 に入れる**

Task 1 の Step 3・4 と同じ手順(ビルド → `tsumugi-verify` を `-read-only` で起動 → インストール → 起動 → `adb forward` と `$SCRATCH/cdp.py`)。

- [ ] **Step 2: 投稿画面でキーボード上端との一致を測る**

投稿画面を開き、入力欄をタップしてキーボードを出してから測定する:

```bash
python3 "$SCRATCH/cdp.py" 'JSON.stringify((()=>{
  const kb = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--keyboard-inset"));
  const ta = document.querySelector("[data-testid=compose-textarea]").getBoundingClientRect();
  const submit = document.querySelector("[data-testid=compose-submit]").getBoundingClientRect();
  const body = document.querySelector("[role=dialog] > div:last-child").getBoundingClientRect();
  return { kb, innerH: innerHeight, taH: ta.height, taBottom: ta.bottom, submitBottom: submit.bottom, bodyBottom: body.bottom, kbTop: innerHeight - kb };
})())'
```

Expected(キーボード表示中):
- `kb` が 0 より大きい。
- `submitBottom <= kbTop`(ボタン群がキーボードに隠れない)。
- 本文の下端 `bodyBottom` は `innerH`、内容の下端(ボタン群の行の下端)が `kbTop − 8` 付近(誤差 数px)。
- 入力欄の高さ `taH` がキーボード無しのときより小さく、最小高さ(96px)以上。

スクリーンショット(`adb exec-out screencap -p > "$SCRATCH/api34-compose.png"`)で、**入力欄とボタン群がキーボード上端までを埋め、間に大きな空白が無い**ことを目視する(画像は自分で見る)。キーボードを閉じる(`KEYCODE_BACK`)と `kb` が 0 になり、ボタン群が画面下端(セーフエリア内側)に来ることも確認する。

- [ ] **Step 3: 検索モーダルで下の入力欄に届くか確認する**

アプリメニュー → 検索を開き、キーワード欄をタップしてキーボードを出す。`--keyboard-inset` が 0 でなく、本文をスクロールして「検索」ボタンがキーボードの上に出せることを CDP で確認する(`document.querySelector("[data-testid=search-submit]").scrollIntoView(); getBoundingClientRect().bottom <= innerHeight - kb`)。

- [ ] **Step 4: API 36 で同じ測定を行う**

API 34 のエミュレーターを PID 指定で止め、`tsumugi-e2e`(API 36)で Step 1〜3 を繰り返す。エッジ・トゥ・エッジ強制環境でも `submitBottom <= kbTop` となり、キーボードとの隙間/重なりが数px以内であること。ずれる場合は `publishKeyboardInset` の計算(Task 1 Step 2)を直し、修正の前に失敗を再現する測定結果を残す。

- [ ] **Step 5: 後片付け**

エミュレーターを PID 指定で停止し、APK のアンインストール・`adb forward --remove` を行う。`git status --short` が空であること。

- [ ] **Step 6: 報告**

測定値(API 34/36 それぞれの `kb`・`kbTop`・`submitBottom`・`taH`)とスクリーンショットの所見を報告する。**プッシュ・PR 作成はユーザーの指示を待つ**(基準ブランチは、PR #488 がマージ済みなら `main`、未マージなら `feat/issue-450-mobile-fullscreen-modal`)。
