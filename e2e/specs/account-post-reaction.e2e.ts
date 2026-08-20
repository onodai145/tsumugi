// アカウント追加(MiAuth) → 投稿 → 自分の投稿へのリアクション、という
// tsumugiの一番基本的な操作フローを実機(tauri-driver経由のWebKitGTK)で通しで検証する。
//
// セレクタは全て実際のSvelteコンポーネント(frontend/src/ui/*.svelte)から確認した実在の
// aria-label/data-testidであり、計画段階のドラフト仕様のセレクタは採用していない。
// data-testidが存在しなかった箇所は、表示に影響しない追加のみ行って新設した
// (frontend/src/ui/AddAccount.svelte, ComposeBar.svelte, NoteCard.svelte,
// AppMenu.svelte, AddColumnModal.svelte, input/ReactionPicker.svelte)。
import { chromium, type Page } from "playwright";
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { debugLog, debugLogPath } from "../helpers/debugLog";

const MISSKEY_HOST = "misskey.local:8443";

/**
 * MiAuth同意フローには、単一アカウントしか無くても必ず「アカウントを選択してください」
 * 画面(MkAuthConfirm.vueのaccountSelectフェーズ)が先に出る。miauthBridge.tsの
 * approveNext()は「許可」ボタン(consentフェーズ)しか押さないため、先にこの画面を
 * 通過させないとapproveNext()はタイムアウトする(Task 6 report「Known follow-up for
 * Task 7」参照)。
 *
 * ここではminiauthBridge.ts自体は変更せず、bridgeが公開しているCDPポートに対して
 * 別のPlaywright CDPクライアントで接続し、同じブラウザコンテキスト上のMiAuthタブを
 * 見つけて「続ける」を押す小さなヘルパーをspec側に追加する形で対応した
 * (Task 6 reportが示した「第二のCDPクライアントをapproveNext()と並行して走らせる」
 * 方式そのもの)。approveNext()自体を書き換えるより影響範囲が小さく、Task 6の
 * 成果物に手を入れずに済む。
 */
/**
 * デバッグ用: 診断に使うタグ。ページのconsole/pageerrorも同じログへ集約し、
 * CI失敗調査時に「MiAuthタブがそもそも来ていたか/来ていたなら何が起きたか」を
 * wdioログとは独立に追えるようにする(miauthBridge.tsのcontext.on("page",...)は
 * ブリッジ自身のCDP接続経由のイベントであり、こちらはconnectOverCDP()した
 * 別のCDP接続から見た同じページなので、念のため独立に貼っておく)。
 */
function attachPageDiagnostics(page: Page, label: string): void {
  page.on("console", (msg) => debugLog(`clickThroughAccountSelect:${label}:console`, `${msg.type()}: ${msg.text()}`));
  page.on("pageerror", (err) => debugLog(`clickThroughAccountSelect:${label}:pageerror`, err.stack ?? err.message));
}

async function dumpFailureArtifacts(page: Page, stage: string): Promise<void> {
  try {
    const bodyText = await page.innerText("body").catch((e) => `<failed to read body: ${String(e)}>`);
    debugLog("clickThroughAccountSelect:failure", `stage=${stage} url=${page.url()} bodyText(先頭2000文字)=${bodyText.slice(0, 2000)}`);
  } catch (err) {
    debugLog("clickThroughAccountSelect:failure", `stage=${stage} failed to dump body text: ${String(err)}`);
  }
  try {
    const screenshotPath = debugLogPath(`account-select-failure-${stage}.png`);
    await page.screenshot({ path: screenshotPath });
    debugLog("clickThroughAccountSelect:failure", `screenshot saved: ${screenshotPath}`);
  } catch (err) {
    debugLog("clickThroughAccountSelect:failure", `stage=${stage} failed to save screenshot: ${String(err)}`);
  }
}

