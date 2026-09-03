// サーバ側ワードミュート(mutedWords)がtsumugiのタイムラインに反映されることを検証する
// (Issue #11)。account-post-reaction.e2e.tsと違い、ミュート対象/非対象のノートは
// UI操作ではなくMisskey REST APIから直接投稿する(helpers/misskeyApi.ts)。これは
// 「compose UIが正しく動くか」ではなく「サーバのmutedWords設定がtsumugiの表示フィルタに
// 反映されるか」だけを見たいためで、投稿経路を単純化してこのテスト自身の複雑さを減らす狙い。
//
// 手順: (1) 種付けユーザーでサインインし、i/updateでmutedWordsを設定 → (2) 同じユーザーで
// 制御ノート(ミュート語を含まない)とミュート対象ノート(ミュート語を含む)を投稿 → (3) tsumugi
// でそのアカウントをMiAuth追加(account-post-reaction.e2e.tsと同じ既存フロー) → (4) Home
// カラムを追加 → (5) 初期ロード(REST fetch_and_filter_multi経由)で制御ノートは表示され、
// ミュート対象ノートは表示されないことを確認する。
//
// アカウント追加より先にmutedWordsを設定しているのは、同期(sync_server_mutes)が
// 起動時とアカウント追加時にしか走らないため — 追加後に設定すると、この実行内では
// 反映を確認する手段が無い(既存のsync_server_mutesにはUI上の手動再同期ボタンが無い)。
//
// 注記: ここで検証するのは「サーバのmutedWords設定が(誰の投稿かに関わらず)効くこと」で
// あり、「自分自身の投稿は対象外になる」という仕様(未決定・未実装)を主張するものではない。
// is_word_note_muted()は投稿者を見ないため、種付けユーザー自身の投稿でもミュート対象なら
// 隠れる。それがこのテストの前提。
import { chromium, type Page } from "playwright";
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { signInAsSeededUser, setMutedWords, createNote } from "../helpers/misskeyApi";
import { debugLog, debugLogPath } from "../helpers/debugLog";

const MISSKEY_HOST = "misskey.local:8443";
const MUTED_WORD = "tsumugie2ewordmute";

function attachPageDiagnostics(page: Page, label: string): void {
  page.on("console", (msg) => debugLog(`serverWordMute:${label}:console`, `${msg.type()}: ${msg.text()}`));
  page.on("pageerror", (err) => debugLog(`serverWordMute:${label}:pageerror`, err.stack ?? err.message));
}

async function dumpFailureArtifacts(page: Page, stage: string): Promise<void> {
  try {
    const bodyText = await page.innerText("body").catch((e) => `<failed to read body: ${String(e)}>`);
    debugLog("serverWordMute:failure", `stage=${stage} url=${page.url()} bodyText(先頭2000文字)=${bodyText.slice(0, 2000)}`);
  } catch (err) {
    debugLog("serverWordMute:failure", `stage=${stage} failed to dump body text: ${String(err)}`);
  }
  try {
    const screenshotPath = debugLogPath(`server-word-mute-failure-${stage}.png`);
    await page.screenshot({ path: screenshotPath });
    debugLog("serverWordMute:failure", `screenshot saved: ${screenshotPath}`);
  } catch (err) {
    debugLog("serverWordMute:failure", `stage=${stage} failed to save screenshot: ${String(err)}`);
  }
}

