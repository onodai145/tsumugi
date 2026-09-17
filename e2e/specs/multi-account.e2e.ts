// 2アカウントを同時に追加し、それぞれのカラムが互いのデータを混同しないこと、
// ComposeBarのアカウント切り替えが正しく機能することを検証する(Issue #223)。
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { clickThroughAccountSelect } from "../helpers/accountSelect";
import { signUp, signInAsSeededUser, allowRegistration } from "../helpers/misskeyApi";
import { debugLog, debugLogPath } from "../helpers/debugLog";

const MISSKEY_HOST = "misskey.local:8443";
const SECOND_USERNAME = "e2etestuser2";
const SECOND_PASSWORD = "e2eTestPassword2!";

describe("multiple accounts used simultaneously", () => {
  // 重要: startMiauthBridge()が起動するChromiumは常に同じ固定CDPポート
  // (miauthBridge.ts: CDP_PORT=9333)で外部向けデバッグポートを開こうとする。
  // また、tsumugi本体(検証対象アプリ)がURLを開く先も`run-app.sh`が起動時に
  // 一度だけ設定する`E2E_MIAUTH_CDP_PORT`(既定9333)で固定されており、アプリの
  // プロセス生存中は変えられない。したがって2つのbridgeを同時に生存させると、
  // 2つ目のChromiumが同じOSポートへのbindに失敗し(先勝ちの1つ目だけが実際に
  // 外部CDP接続を受け付ける)、tsumugi側が開く新規タブは常に「たまたま先に
  // ポートを掴んだ方」のブラウザに着地する。結果、もう一方のbridgeの
  // approveNext()(自分自身のcontextオブジェクト上でpageイベントを待つ)は
  // 永久にイベントが来ずタイムアウトする(実機で確認済み: 1個目のアカウント追加中に
  // 発生した`waitForEvent: Timeout 30000ms exceeded while waiting for event "page"`)。
  // 対策として、2つのbridgeを同時に生存させず、1個目のアカウント追加が完了したら
  // 直ちにbridgeAをteardown()してポートを解放してから2個目のbridgeBを起動する
  // (逐次利用)。
  let bridgeA: MiauthBridge | undefined;
  let bridgeB: MiauthBridge | undefined;

  before(async function () {
    this.timeout(30000);

    // このMisskeyテストインスタンスはデフォルトでdisableRegistration: trueのため、
    // signUp()(セルフサインアップ)を呼ぶ前に管理者トークンでdisableRegistrationを
    // 解除しておく必要がある(Task 3実装者が実機確認済み)。
    const adminToken = await signInAsSeededUser();
    await allowRegistration(adminToken);

    await signUp(SECOND_USERNAME, SECOND_PASSWORD);
  });

  after(async () => {
    await bridgeA?.teardown();
    await bridgeB?.teardown();
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

  async function addAccount(bridge: MiauthBridge, accountLabel: string): Promise<void> {
    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    await Promise.all([
      bridge.approveNext(),
      clickThroughAccountSelect(bridge.cdpPort, { logTag: "multiAccount", accountLabel }),
      startButton.click(),
    ]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();
  }

  it("adds two accounts via MiAuth", async function () {
    this.timeout(120000);

    // startMiauthBridge()自身も内部でsignin-flowを叩く。直前のsignInAsSeededUser()/
    // signUp()(before()フック)と短時間に連続するとMisskeyのサインインレート制限
    // (429)に当たることが実機で確認できた(server-word-mute.e2e.ts等の既存シナリオと
    // 同じ回避策)。
    await new Promise((resolve) => setTimeout(resolve, 10000));
    bridgeA = await startMiauthBridge();

    // 1アカウント目: 起動直後は自動的にAddAccount画面(App.svelte: accounts.length === 0)。
    await addAccount(bridgeA, "e2etestadmin");

    const composeTextarea = await $('[data-testid="compose-textarea"]');
    await composeTextarea.waitForDisplayed({ timeout: 15000 });

    // bridgeAのChromiumを閉じ、固定CDPポート(9333)を解放してからbridgeBを起動する
    // (上記コメント参照: 2つのbridgeを同時生存させない)。
    await bridgeA.teardown();

    // 2アカウント目も同様に一呼吸置いてからbridgeBを起動する。
    await new Promise((resolve) => setTimeout(resolve, 10000));
    bridgeB = await startMiauthBridge({ username: SECOND_USERNAME, password: SECOND_PASSWORD });

    // 2アカウント目: App.svelteは`accounts.length === 0`のときのみAddAccount画面を
    // 自動表示するため、既存動線(設定→アカウント→追加)を使う。設定→アカウントタブ
    // から「＋アカウントを追加」を押すと、Settingsモーダルは自動的に閉じ
    // AddAccount画面(既存の`add-account-host-input`等)がそのまま出る。
    const menuTrigger = await $('[data-testid="app-menu-trigger"]');
    await menuTrigger.waitForDisplayed({ timeout: 15000 });
    await menuTrigger.click();
    const openSettings = await $('[data-testid="app-menu-open-settings"]');
    await openSettings.waitForDisplayed({ timeout: 15000 });
    await openSettings.click();

    const accountsTab = await $('[data-testid="settings-tab-accounts"]');
    await accountsTab.waitForDisplayed({ timeout: 15000 });
    await accountsTab.click();

    const addAccountFromSettings = await $('[data-testid="settings-accounts-add"]');
    await addAccountFromSettings.waitForDisplayed({ timeout: 15000 });
    await addAccountFromSettings.click();

    await addAccount(bridgeB, SECOND_USERNAME);
  });

  it("adds a Home column for each account and keeps their notes separate", async function () {
    this.timeout(60000);
    await browser.setWindowSize(1280, 1024).catch((err) => debugLog("multiAccount", `setWindowSize failed (continuing anyway): ${String(err)}`));

    async function addHomeColumnFor(accountId: string): Promise<void> {
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

      // AddColumnModal内のアカウント選択(AccountSelect)で目的のアカウントを選ぶ。
      // AccountSelectはComposeBar/SearchModalでも使われており、トリガーの
      // data-testid="account-select-trigger"は共通(コンポーネント側は1種類しか
      // 出さない)。スコープせず`$()`で単純に取得すると、DOM順で先に現れる
      // ComposeBar側のトリガー(このモーダルの背後にあり、オーバーレイに覆われて
      // クリックできない)を誤って掴んでしまうことが実機で確認できた
      // ("element click intercepted")。AddColumnModal.svelteに追加した
      // data-testid="add-column-account-select"でスコープし、モーダル自身の
      // トリガーだけを確実に取得する。
      const accountSelectScope = await $('[data-testid="add-column-account-select"]');
      await accountSelectScope.waitForDisplayed({ timeout: 15000 });
      const accountTrigger = await accountSelectScope.$('[data-testid="account-select-trigger"]');
      await accountTrigger.waitForDisplayed({ timeout: 15000 });
      await accountTrigger.waitForClickable({ timeout: 15000 });
      await accountTrigger.click();
      const accountOption = await $(`[data-testid="account-select-option-${accountId}"]`);
      await accountOption.waitForDisplayed({ timeout: 15000 });
      await accountOption.click();

      const addColumnSubmit = await $('[data-testid="add-column-submit"]');
      await addColumnSubmit.waitForDisplayed({ timeout: 15000 });
      await addColumnSubmit.scrollIntoView();
      await addColumnSubmit.waitForClickable({ timeout: 15000 });
      await addColumnSubmit.click();
    }

    // アカウントidはSettings>アカウントタブに並ぶアカウント行のdata-account-row-id属性
    // (AccountsSection.svelte)から拾う。
    const menuTrigger = await $('[data-testid="app-menu-trigger"]');
    await menuTrigger.waitForDisplayed({ timeout: 15000 });
    await menuTrigger.click();
    const openSettings = await $('[data-testid="app-menu-open-settings"]');
    await openSettings.waitForDisplayed({ timeout: 15000 });
    await openSettings.click();
    const accountsTab = await $('[data-testid="settings-tab-accounts"]');
    await accountsTab.waitForDisplayed({ timeout: 15000 });
    await accountsTab.click();

    await browser.waitUntil(
      async () => {
        const rows = await browser.execute(() => document.querySelectorAll<HTMLElement>("[data-account-row-id]").length);
        return rows === 2;
      },
      { timeout: 15000, interval: 300, timeoutMsg: "expected 2 account rows in Settings > accounts" },
    );
    const accountIds = await browser.execute(() => {
      return Array.from(document.querySelectorAll<HTMLElement>("[data-account-row-id]")).map(
        (el) => el.dataset.accountRowId!,
      );
    });
    expect(accountIds.length).toBe(2);
    const [accountIdA, accountIdB] = accountIds;

    // Modal.svelteには`data-testid="modal-close"`のcloseボタンがある(Task 9で追加、
    // client-user-mute.e2e.ts参照)が、ここではEscapeキーでもonclose()が呼べるため
    // それを使う。
    await browser.keys("Escape");

    await addHomeColumnFor(accountIdA);
    await addHomeColumnFor(accountIdB);

    const runId = Date.now();
    const noteFromA = `tsumugi e2e multi-account note from A ${runId}`;

    // UI経由でアカウントAとして投稿する(ComposeBarのAccountSelectをAに合わせてから投稿)。
    const composeAccountTrigger = await $('[data-testid="account-select-trigger"]');
    await composeAccountTrigger.waitForDisplayed({ timeout: 15000 });
    await composeAccountTrigger.click();
    const composeAccountOptionA = await $(`[data-testid="account-select-option-${accountIdA}"]`);
    await composeAccountOptionA.waitForDisplayed({ timeout: 15000 });
    await composeAccountOptionA.click();

    const textarea = await $('[data-testid="compose-textarea"]');
    await textarea.waitForDisplayed({ timeout: 15000 });
    await textarea.setValue(noteFromA);
    const submitButton = await $('[data-testid="compose-submit"]');
    await submitButton.click();

    // アカウントAのカラム(data-account-id=accountIdA)にだけ現れることを確認する。
    const columnA = await $(`[data-account-id="${accountIdA}"]`);
    await browser.waitUntil(
      async () => {
        const noteTexts = await columnA.$$('[data-testid="note-text"]');
        for (const el of noteTexts) {
          const text = await el.getText().catch(() => "");
          if (text.includes(noteFromA)) return true;
        }
        return false;
      },
      { timeout: 20000, interval: 300, timeoutMsg: `note "${noteFromA}" did not appear in account A's column` },
    );

    const columnB = await $(`[data-account-id="${accountIdB}"]`);
    const columnBNoteTexts = await columnB.$$('[data-testid="note-text"]');
    const columnBVisible: string[] = [];
    for (const el of columnBNoteTexts) {
      columnBVisible.push(await el.getText().catch(() => ""));
    }
    expect(columnBVisible.some((t) => t.includes(noteFromA))).toBe(false);
  });
});