async function clickThroughAccountSelect(cdpPort: number): Promise<void> {
  debugLog("clickThroughAccountSelect", `connecting over CDP to 127.0.0.1:${cdpPort}`);
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${cdpPort}`);
  try {
    // bridgeはlaunchPersistentContext()で単一コンテキストを起動済みなので、
    // connectOverCDP()時点で既にそのコンテキストが1つ存在しているはず。
    const context = browser.contexts()[0];
    if (!context) throw new Error("clickThroughAccountSelect: no browser context found via CDP");
    debugLog("clickThroughAccountSelect", `connected; existing pages=${context.pages().map((p) => p.url()).join(", ") || "(none)"}`);
    const existing = context.pages().find((p) => p.url().includes("/miauth/"));
    const page = existing ?? (await context.waitForEvent("page", { timeout: 30000 }));
    attachPageDiagnostics(page, existing ? "existing" : "waited");
    debugLog("clickThroughAccountSelect", `page acquired (${existing ? "was already open" : "via waitForEvent"}): ${page.url()}`);

    await page.waitForLoadState("domcontentloaded");
    debugLog(
      "clickThroughAccountSelect",
      `domcontentloaded: url=${page.url()} title=${await page.title().catch(() => "?")}`,
    );

    // 単一アカウントの場合デフォルトで選択済みのことが多いが、念のため明示的に
    // アカウント項目もクリックしておく(未選択だと「続ける」が無効なままの可能性がある)。
    // 以前はここを`.catch(() => {})`で握りつぶしていたため、このクリックが実際に
    // 成功したかどうかがCIログから一切分からなかった(2026-08-19 CI失敗調査で判明)。
    await page
      .getByText("e2etestadmin", { exact: false })
      .first()
      .click({ timeout: 5000 })
      .then(
        () => debugLog("clickThroughAccountSelect", "account row click: succeeded"),
        (err) => debugLog("clickThroughAccountSelect", `account row click: FAILED (continuing anyway): ${String(err)}`),
      );

    const continueButton = page.getByRole("button", { name: "続ける" });
    // "続ける"がクリック不能(タイムアウト)になる原因を、存在しない/disabled/
    // 覆われている、のどれかに切り分けるための事前チェック。タイムアウト値自体は
    // 変えない(15000ms) — 2026-08-19の失敗ログから、ページ自体は1.5秒程度で
    // レンダリング済みだったと推測されるため、これを「遅い」問題として timeout を
    // 伸ばすのは誤った対処になりうる(詳細はci-debug-report.md参照)。
    try {
      await continueButton.waitFor({ state: "attached", timeout: 15000 });
      const [count, visible, enabled] = await Promise.all([
        continueButton.count(),
        continueButton.isVisible().catch(() => "?"),
        continueButton.isEnabled().catch(() => "?"),
      ]);
      debugLog(
        "clickThroughAccountSelect",
        `続ける button attached: count=${count} visible=${visible} enabled=${enabled}`,
      );
    } catch (err) {
      debugLog("clickThroughAccountSelect", `続ける button never attached to DOM: ${String(err)}`);
      await dumpFailureArtifacts(page, "button-not-attached");
      throw err;
    }

    try {
      await continueButton.click({ timeout: 15000 });
      debugLog("clickThroughAccountSelect", "clicked 続ける button");
    } catch (err) {
      debugLog("clickThroughAccountSelect", `FAILED to click 続ける button: ${String(err)}`);
      await dumpFailureArtifacts(page, "button-click-failed");
      throw err;
    }
  } finally {
    // connectOverCDP()で得たBrowserをclose()してもCDP接続を切るだけで、
    // tsumugi/browser-open.shが開いた実ブラウザ自体は終了しない。
    await browser.close();
  }
}

describe("account → post → reaction", () => {
  let bridge: MiauthBridge;
  let noteText: string;

  before(async () => {
    bridge = await startMiauthBridge();
  });

  after(async () => {
    await bridge.teardown();
  });

  // 失敗時、実際のtsumugiアプリウィンドウ(WebKitGTK)側のスクリーンショットも
  // e2e/wdio-logs/へ残す。Playwright側(MiAuthタブ)のスクリーンショットは
  // dumpFailureArtifacts()が既に個別に保存するので、ここではWebDriverセッション
  // (browser.saveScreenshot())の方だけを担当する。
  // WebKitWebDriverのスクリーンショット対応をこの環境で検証できていないため、
  // 失敗しても元のテスト失敗を隠さないようtry/catchで包む。
  afterEach(async function () {
    if (this.currentTest?.state !== "failed") return;
    const safeTitle = (this.currentTest.title ?? "unknown").replace(/[^a-zA-Z0-9_-]+/g, "_");
    const path = debugLogPath(`app-window-failure-${safeTitle}-${Date.now()}.png`);
    try {
      await browser.saveScreenshot(path);
      debugLog("afterEach", `saved WebDriver screenshot: ${path}`);
    } catch (err) {
      debugLog("afterEach", `failed to save WebDriver screenshot: ${String(err)}`);
    }
  });

  it("adds an account via MiAuth", async function () {
    this.timeout(90000);

    // 起動直後、アカウント0件なのでApp.svelteが自動的にAddAccount画面を表示する
    // (App.svelte: `showAdd || reauthAccount || app.accounts.length === 0`)。
    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    // clickThroughAccountSelect() と bridge.approveNext() は両方とも同じ
    // (単一の)MiAuthタブの"page"イベントを、それぞれ別のPlaywright接続
    // (別々のBrowserContext JSオブジェクト)経由で待ち受けている。
    // Playwrightの`waitForEvent("page")`は将来発火するイベントしか拾えず、
    // 既に発生済みのイベントを後から拾うことはできない。そのため
    // 逐次実行(clickThroughAccountSelectの完了を待ってからapproveNextを
    // 呼ぶ)にすると、タブは1つしか開かれないので、先にclickThroughAccountSelect
    // 側のリスナーがそのイベントを消費してしまい、approveNext()側の
    // waitForEventは(2つ目のタブなど来ないため)30秒タイムアウトするまで
    // 単純にハングする(実機検証済み)。
    // 3つとも並行に開始し、両方のリスナーがタブ生成前に登録されるようにする。
    // approveNext()内の「許可」ボタンクリックはPlaywrightのlocator.click()の
    // 暗黙のauto-wait(既定30秒)で、clickThroughAccountSelectが先に
    // 「続ける」をクリックしてconsentフェーズへ進めるのを自然に待つ。
    await Promise.all([bridge.approveNext(), clickThroughAccountSelect(bridge.cdpPort), startButton.click()]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();

    // アカウントが追加されるとComposeBarが描画される(App.svelte: accounts.length > 0の時のみ表示)。
    const composeTextarea = await $('[data-testid="compose-textarea"]');
    await composeTextarea.waitForDisplayed({ timeout: 15000 });
  });

  it("posts a note", async () => {
    // カラムが1つも無いとノートを表示する場所自体が無い(App.svelte:
    // `groups.length === 0`のプレースホルダのみ)ため、投稿の前にHomeカラムを追加する。
    const menuTrigger = await $('[data-testid="app-menu-trigger"]');
    await menuTrigger.waitForDisplayed({ timeout: 15000 });
    await menuTrigger.click();

    const addColumnItem = await $('[data-testid="app-menu-add-column"]');
    await addColumnItem.waitForDisplayed({ timeout: 15000 });
    await addColumnItem.click();

    // ソースの既定値は"home"(AddColumnModal.svelte)なので、そのまま追加するだけでよい。
    // 「element did not become interactable」の実際の原因はアニメーション待ちの
    // 問題ではなかった(Modal.svelte自体にtransition/animationは無い、実装確認済み)。
    // `document.elementFromPoint()`で該当ボタンの中心座標を検証したところ、
    // `xvfb-run`の既定スクリーンサイズ(640x480、`xvfb-run`本体にハードコード)が
    // アプリのウィンドウサイズ(tauri.conf.json: 800x600)より小さく、
    // AddColumnModalが縦に伸びるとこのボタンが物理的にビューポート外に
    // 出てしまうことが根本原因だった(実機確認済み)。ウィンドウマネージャの
    // 無いXvfb上ではスクロールしても解決しないため、run-app.sh側でtsumugi
    // 起動用に十分な大きさのXvfb(1280x1024)をネストして解決した。
    //
    // それでもなお実機で断続的に`waitForClickable`が失敗することがあり、
    // 原因を追ったところ、WebKitGTKのウィンドウ自体の実サイズ(`window.
    // innerWidth`/`innerHeight`)が起動直後は不安定で、同じ1280x1024の
    // Xvfb画面でも起動ごとに948x987だったり948x464だったりするという
    // タイミング競合を実機で確認した(ウィンドウマネージャの無いXvfbでは
    // レイアウトが初回ペイント後に確定するまでに揺れがあると見られる)。
    // ここでは対象要素を触る前に`window.innerHeight`が短い間隔で2回連続
    // 同じ値になるまで待つことで、レイアウトが落ち着いてから操作する。
    await browser.waitUntil(
      async () => {
        const h1 = await browser.execute(() => window.innerHeight);
        await browser.pause(150);
        const h2 = await browser.execute(() => window.innerHeight);
        return h1 === h2 && h1 > 300;
      },
      { timeout: 10000, interval: 200 },
    );

    const addColumnSubmit = await $('[data-testid="add-column-submit"]');
    await addColumnSubmit.waitForDisplayed({ timeout: 15000 });
    await addColumnSubmit.scrollIntoView();
    await addColumnSubmit.waitForClickable({ timeout: 15000 });
    await addColumnSubmit.click();

    const textarea = await $('[data-testid="compose-textarea"]');
    await textarea.waitForDisplayed({ timeout: 15000 });
    noteText = `tsumugi e2e test note ${Date.now()}`;
    await textarea.setValue(noteText);

    const submitButton = await $('[data-testid="compose-submit"]');
    await submitButton.click();

    // 自分のHomeカラムにストリーミングで即座に流れてくるはず。
    // `*=text`(XPath contains()ベース)セレクタは、実機検証の結果
    // ノート本文自体(data-testid="note-text")には正しくヒットしDOMにも
    // 存在するのに`waitForDisplayed()`だけが20秒待っても`false`のままに
    // なることがあった(実機確認済み: 診断コードでDOM上にテキストが
    // 存在することを確認しつつ`waitForDisplayed`が失敗するケースを観測)。
    // `*=`セレクタがマッチする実際の要素が意図した`note-text`要素ではなく
    // 親のより大きなコンテナ側に解決されている可能性を疑い、
    // `data-testid="note-text"`を明示的に指定したうえで`getText()`の
    // 内容を`waitUntil`で直接ポーリングする、より曖昧さの無い待ち方に
    // 変更した。
    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(noteText)) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: `posted note "${noteText}" did not appear` },
    );
  });

  it("reacts to its own note", async () => {
    // ローカルで同じスタック(古い投稿が残っているMisskey DB)に対して繰り返し
    // 実行した場合、タイムライン全体からの`$()`(先勝ち)だとテスト2で投稿した
    // ノート以外の(過去実行由来の)ノートに誤ってヒットしうる。CIは毎回新規DBの
    // ため問題は顕在化しないが、テスト2同様に`noteText`でスコープする。
    const noteArticle = await $(
      `//article[.//*[@data-testid="note-text" and contains(text(), "${noteText}")]]`,
    );
    await noteArticle.waitForDisplayed({ timeout: 15000 });

    const reactionButton = await noteArticle.$('button[aria-label="リアクション"]');
    await reactionButton.waitForDisplayed({ timeout: 15000 });
    await reactionButton.click();

    const thumbsUp = await $('[data-testid="emoji-pick-👍"]');
    await thumbsUp.waitForDisplayed({ timeout: 15000 });
    await thumbsUp.click();

    const reactionCount = await noteArticle.$('[data-testid="note-reaction-count-👍"]');
    await reactionCount.waitForDisplayed({ timeout: 15000 });
    await expect(reactionCount).toHaveText("1");
  });
});