// account-post-reaction.e2e.tsのclickThroughAccountSelect()と同一の理由・同一の実装
// (アカウント選択画面 → 「続ける」)。miauthBridge.tsは変更せず、spec側でCDP経由の
// 別クライアントとして接続する既存パターンをそのまま踏襲する。
async function clickThroughAccountSelect(cdpPort: number): Promise<void> {
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${cdpPort}`);
  try {
    const context = browser.contexts()[0];
    if (!context) throw new Error("clickThroughAccountSelect: no browser context found via CDP");
    const existing = context.pages().find((p) => p.url().includes("/miauth/"));
    const page = existing ?? (await context.waitForEvent("page", { timeout: 30000 }));
    attachPageDiagnostics(page, existing ? "existing" : "waited");

    await page.waitForLoadState("domcontentloaded");

    await page
      .getByText("e2etestadmin", { exact: false })
      .first()
      .click({ timeout: 5000 })
      .catch(() => {});

    const continueButton = page.getByRole("button", { name: "続ける" });
    try {
      await continueButton.waitFor({ state: "attached", timeout: 15000 });
    } catch (err) {
      await dumpFailureArtifacts(page, "button-not-attached");
      throw err;
    }
    try {
      await continueButton.click({ timeout: 15000 });
    } catch (err) {
      await dumpFailureArtifacts(page, "button-click-failed");
      throw err;
    }
  } finally {
    await browser.close();
  }
}

describe("server-side word mute (mutedWords) hides matching notes", () => {
  let bridge: MiauthBridge;
  let controlNoteText: string;
  let mutedNoteText: string;

  before(async function () {
    this.timeout(30000);
    const token = await signInAsSeededUser();
    // 1語のANDグループとして設定(Misskeyの mutedWords: (string|string[])[] のうち
    // 配列要素。api/mutes.rs::parse_muted_words の「配列要素」経路)。
    await setMutedWords(token, [[MUTED_WORD]]);

    const runId = Date.now();
    controlNoteText = `tsumugi e2e word-mute control ${runId}`;
    mutedNoteText = `tsumugi e2e word-mute target ${runId} ${MUTED_WORD}`;
    // アカウント追加(→sync_server_mutes)より先に投稿しておく。REST初期ロード
    // (fetch_and_filter_multi)経由でカラム追加時に両方まとめて取得されるはず。
    await createNote(token, controlNoteText);
    await createNote(token, mutedNoteText);

    // startMiauthBridge() 自身も内部でsignin-flowを叩く。直前のsignInAsSeededUser()と
    // 短時間に連続するとMisskeyのサインインレート制限(429)に当たることがある
    // (実機確認済み)。miauthBridge.ts自体は変更せず、ここで一呼吸置いて回避する。
    await new Promise((resolve) => setTimeout(resolve, 10000));
    bridge = await startMiauthBridge();
  });

  after(async () => {
    await bridge?.teardown();
  });

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

  it("adds an account via MiAuth (mutedWords already set on the server)", async function () {
    this.timeout(90000);

    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    await Promise.all([bridge.approveNext(), clickThroughAccountSelect(bridge.cdpPort), startButton.click()]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();

    const composeTextarea = await $('[data-testid="compose-textarea"]');
    await composeTextarea.waitForDisplayed({ timeout: 15000 });
  });

  it("shows the control note but hides the word-muted note", async () => {
    // account-post-reaction.e2e.tsのコメント参照: WebKitGTKの初期ウィンドウ実高さは
    // 起動ごとに揺れ(948x987だったり948x464だったりする)、AddColumnModalの送信ボタンが
    // 短い方のケースでビューポート外に出て`waitForClickable`が断続的に失敗する
    // (実機確認済み)。ここでは待つだけでなく、明示的にウィンドウを広げて根本的に回避する。
    await browser.setWindowSize(1280, 1024).catch((err) => debugLog("serverWordMute", `setWindowSize failed (continuing anyway): ${String(err)}`));

    const menuTrigger = await $('[data-testid="app-menu-trigger"]');
    await menuTrigger.waitForDisplayed({ timeout: 15000 });
    await menuTrigger.click();

    const addColumnItem = await $('[data-testid="app-menu-add-column"]');
    await addColumnItem.waitForDisplayed({ timeout: 15000 });
    await addColumnItem.click();

    // account-post-reaction.e2e.tsと同じレイアウト安定待ち(Xvfb初回ペイントの揺れ対策)。
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

    // 制御ノートが表示されるまで待つ(=初期ロードが完了した合図)。
    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(controlNoteText)) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: `control note "${controlNoteText}" did not appear` },
    );

    // 制御ノートが出た時点で初期ロードは完了しているはずなので、ミュート対象ノートが
    // 一切表示されていないことを確認する(タイミング待ちではなく状態確認)。
    const noteTexts = await $$('[data-testid="note-text"]');
    const visibleTexts: string[] = [];
    for (const el of noteTexts) {
      visibleTexts.push(await el.getText().catch(() => ""));
    }
    const mutedNoteVisible = visibleTexts.some((t) => t.includes(mutedNoteText));
    if (mutedNoteVisible) {
      debugLog("serverWordMute", `muted note unexpectedly visible; all visible note texts: ${JSON.stringify(visibleTexts)}`);
    }
    expect(mutedNoteVisible).toBe(false);
  });
});
