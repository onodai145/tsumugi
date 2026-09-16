// main channel経由の通知・リノート・ノート削除が、既に開いているカラム/通知カラムへ
// リアルタイムに反映されることを検証する(Issue #223、stream/connection.rs)。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { clickThroughAccountSelect } from "../helpers/accountSelect";
import { signUp, createNote, renoteNote, deleteNote, signInAsSeededUser } from "../helpers/misskeyApi";
import { debugLog, debugLogPath } from "../helpers/debugLog";

const MISSKEY_HOST = "misskey.local:8443";
const SECOND_USERNAME = "e2etestuser3";
const SECOND_PASSWORD = "e2eTestPassword3!";
const BASE_URL = process.env.E2E_MISSKEY_URL ?? "https://misskey.local:8443";

describe("streaming events reflect in real time", () => {
  let bridge: MiauthBridge;
  let tokenB: string;

  before(async function () {
    this.timeout(30000);

    // このMisskeyテストインスタンスはデフォルトでdisableRegistration: trueのため、
    // signUp()(セルフサインアップ)を呼ぶ前に管理者トークンでdisableRegistrationを
    // 解除しておく必要がある(Task 3実装者が実機確認済み、multi-account.e2e.tsと同じ手順)。
    const adminToken = await signInAsSeededUser();
    const updateMetaRes = await fetch(`${BASE_URL}/api/admin/update-meta`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ i: adminToken, disableRegistration: false }),
    });
    if (!updateMetaRes.ok) {
      throw new Error(`admin/update-meta failed ${updateMetaRes.status}: ${await updateMetaRes.text()}`);
    }

    const signUpResult = await signUp(SECOND_USERNAME, SECOND_PASSWORD);
    tokenB = signUpResult.token;

    // Homeタイムラインはフォロー中ユーザー+自分のノートのみを表示するため、
    // 管理者(アカウントA)がuserBをフォローしないとBの投稿/リノートはHomeカラムに
    // 出てこない(実機確認済み: フォロー前は"まだノートがありません"のまま)。
    const showRes = await fetch(`${BASE_URL}/api/users/show`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ i: adminToken, username: SECOND_USERNAME }),
    });
    if (!showRes.ok) {
      throw new Error(`users/show failed ${showRes.status}: ${await showRes.text()}`);
    }
    const { id: userBId } = (await showRes.json()) as { id: string };
    const followRes = await fetch(`${BASE_URL}/api/following/create`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ i: adminToken, userId: userBId }),
    });
    if (!followRes.ok) {
      throw new Error(`following/create failed ${followRes.status}: ${await followRes.text()}`);
    }

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

  it("adds an account, a Home column, and a Notifications column", async function () {
    this.timeout(90000);

    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    await Promise.all([
      bridge.approveNext(),
      clickThroughAccountSelect(bridge.cdpPort, { logTag: "streamingEvents", accountLabel: "e2etestadmin" }),
      startButton.click(),
    ]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();

    await browser.setWindowSize(1280, 1024).catch((err) => debugLog("streamingEvents", `setWindowSize failed (continuing anyway): ${String(err)}`));

    async function addColumn(sourceValue: "home" | "notifications"): Promise<void> {
      const menuTrigger = await $('[data-testid="app-menu-trigger"]');
      await menuTrigger.waitForDisplayed({ timeout: 15000 });
      await menuTrigger.click();
      const addColumnItem = await $('[data-testid="app-menu-add-column"]');
      await addColumnItem.waitForDisplayed({ timeout: 15000 });
      await addColumnItem.click();

      await browser.waitUntil(
        async () => {
          const h1 = await browser.execute(() => window.innerHeight);
          await browser.pause(150);
          const h2 = await browser.execute(() => window.innerHeight);
          return h1 === h2 && h1 > 300;
        },
        { timeout: 10000, interval: 200 },
      );

      if (sourceValue !== "home") {
        // AddColumnModal.svelteのソース選択(guidedモード)はカスタムDropdownコンポーネント
        // (frontend/src/ui/Dropdown.svelte、ネイティブ<select>ではない)。
        // トリガーボタンをクリックしてポータル表示されるオプション一覧から選ぶ。
        const sourceTrigger = await $('[data-testid="add-column-source-select"]');
        await sourceTrigger.waitForDisplayed({ timeout: 15000 });
        await sourceTrigger.click();
        const notificationsOption = await $(`[data-testid="add-column-source-select-option-${sourceValue}"]`);
        await notificationsOption.waitForDisplayed({ timeout: 15000 });
        await notificationsOption.waitForClickable({ timeout: 15000 });
        await notificationsOption.click();
      }

      const addColumnSubmit = await $('[data-testid="add-column-submit"]');
      await addColumnSubmit.waitForDisplayed({ timeout: 15000 });
      await addColumnSubmit.scrollIntoView();
      await addColumnSubmit.waitForClickable({ timeout: 15000 });
      await addColumnSubmit.click();
    }

    // 1つ目: 既定のHomeカラム。
    await addColumn("home");
    // 2つ目: 通知カラム。
    await addColumn("notifications");

    const composeTextarea = await $('[data-testid="compose-textarea"]');
    await composeTextarea.waitForDisplayed({ timeout: 15000 });
  });

  it("shows a mention notification live", async () => {
    // アカウントAのusernameは"e2etestadmin"固定(certs/seeded-account.json)。
    const mentionText = `@e2etestadmin tsumugi e2e streaming mention ${Date.now()}`;
    await createNote(tokenB, mentionText);

    await browser.waitUntil(
      async () => {
        const previews = await $$('[data-testid="notification-note-preview"]');
        for (const el of previews) {
          const text = await el.getText().catch(() => "");
          if (text.includes("tsumugi e2e streaming mention")) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: "mention notification did not appear live" },
    );
  });

  it("shows a renote live in the timeline", async () => {
    const runId = Date.now();
    const originalText = `tsumugi e2e streaming renote source ${runId}`;
    const originalNoteId = await createNote(tokenB, originalText);
    await renoteNote(tokenB, originalNoteId);

    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(originalText)) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: "renote did not appear live" },
    );
  });

  it("removes a deleted note live", async () => {
    const runId = Date.now();
    const deleteTargetText = `tsumugi e2e streaming delete target ${runId}`;
    const noteId = await createNote(tokenB, deleteTargetText);

    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(deleteTargetText)) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: `note "${deleteTargetText}" did not appear before delete` },
    );

    await deleteNote(tokenB, noteId);

    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(deleteTargetText)) return false;
        }
        return true;
      },
      { timeout: 20000, interval: 300, timeoutMsg: `note "${deleteTargetText}" was not removed after delete` },
    );
  });
});
