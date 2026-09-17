// アプリを一度正常終了させ、同じ設定ディレクトリで再起動しても構成が復元されることを
// 検証するシナリオの前半(Issue #223)。ここでアカウント追加とカラム追加を行い、
// part2で同じTMP_HOMEを再利用して起動した際に復元されることを確認する。
//
// このspecは"pnpm e2e"(既定の全spec実行、wdio.conf.tsの`specs: ["./specs/**/*.e2e.ts"]`)
// の対象外である`./specs-persistence/`ディレクトリに置かれている(wdio.conf.tsの
// コメント参照: WebdriverIOの`specs`配列はnegationパターンをサポートしないため、
// ディレクトリを分けることで除外している)。E2E_REUSE_HOME_FILEを設定せずに実行すると
// part2が「アカウント追加画面が出ないこと」のアサーションで必ず失敗するため、専用の
// "pnpm e2e:persistence"(package.json)からpart1→part2の順に同一プロセスで
// 実行することを前提とする。
//
// 重要: PERSISTENCE_HOME_FILE(wdio-logs/persistence-home-path.txt)の削除は
// このspecファイルの中(例えばbefore())では行わない。WebdriverIOは
// テストセッション(=run-app.shの起動、TMP_HOMEの確定・ファイルへの書き込み)を
// mochaのbefore()より先に確立するため、before()内でrmSync()すると
// run-app.shが今まさに書いたばかりのTMP_HOMEパスを消してしまい、
// part2が「ファイルが無い」と誤認して新規TMP_HOMEを作ってしまう
// (=永続化シナリオとして意味を成さなくなる)。前回の失敗実行が残した
// ファイルの掃除は、セッションが始まる前に完了させる必要があるため、
// "pnpm e2e:persistence"(package.json)側の"rm -f ..."で行う。
import { readFileSync } from "node:fs";
import { startMiauthBridge, type MiauthBridge } from "../helpers/miauthBridge";
import { clickThroughAccountSelect } from "../helpers/accountSelect";
import { debugLog, debugLogPath } from "../helpers/debugLog";
import { PERSISTENCE_HOME_FILE } from "../helpers/persistenceHome";

const MISSKEY_HOST = "misskey.local:8443";
// part2側で復元後にこのタイトルがタブに表示されていることを照合する
// (カラムの種類・順序・タイトルが失われても検知できるようにするため)。
const NAMED_COLUMN_TITLE = "永続化確認用カラム";

describe("settings persist across restart (part 1: set up)", () => {
  let bridge: MiauthBridge;

  before(async function () {
    this.timeout(30000);
    // run-app.shがこのセッション開始時に確定させたTMP_HOMEパスをログに残す
    // (part2側の同じログと突き合わせれば、同一TMP_HOMEが再利用されたことを
    // 目視確認できる)。
    try {
      debugLog("persistenceRestartPart1", `TMP_HOME (from ${PERSISTENCE_HOME_FILE}): ${readFileSync(PERSISTENCE_HOME_FILE, "utf8")}`);
    } catch (err) {
      debugLog("persistenceRestartPart1", `failed to read PERSISTENCE_HOME_FILE (continuing anyway): ${String(err)}`);
    }
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

  it("adds an account and a Home column, then the session ends normally", async function () {
    this.timeout(90000);

    const hostInput = await $('[data-testid="add-account-host-input"]');
    await hostInput.waitForDisplayed({ timeout: 15000 });
    await hostInput.setValue(MISSKEY_HOST);

    const startButton = await $('[data-testid="add-account-start"]');
    await Promise.all([
      bridge.approveNext(),
      clickThroughAccountSelect(bridge.cdpPort, { logTag: "persistenceRestartPart1", accountLabel: "e2etestadmin" }),
      startButton.click(),
    ]);

    const completeButton = await $('[data-testid="add-account-complete"]');
    await completeButton.waitForDisplayed({ timeout: 15000 });
    await completeButton.click();

    await browser.setWindowSize(1280, 1024).catch((err) => debugLog("persistenceRestartPart1", `setWindowSize failed (continuing anyway): ${String(err)}`));

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

    const composeTextarea = await $('[data-testid="compose-textarea"]');
    await composeTextarea.waitForDisplayed({ timeout: 15000 });

    // 2つ目: TQLエキスパートモード + 明示的なタイトルを設定したカラム。
    // 種類(TQL)・順序(2番目)・タイトルの3点が再起動後も失われていないことを
    // part2側で照合できるようにするため、既定のHomeカラム1つだけでは検知できない
    // (設計書docs/superpowers/specs/2026-09-15-e2e-scenario-coverage-design.md
    // シナリオ5参照)。
    //
    // menuTrigger/addColumnItemを使い回さず、ここで改めて$()し直す。AppMenu.svelteは
    // onAddColumn選択時にメニュー自体を閉じる(=DOMから外れる)ため、1回目に取得した
    // ハンドルは2回目のメニュー操作時点で既に外れたノードを指しており、
    // waitForClickable/clickが"did not become interactable"で失敗する
    // (streaming-events.e2e.tsのaddColumn()ヘルパーが毎回$()し直しているのと同じ理由)。
    const menuTriggerForSecondColumn = await $('[data-testid="app-menu-trigger"]');
    await menuTriggerForSecondColumn.waitForDisplayed({ timeout: 15000 });
    await menuTriggerForSecondColumn.click();
    const addColumnItemForSecondColumn = await $('[data-testid="app-menu-add-column"]');
    await addColumnItemForSecondColumn.waitForDisplayed({ timeout: 15000 });
    await addColumnItemForSecondColumn.click();

    await browser.waitUntil(
      async () => {
        const h1 = await browser.execute(() => window.innerHeight);
        await browser.pause(150);
        const h2 = await browser.execute(() => window.innerHeight);
        return h1 === h2 && h1 > 300;
      },
      { timeout: 10000, interval: 200 },
    );

    // guided/expertタブの切り替え(AddColumnModal.svelte、"エキスパート(TQL)"ボタン。
    // tql-column-filter.e2e.tsと同じ手順)。
    const expertTab = await $("button=エキスパート(TQL)");
    await expertTab.waitForDisplayed({ timeout: 15000 });
    await expertTab.click();

    const nameInput = await $('[data-testid="add-column-name-input"]');
    await nameInput.waitForDisplayed({ timeout: 15000 });
    await nameInput.setValue(NAMED_COLUMN_TITLE);

    const tqlTextarea = await $('[data-testid="add-column-tql-textarea"]');
    await tqlTextarea.waitForDisplayed({ timeout: 15000 });
    await tqlTextarea.setValue('from home where text -> "dummy"');

    const secondAddColumnSubmit = await $('[data-testid="add-column-submit"]');
    await secondAddColumnSubmit.waitForDisplayed({ timeout: 15000 });
    await secondAddColumnSubmit.scrollIntoView();
    await secondAddColumnSubmit.waitForClickable({ timeout: 15000 });
    await secondAddColumnSubmit.click();

    // 2つ目のカラム(グループ)が追加されたことを確認してからセッションを終える
    // (wdioのafterフックがアプリを正常終了させる)。
    await browser.waitUntil(
      async () => {
        const groups = await browser.execute(() => document.querySelectorAll<HTMLElement>("[data-group-id]").length);
        return groups === 2;
      },
      { timeout: 15000, interval: 300, timeoutMsg: "expected 2 column groups after adding the named TQL column" },
    );
  });
});
