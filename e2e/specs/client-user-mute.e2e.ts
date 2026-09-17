// クライアント側NGユーザー設定(app.mute.ngUsers)がタイムライン表示に反映される
// ことを検証する(Issue #223)。server-word-mute.e2e.tsのクライアント側版。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { clickThroughAccountSelect } from "../helpers/accountSelect";
import { signUp, createNote, signInAsSeededUser, allowRegistration, followUser } from "../helpers/misskeyApi";
import { debugLog, debugLogPath } from "../helpers/debugLog";

const MISSKEY_HOST = "misskey.local:8443";
const MUTED_USERNAME = "e2etestuser4";
const MUTED_PASSWORD = "e2eTestPassword4!";

describe("client-side user mute hides the muted user's notes", () => {
  let bridge: MiauthBridge;
  let controlNoteText: string;
  let targetNoteText: string;

  before(async function () {
    this.timeout(30000);

    // このMisskeyテストインスタンスはデフォルトでdisableRegistration: trueのため、
    // signUp()(セルフサインアップ)を呼ぶ前に管理者トークンでdisableRegistrationを
    // 解除しておく必要がある(multi-account.e2e.ts/streaming-events.e2e.tsと同じ手順)。
    const adminToken = await signInAsSeededUser();
    await allowRegistration(adminToken);

    const { token } = await signUp(MUTED_USERNAME, MUTED_PASSWORD);

    // Homeタイムラインはフォロー中ユーザー+自分のノートのみを表示するため(streaming-events.e2e.ts
    // と同じ実機確認済みの制約)、admin(tsumugiに追加するアカウント)がMUTED_USERNAMEを
    // フォローしないと対象ノートがそもそもHomeカラムに出てこず、クライアント側ミュートの
    // 効果を確認できない。ここで先にフォローしておく。
    await followUser(adminToken, MUTED_USERNAME);

    const runId = Date.now();
    controlNoteText = `tsumugi e2e user-mute control ${runId}`;
    targetNoteText = `tsumugi e2e user-mute target ${runId}`;
    // ミュート設定より先に、対象ユーザーの投稿(ミュート対象)を投稿しておく。
    // 制御ノート(controlNoteText)はミュート対象外のユーザー=admin自身の投稿で、
    // "adds an account and posts a control note" it()の中でUI経由(ComposeBar)で
    // 投稿する。
    await createNote(token, targetNoteText);

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

  it("adds an account and posts a control note", async function () {
    this.timeout(90000);

    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    await Promise.all([
      bridge.approveNext(),
      clickThroughAccountSelect(bridge.cdpPort, { logTag: "clientUserMute", accountLabel: "e2etestadmin" }),
      startButton.click(),
    ]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();

    await browser.setWindowSize(1280, 1024).catch((err) => debugLog("clientUserMute", `setWindowSize failed (continuing anyway): ${String(err)}`));

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

    const addColumnSubmit = await $('[data-testid="add-column-submit"]');
    await addColumnSubmit.waitForDisplayed({ timeout: 15000 });
    await addColumnSubmit.scrollIntoView();
    await addColumnSubmit.waitForClickable({ timeout: 15000 });
    await addColumnSubmit.click();

    const textarea = await $('[data-testid="compose-textarea"]');
    await textarea.waitForDisplayed({ timeout: 15000 });
    await textarea.setValue(controlNoteText);
    const submitButton = await $('[data-testid="compose-submit"]');
    await submitButton.click();

    // ミュート設定前は両方表示されていることを確認する(制御ノート:自分の投稿でライブ反映、
    // 対象ノート:REST初期ロードで表示済みのはず)。
    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        const visible: string[] = [];
        for (const el of noteTexts) {
          visible.push(await el.getText().catch(() => ""));
        }
        return visible.some((t) => t.includes(controlNoteText)) && visible.some((t) => t.includes(targetNoteText));
      },
      { timeout: 20000, interval: 300, timeoutMsg: "control/target notes did not both appear before muting" },
    );
  });

  it("hides the target user's note after registering a client-side NG user, keeping the control note visible", async () => {
    const menuTrigger = await $('[data-testid="app-menu-trigger"]');
    await menuTrigger.waitForDisplayed({ timeout: 15000 });
    await menuTrigger.click();
    const openSettings = await $('[data-testid="app-menu-open-settings"]');
    await openSettings.waitForDisplayed({ timeout: 15000 });
    await openSettings.click();

    const muteTab = await $('[data-testid="settings-tab-mute"]');
    await muteTab.waitForDisplayed({ timeout: 15000 });
    await muteTab.click();

    const ngUsersTextarea = await $('[data-testid="mute-ng-users-textarea"]');
    await ngUsersTextarea.waitForDisplayed({ timeout: 15000 });
    await ngUsersTextarea.setValue(`@${MUTED_USERNAME}`);

    const saveButton = await $('[data-testid="mute-save"]');
    await saveButton.click();

    // 設定モーダルを閉じてタイムラインへ戻る(Modal.svelteの閉じるボタン)。
    const closeModal = await $('[data-testid="modal-close"]');
    await closeModal.waitForDisplayed({ timeout: 15000 });
    await closeModal.click();

    await browser.waitUntil(
      async () => {
        const noteTexts = await $$('[data-testid="note-text"]');
        const visible: string[] = [];
        for (const el of noteTexts) {
          visible.push(await el.getText().catch(() => ""));
        }
        return !visible.some((t) => t.includes(targetNoteText));
      },
      { timeout: 20000, interval: 300, timeoutMsg: `target note "${targetNoteText}" was not hidden after muting` },
    );

    const noteTexts = await $$('[data-testid="note-text"]');
    const visible: string[] = [];
    for (const el of noteTexts) {
      visible.push(await el.getText().catch(() => ""));
    }
    expect(visible.some((t) => t.includes(controlNoteText))).toBe(true);
  });
});
