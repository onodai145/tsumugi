// TQLエキスパートモードで作成したカラムが、実データに対して正しく絞り込まれることを
// 検証する(Issue #223)。初期表示(REST fetch_and_filter_multi)とストリーミング受信
// (eval pipeline)の両方でフィルタが効くことを確認する。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { clickThroughAccountSelect } from "../helpers/accountSelect";
import { signInAsSeededUser, createNote } from "../helpers/misskeyApi";
import { debugLog, debugLogPath } from "../helpers/debugLog";

const MISSKEY_HOST = "misskey.local:8443";
const MARKER = "tsumugie2etqlmatch";

describe("TQL column filter matches real data", () => {
  let bridge: MiauthBridge;
  let token: string;
  let initialMatchText: string;
  let initialNoMatchText: string;

  before(async function () {
    this.timeout(30000);
    token = await signInAsSeededUser();
    const runId = Date.now();
    initialMatchText = `tsumugi e2e tql initial match ${runId} ${MARKER}`;
    initialNoMatchText = `tsumugi e2e tql initial no-match ${runId}`;
    // アカウント追加(→カラムのREST初期ロード)より先に投稿しておく。
    await createNote(token, initialMatchText);
    await createNote(token, initialNoMatchText);

    // startMiauthBridge() 自身も内部でsignin-flowを叩くため、直前のsignin連続で
    // レート制限(429)に当たることがある(既存2本と同じ既知の回避策)。
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

  it("adds an account via MiAuth", async function () {
    this.timeout(90000);

    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    await Promise.all([
      bridge.approveNext(),
      clickThroughAccountSelect(bridge.cdpPort, { logTag: "tqlColumnFilter", accountLabel: "e2etestadmin" }),
      startButton.click(),
    ]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();

    const composeTextarea = await $('[data-testid="compose-textarea"]');
    await composeTextarea.waitForDisplayed({ timeout: 15000 });
  });

  it("shows only the initial note matching the TQL filter", async () => {
    await browser.setWindowSize(1280, 1024).catch((err) => debugLog("tqlColumnFilter", `setWindowSize failed (continuing anyway): ${String(err)}`));

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

    // guided/expertタブの切り替え(AddColumnModal.svelte、"エキスパート(TQL)"ボタン)。
    const expertTab = await $("button=エキスパート(TQL)");
    await expertTab.waitForDisplayed({ timeout: 15000 });
    await expertTab.click();

    const tqlTextarea = await $('[data-testid="add-column-tql-textarea"]');
    await tqlTextarea.waitForDisplayed({ timeout: 15000 });
    await tqlTextarea.setValue(`from home where text -> "${MARKER}"`);

    const addColumnSubmit = await $('[data-testid="add-column-submit"]');
    await addColumnSubmit.waitForDisplayed({ timeout: 15000 });
    await addColumnSubmit.scrollIntoView();
    await addColumnSubmit.waitForClickable({ timeout: 15000 });
    await addColumnSubmit.click();

    // マーカー入りノートが表示されるまで待つ(=初期ロード完了の合図)。
    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(initialMatchText)) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: `matching note "${initialMatchText}" did not appear` },
    );

    // マーカー無しノートは一切表示されていないことを確認する。
    const noteTexts = await $$('[data-testid="note-text"]');
    const visibleTexts: string[] = [];
    for (const el of noteTexts) {
      visibleTexts.push(await el.getText().catch(() => ""));
    }
    const noMatchVisible = visibleTexts.some((t) => t.includes(initialNoMatchText));
    if (noMatchVisible) {
      debugLog("tqlColumnFilter", `non-matching note unexpectedly visible; all visible note texts: ${JSON.stringify(visibleTexts)}`);
    }
    expect(noMatchVisible).toBe(false);
  });

  it("filters live-streamed notes the same way", async () => {
    const runId = Date.now();
    const liveMatchText = `tsumugi e2e tql live match ${runId} ${MARKER}`;
    const liveNoMatchText = `tsumugi e2e tql live no-match ${runId}`;

    // マッチしないノートを先に投稿し、その後にマッチするノートを投稿する(逆順だと、
    // マッチノートの出現を確認した時点でまだマッチしないノートがストリーミングで
    // 届いていない可能性があり、ネガティブアサーションのタイミングに穴ができる)。
    // マッチノートの出現を待つことで、その時点までにマッチしないノートが届く機会が
    // あったことを保証できる。
    await createNote(token, liveNoMatchText);
    await createNote(token, liveMatchText);

    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(liveMatchText)) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: `live matching note "${liveMatchText}" did not appear` },
    );

    const noteTexts = await $$('[data-testid="note-text"]');
    const visibleTexts: string[] = [];
    for (const el of noteTexts) {
      visibleTexts.push(await el.getText().catch(() => ""));
    }
    const liveNoMatchVisible = visibleTexts.some((t) => t.includes(liveNoMatchText));
    if (liveNoMatchVisible) {
      debugLog("tqlColumnFilter", `live non-matching note unexpectedly visible; all visible note texts: ${JSON.stringify(visibleTexts)}`);
    }
    expect(liveNoMatchVisible).toBe(false);
  });
});
